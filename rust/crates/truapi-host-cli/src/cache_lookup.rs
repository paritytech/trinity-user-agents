//! Preimage lookup through cache nodes first, then the Bulletin node.
//!
//! A cache node keeps verified copies of Bulletin blobs near its users and serves them faster than a Bulletin node.
//! `TRUAPI_CACHE_PROVIDERS` names the provider set file. Each line is one provider: its endpoint id (64 hex digits),
//! any dial hints, and the base URL of its API (`http://` or `https://`). The host asks only providers with an API URL.
//!
//! For each read the host orders the providers. Providers that failed in the last 30 s go last. The host orders the
//! others by expected latency: the average latency it measured, or a prior for a provider it has not used. The
//! content's home nodes count at half their latency. These are the providers that rank highest by
//! `blake2b-256(content id || endpoint id)`, the rank the cache nodes use too, so reads of the same content from many
//! hosts go to the same nodes. Every fourth read tries an unmeasured provider first.
//!
//! The host pays as an sr25519 payer key: `TRUAPI_CACHE_PAYER_SEED`, else `//allowance//cache//{product}` of the
//! signed-in account. It asks with `POST /acquire` and a read request that the payer signs. The request names the
//! provider, the content, a new transfer id and the time, so only the payer can take reads in its name, and a node
//! serves each request once. The host checks the bytes against the CID, and only then signs a receipt for that
//! delivery and sends it to that provider (`POST /receipt`). A node that sends bad bytes gets no receipt. Without a
//! payer the host does not ask cache nodes. A node that is down, refuses the payer or does not have the blob costs one
//! request, and the Bulletin node answers as it would without cache nodes.
//!
//! The host keeps what it measured in `cache-quality.json` in its state directory, so a restart does not forget which
//! providers are fast. `/cache` shows the payer and the measurements.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::time::Duration;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use schnorrkel::{ExpansionMode, Keypair, MiniSecretKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};
use truapi::host_logic::product_account::derive_sr25519_hard_path;
use truapi::{preimage_cid, preimage_key};
use zeroize::Zeroizing;

use crate::bulletin_lookup::{BlobSource, SourceError};
use crate::frame_server::ProductSelection;

const PROVIDERS_ENV: &str = "TRUAPI_CACHE_PROVIDERS";
const PAYER_SEED_ENV: &str = "TRUAPI_CACHE_PAYER_SEED";
/// Bound on one request to one cache node. A node that does not have the blob reads it from Bulletin first.
const NODE_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How many providers are home nodes of each content id. The cache nodes use the same number.
const HOMES: usize = 3;
/// A provider that failed less than this long ago goes last.
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);
/// The expected latency of a provider without measurements.
const PRIOR_MS: u64 = 50;
/// Every this many reads, an unmeasured provider goes first.
const EXPLORE_EVERY: u64 = 4;
/// The signature contexts and message prefixes of a cache receipt and of a cache read request. The cache nodes check
/// the same layouts (`cache/src/payment.rs`).
const RECEIPT_CONTEXT: &[u8] = b"cache-receipt";
const RECEIPT_PREFIX: &[u8] = b"cache-receipt/2";
const READ_CONTEXT: &[u8] = b"cache-read";
const READ_PREFIX: &[u8] = b"cache-read/1";

/// One provider of the provider set that a host can call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    id: [u8; 32],
    url: String,
}

/// Every provider of a provider set file, with an API URL or without. Only the home-node rank uses the providers
/// without one.
fn parse_providers(text: &str) -> (Vec<[u8; 32]>, Vec<Provider>) {
    let mut ids = Vec::new();
    let mut callable = Vec::new();
    for line in text.lines() {
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        let Some(id) = words.next() else {
            continue;
        };
        let Some(id) = hex::decode(id)
            .ok()
            .and_then(|id| <[u8; 32]>::try_from(id).ok())
        else {
            warn!(id, "provider set: not an endpoint id");
            continue;
        };
        ids.push(id);
        let url = words.find(|word| word.starts_with("http://") || word.starts_with("https://"));
        if let Some(url) = url {
            callable.push(Provider {
                id,
                url: url.trim_end_matches('/').to_string(),
            });
        }
    }
    (ids, callable)
}

/// The home nodes of a content id: the `HOMES` providers that rank highest by `blake2b-256(content id || endpoint id)`.
fn home_nodes(content_id: &str, providers: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut ranked: Vec<([u8; 32], [u8; 32])> = providers
        .iter()
        .map(|id| (preimage_key(&[content_id.as_bytes(), id].concat()), *id))
        .collect();
    ranked.sort_by_key(|(score, _)| core::cmp::Reverse(*score));
    ranked.dedup_by_key(|(_, id)| *id);
    ranked.into_iter().take(HOMES).map(|(_, id)| id).collect()
}

/// What the host measured of one provider.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Quality {
    /// Moving average of the request latency of successful reads.
    latency_ms: Option<u64>,
    successes: u32,
    failures: u32,
    /// The last failure since the last success. It is not saved: its cooldown is short.
    #[serde(skip)]
    failed_at: Option<Instant>,
}

/// The measurements in `path`, by provider. A missing or unreadable file gives none.
fn load_quality(path: &std::path::Path) -> HashMap<[u8; 32], Quality> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let saved: HashMap<String, Quality> = serde_json::from_str(&text).unwrap_or_else(|error| {
        warn!(path = %path.display(), %error, "ignored the saved cache measurements");
        HashMap::new()
    });
    saved
        .into_iter()
        .filter_map(|(id, quality)| {
            let id = <[u8; 32]>::try_from(hex::decode(id).ok()?).ok()?;
            Some((id, quality))
        })
        .collect()
}

fn save_quality(
    path: &std::path::Path,
    quality: &HashMap<[u8; 32], Quality>,
) -> std::io::Result<()> {
    let saved: HashMap<String, &Quality> = quality
        .iter()
        .map(|(id, quality)| (hex::encode(id), quality))
        .collect();
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&saved)?)?;
    std::fs::rename(&temp, path)
}

impl Quality {
    fn record_success(&mut self, latency_ms: u64) {
        self.latency_ms = Some(match self.latency_ms {
            Some(average) => (average * 7 + latency_ms * 3) / 10,
            None => latency_ms,
        });
        self.successes += 1;
        self.failed_at = None;
    }

    fn record_failure(&mut self, now: Instant) {
        self.failures += 1;
        self.failed_at = Some(now);
    }
}

/// The order in which to ask `providers` for content whose home nodes are `homes`. See the module docs.
fn order(
    providers: &[Provider],
    homes: &[[u8; 32]],
    quality: &HashMap<[u8; 32], Quality>,
    explore: bool,
    now: Instant,
) -> Vec<Provider> {
    let measured = |provider: &Provider| quality.get(&provider.id).cloned().unwrap_or_default();
    let failing = |provider: &Provider| {
        measured(provider)
            .failed_at
            .is_some_and(|at| now.duration_since(at) < FAILURE_COOLDOWN)
    };
    let cost = |provider: &Provider| {
        let latency = measured(provider).latency_ms.unwrap_or(PRIOR_MS);
        if homes.contains(&provider.id) {
            latency / 2
        } else {
            latency
        }
    };
    let mut ordered = providers.to_vec();
    ordered.sort_by_key(|provider| (failing(provider), cost(provider)));
    if explore
        && let Some(index) = ordered
            .iter()
            .position(|provider| measured(provider).latency_ms.is_none() && !failing(provider))
    {
        let unmeasured = ordered.remove(index);
        ordered.insert(0, unmeasured);
    }
    ordered
}

/// The fields that a receipt and a read request share, after `prefix`: the transfer id, the payer and provider keys,
/// and the content id. Text fields have a little-endian u32 length first, and keys are their 32 raw bytes.
fn signed_fields(
    prefix: &[u8],
    transfer: &str,
    payer: &[u8; 32],
    provider: &[u8; 32],
    content_id: &str,
) -> Vec<u8> {
    let mut message = prefix.to_vec();
    message.extend_from_slice(&(transfer.len() as u32).to_le_bytes());
    message.extend_from_slice(transfer.as_bytes());
    message.extend_from_slice(payer);
    message.extend_from_slice(provider);
    message.extend_from_slice(&(content_id.len() as u32).to_le_bytes());
    message.extend_from_slice(content_id.as_bytes());
    message
}

/// The bytes that a payer signs for one cache delivery: the shared fields, the service as one byte (0 delivery), and
/// the retention size, `from` and `until` as little-endian u64s. A delivery has no retention, so all three are 0.
fn receipt_message(
    transfer: &str,
    payer: &[u8; 32],
    provider: &[u8; 32],
    content_id: &str,
) -> Vec<u8> {
    let mut message = signed_fields(RECEIPT_PREFIX, transfer, payer, provider, content_id);
    message.push(0);
    message.extend_from_slice(&[0; 24]);
    message
}

/// The bytes that a payer signs to ask one provider for one read: the shared fields and the issue time in Unix
/// seconds as a little-endian u64.
fn read_message(
    transfer: &str,
    payer: &[u8; 32],
    provider: &[u8; 32],
    content_id: &str,
    issued_at: u64,
) -> Vec<u8> {
    let mut message = signed_fields(READ_PREFIX, transfer, payer, provider, content_id);
    message.extend_from_slice(&issued_at.to_le_bytes());
    message
}

/// Where the payer key comes from.
enum PayerSource {
    /// One fixed key from `TRUAPI_CACHE_PAYER_SEED`, for test hosts without an account.
    Seed(Box<Keypair>),
    /// The signed-in account: the payer is `//allowance//cache//{product}` for the product served at each read.
    Account {
        entropy: Zeroizing<Vec<u8>>,
        product: Arc<ProductSelection>,
    },
}

/// Where the provider set comes from.
enum ProviderSet {
    File(PathBuf),
    #[cfg(test)]
    Fixed(Vec<[u8; 32]>, Vec<Provider>),
}

/// The cache nodes of the provider set, what the host measured of them, and the payer.
pub struct CacheNodes {
    http: reqwest::Client,
    providers: ProviderSet,
    quality: Mutex<HashMap<[u8; 32], Quality>>,
    /// Where the measurements are kept across restarts, when the host has a state directory.
    quality_path: Option<PathBuf>,
    payer: Mutex<Option<PayerSource>>,
    reads: AtomicU64,
    warned_no_payer: AtomicBool,
}

impl CacheNodes {
    /// The provider set that `TRUAPI_CACHE_PROVIDERS` names, or `None` when it names none. A payer seed in
    /// `TRUAPI_CACHE_PAYER_SEED` pays for every read; without one, the signed-in account pays once there is one. The
    /// measurements are kept in `quality_path` when it is set.
    pub fn from_env(quality_path: Option<PathBuf>) -> Option<Self> {
        let path = std::env::var_os(PROVIDERS_ENV).filter(|path| !path.is_empty())?;
        let seed = std::env::var(PAYER_SEED_ENV)
            .ok()
            .filter(|seed| !seed.trim().is_empty());
        let payer = match seed.map(|seed| payer_from_seed(&seed)) {
            Some(Ok(payer)) => {
                info!(
                    payer = hex::encode(payer.public.to_bytes()),
                    "cache payer from {PAYER_SEED_ENV}"
                );
                Some(PayerSource::Seed(Box::new(payer)))
            }
            Some(Err(reason)) => {
                warn!(%reason, "{PAYER_SEED_ENV} is not a 32-byte hex seed, so no seed payer");
                None
            }
            None => None,
        };
        info!(providers = ?path, "preimage lookups ask cache nodes before Bulletin");
        Self::new(ProviderSet::File(path.into()), payer, quality_path)
    }

    fn new(
        providers: ProviderSet,
        payer: Option<PayerSource>,
        quality_path: Option<PathBuf>,
    ) -> Option<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(NODE_TIMEOUT)
            .build();
        match http {
            Ok(http) => Some(Self {
                http,
                providers,
                quality: Mutex::new(
                    quality_path
                        .as_deref()
                        .map(load_quality)
                        .unwrap_or_default(),
                ),
                quality_path,
                payer: Mutex::new(payer),
                reads: AtomicU64::new(0),
                warned_no_payer: AtomicBool::new(false),
            }),
            Err(error) => {
                warn!(%error, "no HTTP client for the cache nodes, so lookups go to Bulletin only");
                None
            }
        }
    }

    /// Pay as the signed-in account from now on, unless a payer seed is set. `None` removes the account payer.
    pub fn set_account(&self, account: Option<(&[u8], Arc<ProductSelection>)>) {
        let mut payer = self.payer.lock().expect("cache payer mutex poisoned");
        if matches!(*payer, Some(PayerSource::Seed(_))) {
            return;
        }
        *payer = account.map(|(entropy, product)| PayerSource::Account {
            entropy: Zeroizing::new(entropy.to_vec()),
            product,
        });
    }

    fn payer(&self) -> Option<Keypair> {
        match &*self.payer.lock().expect("cache payer mutex poisoned") {
            Some(PayerSource::Seed(payer)) => Some(Keypair::clone(payer)),
            Some(PayerSource::Account { entropy, product }) => {
                derive_sr25519_hard_path(entropy, &["allowance", "cache", &product.current()]).ok()
            }
            None => None,
        }
    }

    fn load_providers(&self) -> (Vec<[u8; 32]>, Vec<Provider>) {
        match &self.providers {
            ProviderSet::File(path) => match std::fs::read_to_string(path) {
                Ok(text) => parse_providers(&text),
                Err(error) => {
                    warn!(path = %path.display(), %error, "cannot read the provider set");
                    (Vec::new(), Vec::new())
                }
            },
            #[cfg(test)]
            ProviderSet::Fixed(ids, callable) => (ids.clone(), callable.clone()),
        }
    }

    /// The blob under `cid` from the first provider, in quality order, that sends bytes that hash to it.
    async fn read(&self, cid: &str) -> Option<Vec<u8>> {
        let Some(payer) = self.payer() else {
            if !self.warned_no_payer.swap(true, Ordering::Relaxed) {
                warn!(
                    "no cache payer yet (sign in, or set {PAYER_SEED_ENV}), so lookups go to Bulletin only"
                );
            }
            return None;
        };
        let (ids, providers) = self.load_providers();
        let homes = home_nodes(cid, &ids);
        let explore =
            self.reads.fetch_add(1, Ordering::Relaxed) % EXPLORE_EVERY == EXPLORE_EVERY - 1;
        let ordered = {
            let quality = self.quality.lock().expect("cache quality mutex poisoned");
            order(&providers, &homes, &quality, explore, Instant::now())
        };
        for (rank, provider) in ordered.iter().enumerate() {
            let transfer = self.transfer_id();
            let started = Instant::now();
            let answer = self.ask(provider, cid, &payer, &transfer).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let node = provider.url.as_str();
            let mut quality = self.quality.lock().expect("cache quality mutex poisoned");
            let measured = quality.entry(provider.id).or_default();
            match answer {
                Ok(Some((value, origin))) if preimage_cid(&preimage_key(&value)) == cid => {
                    measured.record_success(latency_ms);
                    let home = homes.contains(&provider.id);
                    info!(
                        cid,
                        node,
                        origin,
                        rank,
                        home,
                        latency_ms,
                        size = value.len(),
                        "preimage read from a cache node"
                    );
                    drop(quality);
                    self.save_quality();
                    self.pay(provider, cid, &payer, transfer);
                    return Some(value);
                }
                Ok(Some(_)) => {
                    measured.record_failure(Instant::now());
                    warn!(
                        cid,
                        node,
                        "cache node sent bytes that do not hash to the CID, so it gets no receipt"
                    );
                }
                Ok(None) => debug!(cid, node, "cache node cannot find the preimage"),
                Err(reason) => {
                    if !reason.starts_with("402") {
                        measured.record_failure(Instant::now());
                    }
                    warn!(cid, node, %reason, "cache node failed");
                }
            }
        }
        self.save_quality();
        None
    }

    fn save_quality(&self) {
        let Some(path) = &self.quality_path else {
            return;
        };
        let quality = self
            .quality
            .lock()
            .expect("cache quality mutex poisoned")
            .clone();
        if let Err(error) = save_quality(path, &quality) {
            warn!(path = %path.display(), %error, "could not save the cache measurements");
        }
    }

    /// The payer and, for each provider with an API URL, what the host measured of it: the text of `/cache`.
    pub fn status(&self) -> String {
        let (_, providers) = self.load_providers();
        let payer = self.payer().map_or_else(
            || format!("none (sign in, or set {PAYER_SEED_ENV})"),
            |payer| hex::encode(payer.public.to_bytes()),
        );
        let quality = self.quality.lock().expect("cache quality mutex poisoned");
        let now = Instant::now();
        let mut status = format!("cache payer {payer}\n");
        if providers.is_empty() {
            status.push_str("no provider with an API URL in the provider set\n");
        }
        for provider in &providers {
            let measured = quality.get(&provider.id).cloned().unwrap_or_default();
            let latency = measured
                .latency_ms
                .map_or_else(|| "-".to_string(), |ms| format!("{ms} ms"));
            let failing = measured
                .failed_at
                .is_some_and(|at| now.duration_since(at) < FAILURE_COOLDOWN);
            status.push_str(&format!(
                "{} {}  latency {latency}  reads {}  failures {}{}\n",
                &hex::encode(provider.id)[..10],
                provider.url,
                measured.successes,
                measured.failures,
                if failing { "  failing, asked last" } else { "" },
            ));
        }
        status
    }

    /// One `POST /acquire` with a read request that the payer signs for this provider: the bytes and the source that
    /// the node reports, or `None` when the node answers that Bulletin does not hold the blob.
    async fn ask(
        &self,
        provider: &Provider,
        cid: &str,
        payer: &Keypair,
        transfer: &str,
    ) -> Result<Option<(Vec<u8>, String)>, String> {
        let payer_id = payer.public.to_bytes();
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let message = read_message(transfer, &payer_id, &provider.id, cid, issued_at);
        let signature = payer.sign_simple(READ_CONTEXT, &message);
        let request = json!({
            "reference": { "source": format!("bulletin:{cid}"), "cid": null, "size": null },
            "read": {
                "request": {
                    "transfer": transfer,
                    "payer": hex::encode(payer_id),
                    "provider": hex::encode(provider.id),
                    "content": cid,
                    "issued_at": issued_at,
                },
                "signature": hex::encode(signature.to_bytes()),
            },
        });
        let response = self
            .http
            .post(format!("{}/acquire", provider.url))
            .json(&request)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let origin = response
            .headers()
            .get("x-cache-origin")
            .and_then(|origin| origin.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        let body = response.bytes().await.map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("{status}: {}", String::from_utf8_lossy(&body)));
        }
        Ok(Some((body.to_vec(), origin)))
    }

    /// Sign the receipt for a verified delivery and send it to the provider that served it. The read does not wait:
    /// paying is the provider's concern, and the provider keeps the delivery as unpaid until the receipt arrives.
    fn pay(&self, provider: &Provider, cid: &str, payer: &Keypair, transfer: String) {
        let payer_id = payer.public.to_bytes();
        let message = receipt_message(&transfer, &payer_id, &provider.id, cid);
        let signature = payer.sign_simple(RECEIPT_CONTEXT, &message);
        let receipt = json!({
            "receipt": {
                "transfer": transfer,
                "payer": hex::encode(payer_id),
                "provider": hex::encode(provider.id),
                "content": cid,
                "service": "Delivery",
                "size": 0,
                "from": 0,
                "until": 0,
            },
            "signature": hex::encode(signature.to_bytes()),
        });
        let request = self
            .http
            .post(format!("{}/receipt", provider.url))
            .json(&receipt);
        let node = provider.url.clone();
        tokio::spawn(async move {
            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    let settlement = response.text().await.unwrap_or_default();
                    debug!(node, transfer, settlement, "cache receipt paid");
                }
                Ok(response) => {
                    warn!(node, transfer, status = %response.status(), "cache node refused the receipt")
                }
                Err(error) => warn!(node, transfer, %error, "cache receipt not sent"),
            }
        });
    }

    /// A new transfer id for each request to a node. A node serves a transfer id once, and the ledger of the cache
    /// charges it once.
    fn transfer_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("truapi-{nanos}")
    }
}

/// A payer from a 32-byte hex seed, expanded the way Substrate expands an sr25519 mini secret key.
fn payer_from_seed(seed: &str) -> Result<Keypair, String> {
    let bytes =
        hex::decode(seed.trim().trim_start_matches("0x")).map_err(|error| error.to_string())?;
    let secret = MiniSecretKey::from_bytes(&bytes).map_err(|error| error.to_string())?;
    Ok(secret.expand_to_keypair(ExpansionMode::Ed25519))
}

/// A blob source that asks the cache nodes first, when there are any, and then `fallback`.
pub struct CacheFirst<S> {
    cache: Option<Arc<CacheNodes>>,
    fallback: S,
}

impl<S> CacheFirst<S> {
    /// Ask `cache` before `fallback`. Without cache nodes, `fallback` answers alone.
    pub fn new(cache: Option<Arc<CacheNodes>>, fallback: S) -> Self {
        Self { cache, fallback }
    }
}

#[async_trait]
impl<S: BlobSource> BlobSource for CacheFirst<S> {
    async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
        if let Some(cache) = &self.cache
            && let Some(value) = cache.read(cid).await
        {
            return Ok(Some(value));
        }
        self.fallback.get(cid).await
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    fn blob() -> Vec<u8> {
        b"cache lookup test blob".to_vec()
    }

    fn cid() -> String {
        preimage_cid(&preimage_key(&blob()))
    }

    /// A Bulletin node that always holds `value`, and counts how often it is asked.
    struct Bulletin {
        value: Option<Vec<u8>>,
        calls: Mutex<u32>,
    }

    #[async_trait]
    impl BlobSource for Bulletin {
        async fn get(&self, _cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
            *self.calls.lock().unwrap() += 1;
            Ok(self.value.clone())
        }
    }

    fn bulletin(value: Option<Vec<u8>>) -> Bulletin {
        Bulletin {
            value,
            calls: Mutex::new(0),
        }
    }

    type Requests = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

    /// A cache node on loopback. It answers `POST /acquire` with `status` and `body` and `POST /receipt` with a
    /// settlement, and keeps every request path and body.
    async fn fake_node(status: u16, body: Vec<u8>) -> (String, Requests) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Requests::default();
        let seen = requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (path, request) = read_request(&mut stream).await;
                let (status, body) = match path.as_str() {
                    "/receipt" => (200, b"{\"Charged\":10}".to_vec()),
                    _ => (status, body.clone()),
                };
                seen.lock()
                    .unwrap()
                    .push((path, serde_json::from_slice(&request).unwrap()));
                let head = format!(
                    "HTTP/1.1 {status} Fake\r\ncontent-length: {}\r\nx-cache-origin: local\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
            }
        });
        (url, requests)
    }

    async fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            buffer.extend_from_slice(&chunk[..read]);
            let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&buffer[..end]).to_ascii_lowercase();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map_or(0, |length| length.trim().parse().unwrap());
            while buffer.len() < end + 4 + length {
                let read = stream.read(&mut chunk).await.unwrap();
                buffer.extend_from_slice(&chunk[..read]);
            }
            return (path, buffer[end + 4..end + 4 + length].to_vec());
        }
    }

    fn provider(n: u8, url: &str) -> Provider {
        Provider {
            id: [n; 32],
            url: url.to_string(),
        }
    }

    fn test_payer() -> Keypair {
        payer_from_seed(&hex::encode([1u8; 32])).unwrap()
    }

    fn cache(providers: Vec<Provider>, payer: Option<Keypair>) -> Option<Arc<CacheNodes>> {
        let ids = providers.iter().map(|provider| provider.id).collect();
        CacheNodes::new(
            ProviderSet::Fixed(ids, providers),
            payer.map(|payer| PayerSource::Seed(Box::new(payer))),
            None,
        )
        .map(Arc::new)
    }

    /// The answer of `source` for the test blob. These sources never fail a read: they report a miss instead.
    async fn read<S: BlobSource>(source: &CacheFirst<S>) -> Option<Vec<u8>> {
        match source.get(&cid()).await {
            Ok(value) => value,
            Err(_) => panic!("the read failed instead of a miss"),
        }
    }

    /// The receipt requests a fake node got, once the receipt task had time to send them.
    async fn receipts(requests: &Requests) -> Vec<serde_json::Value> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let requests = requests.lock().unwrap();
        requests
            .iter()
            .filter(|(path, _)| path == "/receipt")
            .map(|(_, body)| body.clone())
            .collect()
    }

    /// Whether `signature` (hex) is the signature of the test payer over `message` in `context`.
    fn signed_by_test_payer(context: &[u8], message: &[u8], signature: &serde_json::Value) -> bool {
        let signature = hex::decode(signature.as_str().unwrap()).unwrap();
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        test_payer()
            .public
            .verify_simple(context, message, &signature)
            .is_ok()
    }

    // The same vectors are in the cache (cache/src/logic.rs and cache/src/payment.rs): hosts and nodes must rank the
    // same home nodes, derive the same payer from a seed and sign the same bytes.
    #[test]
    fn vectors_shared_with_the_cache() {
        let key = "bafk2bzaceb2yf3stdn7wptwblupjbssdhdp2czormoqyiwkcrq35uhvzgcgp4";
        let payer = test_payer().public.to_bytes();
        let fields = "03000000742d31189dac29296d31814dc8c56cf3d36a0543372bba7538fa322a4aebfebc39e056070707070707070707\
                      07070707070707070707070707070707070707070707073e0000006261666b32627a61636562327966337374646e3777\
                      707477626c75706a6273736468647032637a6f726d6f717969776b63727133357568767a6763677034";
        assert_eq!(
            (
                hex::encode(preimage_key(&[key.as_bytes(), &[7; 32]].concat())),
                hex::encode(payer),
                hex::encode(receipt_message("t-1", &payer, &[7; 32], key)),
                hex::encode(read_message("t-1", &payer, &[7; 32], key, 1_791_500_000)),
            ),
            (
                "1b90bef8509cc25b45d94885d8a1eca44bd751212310efc75c9e2628a663b98f".to_string(),
                "189dac29296d31814dc8c56cf3d36a0543372bba7538fa322a4aebfebc39e056".to_string(),
                format!(
                    "63616368652d726563656970742f32{fields}00000000000000000000000000000000000000000000000000"
                ),
                format!("63616368652d726561642f31{fields}e01ec86a00000000"),
            )
        );
    }

    #[test]
    fn provider_set_files() {
        let text = format!(
            "# providers\n{a} 10.0.0.1:4433 http://a.example:8080/\n\n{b}  # no API\nnot-hex http://x\n",
            a = hex::encode([1u8; 32]),
            b = hex::encode([2u8; 32]),
        );
        assert_eq!(
            parse_providers(&text),
            (
                vec![[1; 32], [2; 32]],
                vec![provider(1, "http://a.example:8080")]
            )
        );
    }

    /// Measured latency decides, a home node counts at half its latency, a recent failure goes last, and exploring
    /// puts an unmeasured provider first.
    #[test]
    fn providers_are_ordered_by_quality() {
        let now = Instant::now();
        let providers: Vec<Provider> = (1..=4)
            .map(|n| provider(n, &format!("http://{n}")))
            .collect();
        let latency = |ms| Quality {
            latency_ms: Some(ms),
            ..Default::default()
        };
        let quality = HashMap::from([
            ([1; 32], latency(30)),
            ([2; 32], latency(10)),
            (
                [3; 32],
                Quality {
                    failed_at: Some(now),
                    ..latency(5)
                },
            ),
        ]);
        let ids = |ordered: Vec<Provider>| {
            ordered
                .iter()
                .map(|provider| provider.id[0])
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(order(&providers, &[], &quality, false, now)),
            vec![2, 1, 4, 3]
        );
        assert_eq!(
            ids(order(&providers, &[[1; 32]], &quality, false, now)),
            vec![2, 1, 4, 3]
        );
        // An unmeasured home node counts at 50 / 2 = 25 ms: ahead of 1 (30 ms), behind 2 (10 ms).
        assert_eq!(
            ids(order(&providers, &[[4; 32]], &quality, false, now)),
            vec![2, 4, 1, 3]
        );
        assert_eq!(
            ids(order(&providers, &[], &quality, true, now)),
            vec![4, 2, 1, 3]
        );
        let later = now + FAILURE_COOLDOWN;
        assert_eq!(
            ids(order(&providers, &[], &quality, false, later)),
            vec![3, 2, 1, 4]
        );
    }

    #[test]
    fn latency_is_a_moving_average_and_a_success_ends_a_failure() {
        let mut quality = Quality::default();
        quality.record_success(100);
        quality.record_failure(Instant::now());
        quality.record_success(0);
        assert_eq!(
            (
                quality.latency_ms,
                quality.successes,
                quality.failures,
                quality.failed_at
            ),
            (Some(70), 2, 1, None)
        );
    }

    /// The host asks with a read request that the payer signs for that provider, and pays for what it verified: one
    /// receipt for the same transfer, to the provider that served, signed by the payer.
    #[tokio::test]
    async fn a_cache_hit_is_asked_and_paid_with_signatures_and_bulletin_is_not_asked() {
        let (node, requests) = fake_node(200, blob()).await;
        let source = CacheFirst::new(
            cache(vec![provider(1, &node)], Some(test_payer())),
            bulletin(None),
        );
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 0)
        );
        let receipts = receipts(&requests).await;
        let acquire = requests.lock().unwrap()[0].1["read"].clone();
        let (read, receipt) = (&acquire["request"], &receipts[0]["receipt"]);
        let payer = test_payer().public.to_bytes();
        let transfer = read["transfer"].as_str().unwrap();
        let read_signed = signed_by_test_payer(
            READ_CONTEXT,
            &read_message(
                transfer,
                &payer,
                &[1; 32],
                &cid(),
                read["issued_at"].as_u64().unwrap(),
            ),
            &acquire["signature"],
        );
        let receipt_signed = signed_by_test_payer(
            RECEIPT_CONTEXT,
            &receipt_message(transfer, &payer, &[1; 32], &cid()),
            &receipts[0]["signature"],
        );
        assert_eq!(
            (
                receipts.len(),
                read_signed,
                receipt_signed,
                [&read["provider"], &read["content"], &read["payer"]],
                [
                    &receipt["transfer"],
                    &receipt["provider"],
                    &receipt["content"]
                ],
                [
                    &receipt["service"],
                    &receipt["size"],
                    &receipt["from"],
                    &receipt["until"]
                ],
            ),
            (
                1,
                true,
                true,
                [
                    &json!(hex::encode([1u8; 32])),
                    &json!(cid()),
                    &json!(hex::encode(payer))
                ],
                [
                    &json!(transfer),
                    &json!(hex::encode([1u8; 32])),
                    &json!(cid())
                ],
                [&json!("Delivery"), &json!(0), &json!(0), &json!(0)],
            )
        );
    }

    /// A cache node is not trusted: bytes that do not hash to the CID are dropped and not paid, and the next node is
    /// asked.
    #[tokio::test]
    async fn forged_bytes_get_no_receipt_and_the_next_node_is_asked() {
        let (liar, liar_requests) = fake_node(200, b"forged".to_vec()).await;
        let (honest, honest_requests) = fake_node(200, blob()).await;
        let source = CacheFirst::new(
            cache(
                vec![provider(1, &liar), provider(2, &honest)],
                Some(test_payer()),
            ),
            bulletin(None),
        );
        // The liar measured faster, so the host asks it first.
        if let Some(cache) = &source.cache {
            let mut quality = cache.quality.lock().unwrap();
            for (id, latency_ms) in [([1; 32], 1), ([2; 32], 1000)] {
                quality.insert(
                    id,
                    Quality {
                        latency_ms: Some(latency_ms),
                        ..Default::default()
                    },
                );
            }
        }
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 0)
        );
        assert_eq!(
            (
                receipts(&liar_requests).await.len(),
                receipts(&honest_requests).await.len()
            ),
            (0, 1)
        );
    }

    /// Cache nodes only make a read faster: when none of them can help, Bulletin answers as without them.
    #[tokio::test]
    async fn nodes_that_miss_refuse_or_are_down_leave_the_read_to_bulletin() {
        let (missing, _) = fake_node(404, b"not found: not held".to_vec()).await;
        let (refusing, _) = fake_node(402, b"refused: payer has 0 credits".to_vec()).await;
        let providers = vec![
            provider(1, &missing),
            provider(2, &refusing),
            provider(3, "http://127.0.0.1:9"),
        ];
        let source = CacheFirst::new(cache(providers, Some(test_payer())), bulletin(Some(blob())));
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 1)
        );
    }

    /// A host that cannot pay does not take service from a cache node.
    #[tokio::test]
    async fn without_a_payer_cache_nodes_are_not_asked() {
        let (node, requests) = fake_node(200, blob()).await;
        let source = CacheFirst::new(
            cache(vec![provider(1, &node)], None),
            bulletin(Some(blob())),
        );
        assert_eq!(
            (
                read(&source).await,
                *source.fallback.calls.lock().unwrap(),
                requests.lock().unwrap().len()
            ),
            (Some(blob()), 1, 0)
        );
    }

    /// A restart keeps which providers are fast and how often they failed, but not a recent failure.
    #[tokio::test]
    async fn measurements_survive_a_restart_and_show_in_the_status() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache-quality.json");
        let (node, _) = fake_node(200, blob()).await;
        let nodes = |path: &std::path::Path| {
            CacheNodes::new(
                ProviderSet::Fixed(vec![[1; 32]], vec![provider(1, &node)]),
                Some(PayerSource::Seed(Box::new(test_payer()))),
                Some(path.to_path_buf()),
            )
            .unwrap()
        };
        let first = nodes(&path);
        assert_eq!(first.read(&cid()).await, Some(blob()));
        first
            .quality
            .lock()
            .unwrap()
            .get_mut(&[1; 32])
            .unwrap()
            .record_failure(Instant::now());
        first.save_quality();

        let restarted = nodes(&path);
        let measured = restarted.quality.lock().unwrap()[&[1; 32]].clone();
        assert_eq!(
            (
                measured.latency_ms.is_some(),
                measured.successes,
                measured.failures,
                measured.failed_at
            ),
            (true, 1, 1, None)
        );
        let status = restarted.status();
        assert!(
            status.contains(&node) && status.contains("reads 1  failures 1"),
            "{status}"
        );
        let payer = hex::encode(test_payer().public.to_bytes());
        assert!(
            status.starts_with(&format!("cache payer {payer}")),
            "{status}"
        );
    }

    #[tokio::test]
    async fn without_cache_nodes_bulletin_answers_alone() {
        let source = CacheFirst::new(None, bulletin(Some(blob())));
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 1)
        );
    }
}
