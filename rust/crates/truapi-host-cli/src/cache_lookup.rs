//! Preimage reads through cache nodes first, then through the Bulletin node.
//!
//! A cache node keeps verified copies of Bulletin blobs near its users. It serves them faster than
//! a Bulletin node. `TRUAPI_CACHE_PROVIDERS` names the provider set file. Each line is one
//! provider: its endpoint id (64 hex digits), dial hints, and the base URL of its API (`http://` or
//! `https://`). The host asks only the providers that have an API URL.
//!
//! For each read, the host puts the providers in order. Providers that failed in the last 30 s go
//! last. The host sorts the other providers by expected latency. This is the average latency that
//! the host measured, or a prior value for a provider that the host did not use yet. The home nodes
//! of the content count at half their latency. The home nodes are the providers with the highest
//! `blake2b-256(content id || endpoint id)`. The cache nodes use the same rank, so reads of the
//! same content from many hosts go to the same nodes. On every fourth read, an unmeasured provider
//! goes first.
//!
//! The host pays with an sr25519 payer key: `TRUAPI_CACHE_PAYER_SEED`, else
//! `//allowance//cache//{product}` of the signed-in account. The host sends `POST /acquire` with a
//! read request that the payer signs. The request names the provider, the content, a new transfer
//! id and the time. Thus only the payer can ask for reads in its name, and a node serves each
//! request once. The host checks the bytes against the CID. Only then does it sign a receipt for
//! that delivery and send it to that provider (`POST /receipt`). A node that sends bad bytes gets
//! no receipt. Without a payer, the host does not ask cache nodes. A node that is down, refuses the
//! payer or does not have the blob costs one request. The Bulletin node then answers as it does
//! without cache nodes.
//!
//! `TRUAPI_CACHE_REGISTRY` gives the provider set from the provider registry contract on Asset
//! Hub instead of a file: the address of the contract, read through the Asset Hub of the network,
//! or `TRUAPI_CACHE_REGISTRY_RPC` (see `registry`). The two variables cannot be used together.
//!
//! The host keeps its measurements in `cache-quality.json` in its state directory, so a restart
//! does not lose them. `/cache` shows the payer and the measurements.
//!
//! A retention keeps a preimage key, a CID, or the CID in the contenthash of a DotNS name (see
//! `content`). For a dag-pb CID, such as the root of a bulletin-deploy site, the node keeps every
//! block of the DAG, and the size is the size of all blocks.
//!
//! The list of retentions comes from the ledger of the cache. The payer signs a request for its
//! retentions, and one provider sends it to the ledger (`POST /leases`). Then the host asks the
//! provider of each retention for the state of the content (`GET /pin-state/<content>`).
//!
//! An item that renews is a lease. The payer signs one lease authorization with the most that it
//! pays for one day, and the provider charges each day itself. The lease ends at its end, or when
//! the payer stops it: the payer signs a stop request, and the provider sends it to the ledger
//! (`POST /leases/stop`).

use core::{
	sync::atomic::{AtomicBool, AtomicU64, Ordering},
	time::Duration,
};
use std::{
	collections::HashMap,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{Instant, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use schnorrkel::{ExpansionMode, Keypair, MiniSecretKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};
use truapi::{
	host_logic::product_account::derive_sr25519_hard_path,
	latest::{
		CacheProviderInfo, CacheProviderSource, PreimageReadError, PreimageReadRoute,
		PreimageRetainError, PreimageRetention, PreimageTarget, RemotePreimageCacheAccountResponse,
		RemotePreimageCacheProvidersResponse, RemotePreimageReadResponse,
		RemotePreimageRetainRequest, RemotePreimageRetainResponse,
		RemotePreimageRetentionsResponse, RemotePreimageStopRetentionRequest,
		RemotePreimageStopRetentionResponse, RetainItem, RetainKind, RetainOutcome, RetainState,
		RetentionState,
	},
	preimage_cid, preimage_key,
	v01::{
		BulletinReadVia, CacheOrigin, PreimageReadAttempt, PreimageReadAttemptSource,
		PreimageReadOutcome, PreimageReadReport, PreimageReadSource,
	},
};
use zeroize::Zeroizing;

use self::content::Cid;
use crate::{
	bulletin_lookup::{BlobSource, SourceError},
	frame_server::ProductSelection,
};

mod content;
mod registry;

const PROVIDERS_ENV: &str = "TRUAPI_CACHE_PROVIDERS";
const REGISTRY_ENV: &str = "TRUAPI_CACHE_REGISTRY";
const REGISTRY_RPC_ENV: &str = "TRUAPI_CACHE_REGISTRY_RPC";
const PAYER_SEED_ENV: &str = "TRUAPI_CACHE_PAYER_SEED";
/// The time limit of one request to one cache node. A node that does not have the blob reads it
/// from Bulletin first, so the limit is long.
const NODE_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The number of home nodes for each content id. The cache nodes use the same number.
const HOMES: usize = 3;
/// A provider that failed less than this time ago goes last.
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);
/// The expected latency of a provider without measurements.
const PRIOR_MS: u64 = 50;
/// An unmeasured provider goes first once in this number of reads.
const EXPLORE_EVERY: u64 = 4;
/// The sr25519 signing context of Substrate. Browser and wallet libraries use it too. A receipt and
/// a read request have different prefixes, so a signature for one is not valid for the other. The
/// cache nodes check the same layouts (`cache/src/payment.rs`).
const SIGNING_CONTEXT: &[u8] = b"substrate";
const RECEIPT_PREFIX: &[u8] = b"cache-receipt/2";
const LEASE_PREFIX: &[u8] = b"cache-receipt/3";
const STOP_PREFIX: &[u8] = b"cache-stop/1";
const READ_PREFIX: &[u8] = b"cache-read/1";
const LEASES_PREFIX: &[u8] = b"cache-leases/1";
/// The time limit of one request for the state of one retention.
const PIN_STATE_TIMEOUT: Duration = Duration::from_secs(5);

/// One provider of the provider set that a host can call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
	id: [u8; 32],
	url: String,
	/// The display name from `name=<text>` on the provider's line, if any.
	name: Option<String>,
	/// The region from `region=<text>` on the provider's line, if any.
	region: Option<String>,
}

/// The providers of a provider set file: all endpoint ids, and the providers with an API URL. Only
/// the home-node rank uses the providers without an API URL. After the endpoint id, a line can hold
/// dial hints, an API URL, `name=<text>` and `region=<text>`, in any order.
fn parse_providers(text: &str) -> (Vec<[u8; 32]>, Vec<Provider>) {
	let mut ids = Vec::new();
	let mut callable = Vec::new();
	for line in text.lines() {
		let mut words = line.split('#').next().unwrap_or("").split_whitespace();
		let Some(id) = words.next() else {
			continue;
		};
		let Some(id) = hex::decode(id).ok().and_then(|id| <[u8; 32]>::try_from(id).ok()) else {
			warn!(id, "provider set: not an endpoint id");
			continue;
		};
		ids.push(id);
		let words: Vec<&str> = words.collect();
		let url = words
			.iter()
			.find(|word| word.starts_with("http://") || word.starts_with("https://"));
		let named = |prefix: &str| {
			words
				.iter()
				.find_map(|word| word.strip_prefix(prefix))
				.filter(|value| !value.is_empty())
				.map(str::to_string)
		};
		if let Some(url) = url {
			callable.push(Provider {
				id,
				url: url.trim_end_matches('/').to_string(),
				name: named("name="),
				region: named("region="),
			});
		}
	}
	(ids, callable)
}

/// Where a cache node got the content, from its `x-cache-origin` header. The header is `local`,
/// `source`, or `peer:` and the 64 hex digits of the endpoint id of the peer. Other text gives
/// `Unknown`.
fn parse_origin(origin: &str) -> CacheOrigin {
	match origin {
		"local" => CacheOrigin::Local,
		"source" => CacheOrigin::Source,
		_ => origin
			.strip_prefix("peer:")
			.and_then(|id| hex::decode(id).ok())
			.and_then(|id| <[u8; 32]>::try_from(id).ok())
			.map_or(CacheOrigin::Unknown, |id| CacheOrigin::Peer { id }),
	}
}

/// Milliseconds since `started`, at most `u32::MAX`.
fn elapsed_ms(started: Instant) -> u32 {
	u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// What a cache node answered to one `POST /acquire` that it served.
struct Answer {
	value: Vec<u8>,
	/// The `x-cache-origin` header, or `unknown` when the response does not have it.
	origin: String,
	/// The `x-cache-elapsed-ms` header: the time of the node's own work.
	provider_ms: Option<u32>,
	/// The `x-cache-trace` header: the node's steps, as JSON.
	trace: Option<String>,
	/// The `x-cache-price` header: the price of the read in the node's ledger.
	price: Option<u64>,
}

/// The account of a payer that a cache node gives at `GET /account/<payer>`: the quote of its
/// ledger. A ledger without allowances leaves the allowance fields out. A ledger without price
/// units has one price for each read and MiB-hours of retention.
#[derive(Deserialize)]
struct Quote {
	price: u64,
	#[serde(default)]
	read_price_per_mib: u64,
	retention_price: u64,
	#[serde(default = "hour_secs")]
	retention_unit_secs: u64,
	#[serde(default)]
	allowance: u64,
	#[serde(default)]
	allowance_retention: u64,
	#[serde(default)]
	paid: Option<u64>,
	balance: u64,
	#[serde(default)]
	allowance_per_day: u64,
	#[serde(default)]
	retention_allowance_per_day: u64,
}

fn hour_secs() -> u64 {
	HOUR_SECS
}

/// What a cache node answers to `POST /pin`. For a lease, `until` is the end of the paid day.
#[derive(Deserialize)]
struct PinReply {
	until: u64,
	size: u64,
	charged: u64,
	#[serde(default)]
	chunks: u32,
}

/// What a cache node answers to `POST /leases`: the retentions of the payer in the ledger, the
/// latest end first.
#[derive(Deserialize)]
struct Leases {
	leases: Vec<Lease>,
}

/// One retention in the ledger: the content, the endpoint id of its provider (hex), the window, the
/// sum of its charges, and for a lease the price of one day and whether it renews. A ledger without
/// leases leaves `paid_until`, `per_day` and `renews` out.
#[derive(Deserialize)]
struct Lease {
	content: String,
	provider: String,
	from: u64,
	until: u64,
	size: u64,
	charged: u64,
	#[serde(default)]
	paid_until: u64,
	#[serde(default)]
	per_day: u64,
	#[serde(default)]
	renews: bool,
}

impl Lease {
	/// The end of the paid time. A ledger without `paid_until` has only retention paid in advance,
	/// which is paid until its end.
	fn paid_until(&self) -> u64 {
		if self.paid_until == 0 { self.until } else { self.paid_until }
	}
}

/// What a cache node answers to `POST /leases/stop`: the number of leases that stopped.
#[derive(Deserialize)]
struct Stopped {
	stopped: u64,
}

/// What a cache node answers to `GET /pin-state/<content>`: the state of the content (`kept`,
/// `fetching`, `lost`, `ended` or `none`), the blocks that its pins name, and the pieces of a
/// fetch.
#[derive(Deserialize)]
struct PinState {
	state: String,
	#[serde(default)]
	blocks: u64,
	#[serde(default)]
	pieces: u64,
	#[serde(default)]
	total_pieces: u64,
}

/// The content of one retention: its CID text, its size, its kind and its number of pieces.
struct Content {
	cid: String,
	size: u64,
	kind: RetainKind,
	pieces: u32,
}

/// The longest wait for a pin: the node fetches the content first, for a chunked file each chunk.
const PIN_TIMEOUT: Duration = Duration::from_secs(300);
const MIB: u64 = 1 << 20;
const HOUR_SECS: u64 = 3600;
const DAY_SECS: u64 = 86_400;

/// What the payer signs for a pin: the window, and for a lease the most that it pays for one day.
struct Terms {
	/// `from` and `until`. A lease with `until` 0 renews until the payer stops it.
	window: (u64, u64),
	/// `Some` for a lease, which the provider charges one day at a time. `None` for retention that
	/// the payer pays in advance.
	max_per_day: Option<u64>,
}

impl Terms {
	/// The end of the time that the first charge pays: the window of a retention, or the first day
	/// of a lease.
	fn paid_until(&self) -> u64 {
		let (from, until) = self.window;
		match self.max_per_day {
			None => until,
			Some(_) if until == 0 => from.saturating_add(DAY_SECS),
			Some(_) => from.saturating_add(DAY_SECS).min(until),
		}
	}
}

/// One read through the cache nodes: the value, the provider that served it, and each provider that
/// the host asked.
struct CacheRead {
	value: Option<Vec<u8>>,
	served_by: Option<PreimageReadSource>,
	attempts: Vec<PreimageReadAttempt>,
}

/// The home nodes of a content id: the `HOMES` providers with the highest
/// `blake2b-256(content id || endpoint id)`.
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
	/// The average request latency of successful reads, with more weight on recent reads.
	latency_ms: Option<u64>,
	successes: u32,
	failures: u32,
	/// The last failure after the last success. The file does not keep it, because its cooldown is
	/// short.
	#[serde(skip)]
	failed_at: Option<Instant>,
}

/// The measurements in `path`, by provider. A file that is not there or not valid gives no
/// measurements.
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
	let saved: HashMap<String, &Quality> =
		quality.iter().map(|(id, quality)| (hex::encode(id), quality)).collect();
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

/// The order in which the host asks `providers` for content with the home nodes `homes`. The module
/// documentation gives the rules.
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
		if homes.contains(&provider.id) { latency / 2 } else { latency }
	};
	let mut ordered = providers.to_vec();
	ordered.sort_by_key(|provider| (failing(provider), cost(provider)));
	if explore &&
		let Some(index) = ordered
			.iter()
			.position(|provider| measured(provider).latency_ms.is_none() && !failing(provider))
	{
		let unmeasured = ordered.remove(index);
		ordered.insert(0, unmeasured);
	}
	ordered
}

/// The fields that a receipt and a read request share, after `prefix`: the transfer id, the payer
/// and provider keys, and the content id. A text field starts with its length as a little-endian
/// u32. A key is its 32 raw bytes.
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

/// The bytes that a payer signs for one cache delivery. They are the shared fields and the service
/// as one byte (0 for a delivery). Then come the size that the payer checked, `from` and `until` as
/// little-endian u64s. A delivery has no window, so `from` and `until` are 0.
fn receipt_message(
	transfer: &str,
	payer: &[u8; 32],
	provider: &[u8; 32],
	content_id: &str,
	size: u64,
) -> Vec<u8> {
	let mut message = signed_fields(RECEIPT_PREFIX, transfer, payer, provider, content_id);
	message.push(0);
	message.extend_from_slice(&size.to_le_bytes());
	message.extend_from_slice(&[0; 16]);
	message
}

/// The bytes that a payer signs to pay in advance for retention: the layout of a receipt, with the
/// service byte 1, the size that the retention pays for, and the window `from` to `until`.
fn retention_message(
	transfer: &str,
	payer: &[u8; 32],
	provider: &[u8; 32],
	content_id: &str,
	size: u64,
	window: (u64, u64),
) -> Vec<u8> {
	let mut message = signed_fields(RECEIPT_PREFIX, transfer, payer, provider, content_id);
	message.push(1);
	message.extend_from_slice(&size.to_le_bytes());
	message.extend_from_slice(&window.0.to_le_bytes());
	message.extend_from_slice(&window.1.to_le_bytes());
	message
}

/// The bytes that a payer signs for a lease: the layout of a retention with its own prefix and the
/// service byte 2, then the most that the payer pays for one day as a little-endian u64. The window
/// starts at day 0 and ends at the end of the lease, or at 0 for a lease that renews.
fn lease_message(
	transfer: &str,
	payer: &[u8; 32],
	provider: &[u8; 32],
	content_id: &str,
	size: u64,
	window: (u64, u64),
	max_per_day: u64,
) -> Vec<u8> {
	let mut message = signed_fields(LEASE_PREFIX, transfer, payer, provider, content_id);
	message.push(2);
	message.extend_from_slice(&size.to_le_bytes());
	message.extend_from_slice(&window.0.to_le_bytes());
	message.extend_from_slice(&window.1.to_le_bytes());
	message.extend_from_slice(&max_per_day.to_le_bytes());
	message
}

/// The bytes that a payer signs to stop its leases of one content on one provider: the prefix, the
/// payer and provider keys, the content id with its length as a little-endian u32, and the issue
/// time in Unix seconds as a little-endian u64.
fn stop_message(
	payer: &[u8; 32],
	provider: &[u8; 32],
	content_id: &str,
	issued_at: u64,
) -> Vec<u8> {
	let mut message = STOP_PREFIX.to_vec();
	message.extend_from_slice(payer);
	message.extend_from_slice(provider);
	message.extend_from_slice(&(content_id.len() as u32).to_le_bytes());
	message.extend_from_slice(content_id.as_bytes());
	message.extend_from_slice(&issued_at.to_le_bytes());
	message
}

/// The bytes that a payer signs to ask one provider for one read. They are the shared fields and
/// the issue time in Unix seconds, as a little-endian u64.
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

/// The bytes that a payer signs to ask the ledger for its retentions: the prefix, the payer key and
/// the issue time in Unix seconds, as a little-endian u64.
fn leases_message(payer: &[u8; 32], issued_at: u64) -> Vec<u8> {
	let mut message = LEASES_PREFIX.to_vec();
	message.extend_from_slice(payer);
	message.extend_from_slice(&issued_at.to_le_bytes());
	message
}

/// The source of the payer key.
enum PayerSource {
	/// One fixed key from `TRUAPI_CACHE_PAYER_SEED`, for test hosts without an account.
	Seed(Box<Keypair>),
	/// The signed-in account. The payer is `//allowance//cache//{product}` for the product that the
	/// host serves at each read.
	Account { entropy: Zeroizing<Vec<u8>>, product: Arc<ProductSelection> },
}

/// The source of the provider set.
enum ProviderSet {
	File(PathBuf),
	Registry(Arc<registry::Registry>),
	#[cfg(test)]
	Fixed(Vec<[u8; 32]>, Vec<Provider>),
}

/// The cache nodes of the provider set, what the host measured of them, and the payer.
pub struct CacheNodes {
	http: reqwest::Client,
	providers: ProviderSet,
	quality: Mutex<HashMap<[u8; 32], Quality>>,
	/// The file that keeps the measurements across restarts, when the host has a state directory.
	quality_path: Option<PathBuf>,
	payer: Mutex<Option<PayerSource>>,
	/// The HTTP JSON-RPC URL of the Asset Hub that holds DotNS.
	asset_hub: Option<String>,
	reads: AtomicU64,
	warned_no_payer: AtomicBool,
}

impl CacheNodes {
	/// The cache nodes of the provider set that `TRUAPI_CACHE_PROVIDERS` names, or of the registry
	/// that `TRUAPI_CACHE_REGISTRY` names, read through `asset_hub_rpc` unless
	/// `TRUAPI_CACHE_REGISTRY_RPC` is set. The answer is `None` when neither variable is set, both
	/// are set, or the HTTP client fails. A payer seed in `TRUAPI_CACHE_PAYER_SEED` pays for every
	/// read. Without a seed, the signed-in account pays when there is one. The host keeps the
	/// measurements in `quality_path` when it is set. The host resolves DotNS names through
	/// `asset_hub_rpc`.
	pub fn from_env(quality_path: Option<PathBuf>, asset_hub_rpc: &str) -> Option<Self> {
		let set = |name: &str| std::env::var(name).ok().filter(|value| !value.trim().is_empty());
		let providers = match (set(PROVIDERS_ENV), set(REGISTRY_ENV)) {
			(None, None) => return None,
			(Some(_), Some(_)) => {
				warn!(
					"set {PROVIDERS_ENV} or {REGISTRY_ENV}, not both, so lookups go to Bulletin only"
				);
				return None;
			},
			(Some(path), None) => {
				info!(providers = path, "preimage lookups ask cache nodes before Bulletin");
				ProviderSet::File(path.into())
			},
			(None, Some(contract)) => {
				let rpc = set(REGISTRY_RPC_ENV).unwrap_or_else(|| asset_hub_rpc.to_string());
				match registry::Registry::new(&contract, &rpc) {
					Ok(registry) => {
						let registry = Arc::new(registry);
						info!(
							"preimage lookups ask the cache nodes of the {}",
							registry.describe()
						);
						registry.start();
						ProviderSet::Registry(registry)
					},
					Err(error) => {
						warn!(%error, "{REGISTRY_ENV}: lookups go to Bulletin only");
						return None;
					},
				}
			},
		};
		let seed = std::env::var(PAYER_SEED_ENV).ok().filter(|seed| !seed.trim().is_empty());
		let payer = match seed.map(|seed| payer_from_seed(&seed)) {
			Some(Ok(payer)) => {
				info!(
					payer = hex::encode(payer.public.to_bytes()),
					"cache payer from {PAYER_SEED_ENV}"
				);
				Some(PayerSource::Seed(Box::new(payer)))
			},
			Some(Err(reason)) => {
				warn!(%reason, "{PAYER_SEED_ENV} is not a 32-byte hex seed, so no seed payer");
				None
			},
			None => None,
		};
		let asset_hub =
			Some(asset_hub_rpc.trim()).filter(|rpc| !rpc.is_empty()).map(registry::http_url);
		Self::new(providers, payer, quality_path, asset_hub)
	}

	fn new(
		providers: ProviderSet,
		payer: Option<PayerSource>,
		quality_path: Option<PathBuf>,
		asset_hub: Option<String>,
	) -> Option<Self> {
		let http = reqwest::Client::builder()
			.connect_timeout(CONNECT_TIMEOUT)
			.timeout(NODE_TIMEOUT)
			.build();
		match http {
			Ok(http) => Some(Self {
				http,
				providers,
				quality: Mutex::new(quality_path.as_deref().map(load_quality).unwrap_or_default()),
				quality_path,
				payer: Mutex::new(payer),
				asset_hub,
				reads: AtomicU64::new(0),
				warned_no_payer: AtomicBool::new(false),
			}),
			Err(error) => {
				warn!(%error, "no HTTP client for the cache nodes, so lookups go to Bulletin only");
				None
			},
		}
	}

	/// Pay as the signed-in account from now on, unless a payer seed is set. `None` removes the
	/// account payer.
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
			},
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
				},
			},
			ProviderSet::Registry(registry) => {
				let state = registry.state();
				(state.ids, state.providers)
			},
			#[cfg(test)]
			ProviderSet::Fixed(ids, callable) => (ids.clone(), callable.clone()),
		}
	}

	/// The blob under `cid` from the first provider, in quality order, that sends bytes that hash
	/// to it. Without a payer the answer is `None`, and the host gives one warning.
	async fn read(&self, cid: &str) -> Option<Vec<u8>> {
		if self.payer().is_none() {
			if !self.warned_no_payer.swap(true, Ordering::Relaxed) {
				warn!(
					"no cache payer yet (sign in, or set {PAYER_SEED_ENV}), so lookups go to Bulletin only"
				);
			}
			return None;
		}
		self.read_reported(cid, None).await.ok().and_then(|read| read.value)
	}

	/// One pass through the cache nodes in quality order, or through the provider `only`. The
	/// answer has the value from the first provider that sends bytes that hash to `cid`. It also
	/// has an attempt for each provider that the host asked. The host pays the provider that
	/// served. Without a payer or a callable provider, the answer is `NoCacheProviders`. An `only`
	/// outside the provider set gives `UnknownProvider`.
	async fn read_reported(
		&self,
		cid: &str,
		only: Option<[u8; 32]>,
	) -> Result<CacheRead, PreimageReadError> {
		let Some(payer) = self.payer() else {
			return Err(PreimageReadError::NoCacheProviders);
		};
		let (ids, providers) = self.load_providers();
		if providers.is_empty() {
			return Err(PreimageReadError::NoCacheProviders);
		}
		let homes = home_nodes(cid, &ids);
		let explore =
			self.reads.fetch_add(1, Ordering::Relaxed) % EXPLORE_EVERY == EXPLORE_EVERY - 1;
		let mut ordered = {
			let quality = self.quality.lock().expect("cache quality mutex poisoned");
			order(&providers, &homes, &quality, explore, Instant::now())
		};
		if let Some(only) = only {
			ordered.retain(|provider| provider.id == only);
			if ordered.is_empty() {
				return Err(PreimageReadError::UnknownProvider);
			}
		}
		let mut attempts = Vec::new();
		for (rank, provider) in ordered.iter().enumerate() {
			let transfer = self.transfer_id();
			let started = Instant::now();
			let answer = self.ask(provider, cid, &payer, &transfer).await;
			let latency_ms = started.elapsed().as_millis() as u64;
			let ms = elapsed_ms(started);
			let node = provider.url.as_str();
			let mut quality = self.quality.lock().expect("cache quality mutex poisoned");
			let measured = quality.entry(provider.id).or_default();
			let outcome = match answer {
				Ok(Some(answer)) if preimage_cid(&preimage_key(&answer.value)) == cid => {
					measured.record_success(latency_ms);
					let home = homes.contains(&provider.id);
					info!(
						cid,
						node,
						origin = answer.origin,
						rank,
						home,
						latency_ms,
						size = answer.value.len(),
						"preimage read from a cache node"
					);
					drop(quality);
					self.save_quality();
					self.pay(provider, cid, &payer, transfer, answer.value.len() as u64);
					attempts.push(PreimageReadAttempt {
						source: PreimageReadAttemptSource::CacheProvider { id: provider.id },
						outcome: PreimageReadOutcome::Served,
						ms,
					});
					return Ok(CacheRead {
						served_by: Some(PreimageReadSource::CacheProvider {
							id: provider.id,
							name: provider.name.clone(),
							region: provider.region.clone(),
							origin: parse_origin(&answer.origin),
							rank: u32::try_from(rank).unwrap_or(u32::MAX),
							home,
							provider_ms: answer.provider_ms,
							trace: answer.trace,
							price: answer.price,
						}),
						value: Some(answer.value),
						attempts,
					});
				},
				Ok(Some(_)) => {
					measured.record_failure(Instant::now());
					warn!(
						cid,
						node,
						"cache node sent bytes that do not hash to the CID, so it gets no receipt"
					);
					PreimageReadOutcome::BadBytes
				},
				Ok(None) => {
					debug!(cid, node, "cache node cannot find the preimage");
					PreimageReadOutcome::Miss
				},
				Err(reason) => {
					warn!(cid, node, %reason, "cache node failed");
					if reason.starts_with("402") {
						PreimageReadOutcome::Refused { reason }
					} else {
						measured.record_failure(Instant::now());
						PreimageReadOutcome::Failed { reason }
					}
				},
			};
			attempts.push(PreimageReadAttempt {
				source: PreimageReadAttemptSource::CacheProvider { id: provider.id },
				outcome,
				ms,
			});
		}
		self.save_quality();
		Ok(CacheRead { value: None, served_by: None, attempts })
	}

	fn save_quality(&self) {
		let Some(path) = &self.quality_path else {
			return;
		};
		let quality = self.quality.lock().expect("cache quality mutex poisoned").clone();
		if let Err(error) = save_quality(path, &quality) {
			warn!(path = %path.display(), %error, "could not save the cache measurements");
		}
	}

	/// The text of `/cache`: the payer, and the measurements of each provider with an API URL.
	pub fn status(&self) -> String {
		let (_, providers) = self.load_providers();
		let payer = self.payer().map_or_else(
			|| format!("none (sign in, or set {PAYER_SEED_ENV})"),
			|payer| hex::encode(payer.public.to_bytes()),
		);
		let quality = self.quality.lock().expect("cache quality mutex poisoned");
		let now = Instant::now();
		let mut status = format!("cache payer {payer}\n");
		if let ProviderSet::Registry(registry) = &self.providers {
			let state = registry.state();
			let block = state
				.block
				.map_or_else(|| "no read yet".to_string(), |block| format!("block {block}"));
			status.push_str(&format!(
				"{}: {block}, {} records\n",
				registry.describe(),
				state.records
			));
			if let Some(error) = state.error {
				status.push_str(&format!("last read failed: {error}\n"));
			}
		}
		if providers.is_empty() {
			status.push_str("no provider with an API URL in the provider set\n");
		}
		for provider in &providers {
			let measured = quality.get(&provider.id).cloned().unwrap_or_default();
			let latency =
				measured.latency_ms.map_or_else(|| "-".to_string(), |ms| format!("{ms} ms"));
			let failing =
				measured.failed_at.is_some_and(|at| now.duration_since(at) < FAILURE_COOLDOWN);
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

	/// One `POST /acquire` with a read request that the payer signs for this provider. The answer
	/// is the bytes and the node's headers about them, or `None` when the node answers 404
	/// (Bulletin does not hold the blob).
	async fn ask(
		&self,
		provider: &Provider,
		cid: &str,
		payer: &Keypair,
		transfer: &str,
	) -> Result<Option<Answer>, String> {
		let payer_id = payer.public.to_bytes();
		let issued_at =
			SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
		let message = read_message(transfer, &payer_id, &provider.id, cid, issued_at);
		let signature = payer.sign_simple(SIGNING_CONTEXT, &message);
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
		let header = |name: &str| {
			response
				.headers()
				.get(name)
				.and_then(|value| value.to_str().ok())
				.map(str::to_string)
		};
		let origin = header("x-cache-origin").unwrap_or_else(|| "unknown".to_string());
		let provider_ms = header("x-cache-elapsed-ms").and_then(|ms| ms.parse().ok());
		let trace = header("x-cache-trace");
		let price = header("x-cache-price").and_then(|price| price.parse().ok());
		let body = response.bytes().await.map_err(|error| error.to_string())?;
		if !status.is_success() {
			return Err(format!("{status}: {}", String::from_utf8_lossy(&body)));
		}
		Ok(Some(Answer { value: body.to_vec(), origin, provider_ms, trace, price }))
	}

	/// The provider that keeps `cid`: the provider `only`, or the first provider in quality order
	/// with the home nodes of `cid` first.
	fn retention_provider(
		&self,
		cid: &str,
		only: Option<[u8; 32]>,
	) -> Result<Provider, PreimageRetainError> {
		let unknown = |reason: &str| PreimageRetainError::Unknown { reason: reason.to_string() };
		let (ids, providers) = self.load_providers();
		let homes = home_nodes(cid, &ids);
		let ordered = {
			let quality = self.quality.lock().expect("cache quality mutex poisoned");
			order(&providers, &homes, &quality, false, Instant::now())
		};
		match only {
			Some(id) => ordered
				.into_iter()
				.find(|provider| provider.id == id)
				.ok_or_else(|| unknown("the cache provider is not in the provider set")),
			None => ordered.into_iter().next().ok_or_else(|| unknown("no cache providers")),
		}
	}

	/// Keep `content` on the provider of `item`, or on the provider that `retention_provider`
	/// picks. For an item that does not renew, the payer signs a retention authorization for the
	/// size and the window of `item.period_secs`, and pays the window in advance. For an item that
	/// renews, the payer signs a lease authorization with the price of one day as the most for each
	/// day. The lease ends after `item.period_secs`, or never when it is 0. The provider fetches,
	/// checks and keeps the content before it charges. It charges each next day of a lease itself.
	/// A dry run reads the account of the payer at the same provider and answers the price and its
	/// split, but it keeps and pays nothing. A dry run answers the kind and the pieces of
	/// `content`.
	async fn retain(
		&self,
		content: &Content,
		item: &RetainItem,
		dry_run: bool,
	) -> Result<RetainOutcome, PreimageRetainError> {
		let (cid, size) = (content.cid.as_str(), content.size);
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let no_account =
			|error: &PreimageReadError| unknown(format!("no cache account: {error:?}"));
		let Some(payer) = self.payer() else {
			return Err(unknown(format!("no cache payer yet: sign in, or set {PAYER_SEED_ENV}")));
		};
		let provider = self.retention_provider(cid, item.provider)?;
		let account = self.account(Some(provider.id)).await;
		let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
		let terms = if item.renew {
			let account = account.as_ref().map_err(no_account)?;
			let until =
				if item.period_secs == 0 { 0 } else { now.saturating_add(item.period_secs) };
			Terms { window: (now, until), max_per_day: Some(day_price(size, account)) }
		} else {
			Terms { window: (now, now.saturating_add(item.period_secs)), max_per_day: None }
		};
		let price = |retention: u64| terms.max_per_day.unwrap_or(retention);
		let mut outcome = RetainOutcome {
			cid: cid.to_string(),
			provider: provider.id,
			name: provider.name.clone(),
			region: provider.region.clone(),
			operator: None,
			kind: content.kind,
			size,
			pieces: content.pieces,
			until: terms.paid_until(),
			price: 0,
			charged: 0,
			free: 0,
			paid: 0,
			short: 0,
			renews: item.renew,
			state: RetainState::Quote,
		};
		if dry_run {
			let account = account.as_ref().map_err(no_account)?;
			outcome.price = price(retention_price(size, item.period_secs, account));
			(outcome.free, outcome.paid, outcome.short) = split(outcome.price, account);
			return Ok(outcome);
		}
		let reply = self.pin(&provider, &payer, cid, size, &terms).await?;
		if let Ok(account) = &account {
			(outcome.free, outcome.paid, outcome.short) = split(reply.charged, account);
		}
		Ok(RetainOutcome {
			kind: if reply.chunks > 0 { RetainKind::File } else { content.kind },
			size: reply.size,
			pieces: reply.chunks.saturating_add(1),
			until: reply.until,
			price: price(reply.charged),
			charged: reply.charged,
			state: RetainState::Kept,
			..outcome
		})
	}

	/// One `POST /pin` of the content `cid` of `size` bytes on `provider`, with a retention or a
	/// lease authorization that the payer signs for the size and `terms`.
	async fn pin(
		&self,
		provider: &Provider,
		payer: &Keypair,
		cid: &str,
		size: u64,
		terms: &Terms,
	) -> Result<PinReply, PreimageRetainError> {
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let payer_id = payer.public.to_bytes();
		let transfer = self.transfer_id();
		let window = terms.window;
		let (service, message) = match terms.max_per_day {
			None => (
				"Retention",
				retention_message(&transfer, &payer_id, &provider.id, cid, size, window),
			),
			Some(max_per_day) => (
				"Lease",
				lease_message(&transfer, &payer_id, &provider.id, cid, size, window, max_per_day),
			),
		};
		let signature = payer.sign_simple(SIGNING_CONTEXT, &message);
		let mut receipt = json!({
			"transfer": transfer,
			"payer": hex::encode(payer_id),
			"provider": hex::encode(provider.id),
			"content": cid,
			"service": service,
			"size": size,
			"from": window.0,
			"until": window.1,
		});
		if let Some(max_per_day) = terms.max_per_day {
			receipt["max_per_day"] = json!(max_per_day);
		}
		let body = json!({
			"reference": { "source": format!("bulletin:{cid}"), "cid": null, "size": size },
			"authorization": { "receipt": receipt, "signature": hex::encode(signature.to_bytes()) },
		});
		let response = self
			.http
			.post(format!("{}/pin", provider.url))
			.timeout(PIN_TIMEOUT)
			.json(&body)
			.send()
			.await
			.map_err(|error| unknown(error.to_string()))?;
		let status = response.status();
		if !status.is_success() {
			let text = response.text().await.unwrap_or_default();
			return Err(pin_error(status, &text));
		}
		let reply: PinReply = response.json().await.map_err(|error| unknown(error.to_string()))?;
		info!(
			cid,
			node = provider.url,
			until = reply.until,
			charged = reply.charged,
			"cache node keeps the preimage"
		);
		Ok(reply)
	}

	/// The CID in the contenthash of the DotNS name `name`, read through the Asset Hub. The host
	/// asks for `app.<name>` first, then for `name`.
	async fn resolve(&self, name: &str) -> Result<Cid, PreimageRetainError> {
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let Some(rpc) = &self.asset_hub else {
			return Err(unknown(
				"this host cannot resolve DotNS names without an Asset Hub RPC".to_string(),
			));
		};
		for candidate in [format!("app.{name}"), name.to_string()] {
			if let Some(cid) = content::contenthash(&self.http, rpc, &candidate)
				.await
				.map_err(|reason| unknown(format!("the contenthash of {candidate}: {reason}")))?
			{
				return Ok(cid);
			}
		}
		Err(PreimageRetainError::NotFound)
	}

	/// The cache providers in quality order, with the home nodes of the content `cid` first. Each
	/// provider shows its name, its region, its home rank for `cid` and what the host measured of
	/// it. Without a callable provider, the answer is `NoCacheProviders`.
	fn providers(
		&self,
		cid: Option<&str>,
	) -> Result<RemotePreimageCacheProvidersResponse, PreimageReadError> {
		let (ids, providers) = self.load_providers();
		if providers.is_empty() {
			return Err(PreimageReadError::NoCacheProviders);
		}
		let homes = cid.map(|cid| home_nodes(cid, &ids)).unwrap_or_default();
		let now = Instant::now();
		let quality = self.quality.lock().expect("cache quality mutex poisoned");
		let providers = order(&providers, &homes, &quality, false, now)
			.into_iter()
			.map(|provider| {
				let measured = quality.get(&provider.id).cloned().unwrap_or_default();
				CacheProviderInfo {
					id: provider.id,
					name: provider.name,
					region: provider.region,
					operator: None,
					source: CacheProviderSource::Registry,
					home: homes.contains(&provider.id),
					latency_ms: measured.latency_ms.map(|ms| u32::try_from(ms).unwrap_or(u32::MAX)),
					failed_recently: measured
						.failed_at
						.is_some_and(|at| now.duration_since(at) < FAILURE_COOLDOWN),
				}
			})
			.collect();
		Ok(RemotePreimageCacheProvidersResponse { providers })
	}

	/// The account of the payer at the provider `only`, or at the first provider in quality order:
	/// the quote of the provider's ledger. Without a payer or a provider, there is no account.
	async fn account(
		&self,
		only: Option<[u8; 32]>,
	) -> Result<RemotePreimageCacheAccountResponse, PreimageReadError> {
		let Some(payer) = self.payer() else {
			return Err(PreimageReadError::Unknown {
				reason: format!("no cache payer yet: sign in, or set {PAYER_SEED_ENV}"),
			});
		};
		let (_, providers) = self.load_providers();
		let provider = self.ledger_provider(&providers, only)?;
		let payer_id = payer.public.to_bytes();
		let unknown = |reason: String| PreimageReadError::Unknown { reason };
		let response = self
			.http
			.get(format!("{}/account/{}", provider.url, hex::encode(payer_id)))
			.send()
			.await
			.map_err(|error| unknown(error.to_string()))?;
		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(unknown(format!("{status}: {body}")));
		}
		let quote: Quote = response.json().await.map_err(|error| unknown(error.to_string()))?;
		Ok(RemotePreimageCacheAccountResponse {
			payer: payer_id,
			provider: provider.id,
			allowance: quote.allowance,
			allowance_retention: quote.allowance_retention,
			// A ledger without allowances gives only the balance, which is all paid.
			paid: quote.paid.unwrap_or(quote.balance),
			allowance_per_day: quote.allowance_per_day,
			retention_allowance_per_day: quote.retention_allowance_per_day,
			read_price: quote.price,
			read_price_per_mib: quote.read_price_per_mib,
			retention_price: quote.retention_price,
			retention_unit_secs: quote.retention_unit_secs,
		})
	}

	/// The provider of `providers` that the host asks about the ledger: the provider `only`, or the
	/// first provider in quality order.
	fn ledger_provider(
		&self,
		providers: &[Provider],
		only: Option<[u8; 32]>,
	) -> Result<Provider, PreimageReadError> {
		let ordered = {
			let quality = self.quality.lock().expect("cache quality mutex poisoned");
			order(providers, &[], &quality, false, Instant::now())
		};
		match only {
			Some(id) => ordered
				.into_iter()
				.find(|provider| provider.id == id)
				.ok_or(PreimageReadError::UnknownProvider),
			None => ordered.into_iter().next().ok_or(PreimageReadError::NoCacheProviders),
		}
	}

	/// The retentions of the payer, the latest end first. The payer signs a request for its
	/// retentions, and the provider `only`, or the first provider in quality order, sends it to the
	/// ledger. Then the host asks the provider of each retention for the state of the content, all
	/// at the same time. A provider that is not in the provider set, or that does not answer, gives
	/// the state `Unknown`.
	async fn retentions(
		&self,
		only: Option<[u8; 32]>,
	) -> Result<RemotePreimageRetentionsResponse, PreimageReadError> {
		let unknown = |reason: String| PreimageReadError::Unknown { reason };
		let Some(payer) = self.payer() else {
			return Err(unknown(format!("no cache payer yet: sign in, or set {PAYER_SEED_ENV}")));
		};
		let (_, providers) = self.load_providers();
		let provider = self.ledger_provider(&providers, only)?;
		let leases = self.leases(&provider, &payer).await.map_err(unknown)?;
		let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
		let retentions = leases.into_iter().filter_map(|lease| {
			let Some(id) =
				hex::decode(&lease.provider).ok().and_then(|id| <[u8; 32]>::try_from(id).ok())
			else {
				warn!(
					provider = lease.provider,
					"the ledger names a provider that is not an endpoint id"
				);
				return None;
			};
			let holder = providers.iter().find(|provider| provider.id == id).cloned();
			Some(async move {
				let pin = match &holder {
					Some(holder) => self.pin_state(holder, &lease.content).await,
					None => None,
				};
				retention(lease, id, holder, pin.as_ref(), now)
			})
		});
		Ok(RemotePreimageRetentionsResponse {
			payer: payer.public.to_bytes(),
			retentions: futures::future::join_all(retentions).await,
		})
	}

	/// The retentions of `payer` in the ledger, the latest end first. The payer signs a request for
	/// its retentions, and `provider` sends it to the ledger (`POST /leases`).
	async fn leases(&self, provider: &Provider, payer: &Keypair) -> Result<Vec<Lease>, String> {
		let payer_id = payer.public.to_bytes();
		let issued_at =
			SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
		let signature = payer.sign_simple(SIGNING_CONTEXT, &leases_message(&payer_id, issued_at));
		let request = json!({
			"request": { "payer": hex::encode(payer_id), "issued_at": issued_at },
			"signature": hex::encode(signature.to_bytes()),
		});
		let response = self
			.http
			.post(format!("{}/leases", provider.url))
			.json(&request)
			.send()
			.await
			.map_err(|error| error.to_string())?;
		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(format!("{status}: {body}"));
		}
		let leases: Leases = response.json().await.map_err(|error| error.to_string())?;
		Ok(leases.leases)
	}

	/// Stop the leases of `cid` on the provider `id`. The payer signs a stop request, and the
	/// provider sends it to the ledger (`POST /leases/stop`). The ledger charges no more days. The
	/// answer is the end of the paid time of the retention in the ledger after the stop, or 0 when
	/// the ledger has no retention of `cid` on that provider.
	async fn stop(&self, cid: &str, id: [u8; 32]) -> Result<u64, PreimageRetainError> {
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let Some(payer) = self.payer() else {
			return Err(unknown(format!("no cache payer yet: sign in, or set {PAYER_SEED_ENV}")));
		};
		let provider = self.retention_provider(cid, Some(id))?;
		let payer_id = payer.public.to_bytes();
		let issued_at =
			SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
		let message = stop_message(&payer_id, &id, cid, issued_at);
		let signature = payer.sign_simple(SIGNING_CONTEXT, &message);
		let request = json!({
			"request": {
				"payer": hex::encode(payer_id),
				"provider": hex::encode(id),
				"content": cid,
				"issued_at": issued_at,
			},
			"signature": hex::encode(signature.to_bytes()),
		});
		let response = self
			.http
			.post(format!("{}/leases/stop", provider.url))
			.json(&request)
			.send()
			.await
			.map_err(|error| unknown(error.to_string()))?;
		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(unknown(format!("{status}: {body}")));
		}
		let stopped: Stopped = response.json().await.map_err(|error| unknown(error.to_string()))?;
		info!(cid, node = provider.url, stopped = stopped.stopped, "cache node stopped the leases");
		let provider_hex = hex::encode(id);
		let leases = self.leases(&provider, &payer).await.map_err(unknown)?;
		Ok(leases
			.iter()
			.find(|lease| lease.content == cid && lease.provider == provider_hex)
			.map_or(0, Lease::paid_until))
	}

	/// The state of `content` on `provider` (`GET /pin-state/<content>`). The answer is `None` when
	/// the provider fails, or does not answer in `PIN_STATE_TIMEOUT`.
	async fn pin_state(&self, provider: &Provider, content: &str) -> Option<PinState> {
		let response = self
			.http
			.get(format!("{}/pin-state/{content}", provider.url))
			.timeout(PIN_STATE_TIMEOUT)
			.send()
			.await;
		let answer = match response {
			Ok(response) if response.status().is_success() => response.json().await,
			Ok(response) => {
				debug!(node = provider.url, content, status = %response.status(), "no pin state");
				return None;
			},
			Err(error) => Err(error),
		};
		answer
			.inspect_err(|error| debug!(node = provider.url, content, %error, "no pin state"))
			.ok()
	}

	/// Sign the receipt for a verified delivery of `size` bytes and send it to the provider that
	/// served it. A ledger that prices reads by the MiB charges this size. The read does not wait
	/// for the payment. The provider keeps the delivery as unpaid until the receipt arrives.
	fn pay(&self, provider: &Provider, cid: &str, payer: &Keypair, transfer: String, size: u64) {
		let payer_id = payer.public.to_bytes();
		let message = receipt_message(&transfer, &payer_id, &provider.id, cid, size);
		let signature = payer.sign_simple(SIGNING_CONTEXT, &message);
		let receipt = json!({
			"receipt": {
				"transfer": transfer,
				"payer": hex::encode(payer_id),
				"provider": hex::encode(provider.id),
				"content": cid,
				"service": "Delivery",
				"size": size,
				"from": 0,
				"until": 0,
			},
			"signature": hex::encode(signature.to_bytes()),
		});
		let request = self.http.post(format!("{}/receipt", provider.url)).json(&receipt);
		let node = provider.url.clone();
		tokio::spawn(async move {
			match request.send().await {
				Ok(response) if response.status().is_success() => {
					let settlement = response.text().await.unwrap_or_default();
					debug!(node, transfer, settlement, "cache receipt paid");
				},
				Ok(response) => {
					warn!(node, transfer, status = %response.status(), "cache node refused the receipt")
				},
				Err(error) => warn!(node, transfer, %error, "cache receipt not sent"),
			}
		});
	}

	/// A new transfer id for each request to a node. A node serves a transfer id once, and the
	/// ledger of the cache charges it once.
	fn transfer_id(&self) -> String {
		let nanos =
			SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
		format!("truapi-{nanos}")
	}
}

/// The error of a pin from the status and the text of the node: a payer that cannot pay is
/// `NotAvailable`, content that no source has is `NotFound`, and a window longer than the node
/// keeps content is `PeriodTooLong`, with the limit from the text of the node.
fn pin_error(status: reqwest::StatusCode, text: &str) -> PreimageRetainError {
	if status == reqwest::StatusCode::PAYMENT_REQUIRED {
		return PreimageRetainError::NotAvailable;
	}
	if status == reqwest::StatusCode::NOT_FOUND {
		return PreimageRetainError::NotFound;
	}
	if let Some(rest) = text.split("a pin ends 1 to ").nth(1) &&
		let Some(max_secs) = rest.split_whitespace().next().and_then(|secs| secs.parse().ok())
	{
		return PreimageRetainError::PeriodTooLong { max_secs };
	}
	PreimageRetainError::Unknown { reason: format!("{status}: {text}") }
}

/// The price of the retention of `size` bytes for `period_secs` in the ledger of `account`: each
/// started MiB, at least one, for each started unit of retention time, at the retention price. The
/// cache nodes use the same rule (`cache/src/logic.rs`).
fn retention_price(
	size: u64,
	period_secs: u64,
	account: &RemotePreimageCacheAccountResponse,
) -> u64 {
	account
		.retention_price
		.saturating_mul(size.div_ceil(MIB).max(1))
		.saturating_mul(period_secs.div_ceil(account.retention_unit_secs.max(1)))
}

/// The price of one day of a lease of `size` bytes in the ledger of `account`.
fn day_price(size: u64, account: &RemotePreimageCacheAccountResponse) -> u64 {
	retention_price(size, DAY_SECS, account)
}

/// The split of `price` over the account of the payer: the part that the free allowance for
/// retention pays, the part that the paid credits pay, and the part that the payer cannot pay.
fn split(price: u64, account: &RemotePreimageCacheAccountResponse) -> (u64, u64, u64) {
	let free = price.min(account.allowance).min(account.allowance_retention);
	let paid = (price - free).min(account.paid);
	(free, paid, price - free - paid)
}

/// The retention of `lease` on the provider `id`. `holder` is that provider in the provider set,
/// and `pin` is the state of the content on it. A pin state `none` means that the provider keeps
/// no block of the content: the retention ended when its window ended, else the provider lost
/// the content. The pieces are the blocks of the pins, and at least one.
fn retention(
	lease: Lease,
	id: [u8; 32],
	holder: Option<Provider>,
	pin: Option<&PinState>,
	now: u64,
) -> PreimageRetention {
	let count = |value: u64| u32::try_from(value).unwrap_or(u32::MAX);
	let state = match pin.map(|pin| pin.state.as_str()) {
		Some("kept") => RetentionState::Kept,
		Some("fetching") => RetentionState::Fetching {
			pieces: pin.map_or(0, |pin| count(pin.pieces)),
			total: pin.map_or(0, |pin| count(pin.total_pieces)),
		},
		Some("lost") => RetentionState::Lost,
		Some("ended") => RetentionState::Ended,
		Some("none") if lease.until <= now => RetentionState::Ended,
		Some("none") => RetentionState::Lost,
		_ => RetentionState::Unknown,
	};
	let (name, region) = holder.map_or((None, None), |holder| (holder.name, holder.region));
	let paid_until = lease.paid_until();
	PreimageRetention {
		cid: lease.content,
		provider: id,
		name,
		region,
		operator: None,
		size: lease.size,
		pieces: pin.map_or(1, |pin| count(pin.blocks).max(1)),
		from: lease.from,
		until: lease.until,
		paid_until,
		charged: lease.charged,
		per_day: lease.per_day,
		renews: lease.renews,
		state,
	}
}

/// A payer from a 32-byte hex seed. The function expands the seed as Substrate expands an sr25519
/// mini secret key.
fn payer_from_seed(seed: &str) -> Result<Keypair, String> {
	let bytes =
		hex::decode(seed.trim().trim_start_matches("0x")).map_err(|error| error.to_string())?;
	let secret = MiniSecretKey::from_bytes(&bytes).map_err(|error| error.to_string())?;
	Ok(secret.expand_to_keypair(ExpansionMode::Ed25519))
}

/// A blob source that asks the cache nodes first, if there are cache nodes, and then `fallback`.
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

impl<S: BlobSource> CacheFirst<S> {
	/// Keep each item of `request` on a cache node, in the order of the request, or quote each item
	/// in a dry run. An item names a preimage key, a CID, or a DotNS name, which the host resolves
	/// to the CID of its contenthash. Without a size in an item, the host reads the block of the
	/// CID once to learn the size.
	pub async fn retain(
		&self,
		request: RemotePreimageRetainRequest,
	) -> Result<RemotePreimageRetainResponse, PreimageRetainError> {
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let Some(cache) = &self.cache else {
			return Err(unknown("no cache providers".to_string()));
		};
		let mut items = Vec::with_capacity(request.items.len());
		for item in &request.items {
			let cid = match &item.target {
				PreimageTarget::Key(key) => <[u8; 32]>::try_from(key.as_slice())
					.map(Cid::preimage)
					.map_err(|_| unknown("a preimage key is 32 bytes".to_string()))?,
				PreimageTarget::Cid(text) => Cid::parse(text).map_err(unknown)?,
				PreimageTarget::DotNs(name) => cache.resolve(name).await?,
			};
			let content = match item.size {
				Some(size) => Content {
					cid: cid.to_string(),
					size,
					kind: if cid.is_dag_pb() { RetainKind::File } else { RetainKind::Raw },
					pieces: 1,
				},
				None => self.measure(&cid).await?,
			};
			items.push(cache.retain(&content, item, request.dry_run).await?);
		}
		Ok(RemotePreimageRetainResponse { items })
	}

	/// The size, the kind and the pieces of the content `cid`, from its block. A raw block is one
	/// piece. A dag-pb root is a file: its size is the size of the root block and the `Tsize` of
	/// each link, and each link is one more piece.
	async fn measure(&self, cid: &Cid) -> Result<Content, PreimageRetainError> {
		let block = self.block(cid).await?;
		let size = block.len() as u64;
		if !cid.is_dag_pb() {
			return Ok(Content { cid: cid.to_string(), size, kind: RetainKind::Raw, pieces: 1 });
		}
		let links = content::link_sizes(&block).map_err(|reason| PreimageRetainError::Unknown {
			reason: format!("the dag-pb root {cid} {reason}"),
		})?;
		Ok(Content {
			cid: cid.to_string(),
			size: links.iter().fold(size, |total, link| total.saturating_add(*link)),
			kind: RetainKind::File,
			pieces: u32::try_from(links.len()).unwrap_or(u32::MAX).saturating_add(1),
		})
	}

	/// The block of `cid`, checked against its multihash. Bulletin finds a block by its multihash,
	/// so the host asks for the raw CID of the same multihash. The cache nodes check blake2b-256
	/// only, so the host asks them first for a blake2b-256 block only, and asks Bulletin for the
	/// other blocks.
	async fn block(&self, cid: &Cid) -> Result<Vec<u8>, PreimageRetainError> {
		let raw = cid.raw().to_string();
		let read =
			if cid.is_blake2b() { self.get(&raw).await } else { self.fallback.get(&raw).await };
		match read {
			Ok(Some(block)) if cid.holds(&block) => Ok(block),
			Ok(Some(_)) => Err(PreimageRetainError::Unknown {
				reason: format!("Bulletin sent bytes that do not hash to {cid}"),
			}),
			Ok(None) => Err(PreimageRetainError::NotFound),
			Err(SourceError::Transient(reason) | SourceError::Permanent(reason)) => {
				Err(PreimageRetainError::Unknown { reason })
			},
		}
	}
}

impl<S> CacheFirst<S> {
	/// The cache account of the payer at the provider `only`, or at the first provider of the
	/// order.
	pub async fn cache_account(
		&self,
		only: Option<[u8; 32]>,
	) -> Result<RemotePreimageCacheAccountResponse, PreimageReadError> {
		match &self.cache {
			Some(cache) => cache.account(only).await,
			None => Err(PreimageReadError::NoCacheProviders),
		}
	}

	/// The retentions of the payer, from the ledger that the provider `only`, or the first provider
	/// of the order, asks.
	pub async fn cache_retentions(
		&self,
		only: Option<[u8; 32]>,
	) -> Result<RemotePreimageRetentionsResponse, PreimageReadError> {
		match &self.cache {
			Some(cache) => cache.retentions(only).await,
			None => Err(PreimageReadError::NoCacheProviders),
		}
	}

	/// Stop the renewal of the retention of `request.cid` on `request.provider`. The answer is the
	/// end of the paid time, until which the provider keeps the content.
	pub async fn stop_retention(
		&self,
		request: RemotePreimageStopRetentionRequest,
	) -> Result<RemotePreimageStopRetentionResponse, PreimageRetainError> {
		let unknown = |reason: String| PreimageRetainError::Unknown { reason };
		let Some(cache) = &self.cache else {
			return Err(unknown("no cache providers".to_string()));
		};
		let cid = Cid::parse(&request.cid).map_err(unknown)?.to_string();
		let until = cache.stop(&cid, request.provider).await?;
		Ok(RemotePreimageStopRetentionResponse { until })
	}

	/// The cache providers in the order of the host for `target`, with the home nodes of the
	/// target first. A DotNS name has the home nodes of the CID of its contenthash. A name that the
	/// host cannot resolve, or no target, has no home nodes.
	pub async fn cache_providers(
		&self,
		target: Option<&PreimageTarget>,
	) -> Result<RemotePreimageCacheProvidersResponse, PreimageReadError> {
		let Some(cache) = &self.cache else {
			return Err(PreimageReadError::NoCacheProviders);
		};
		let cid = match target {
			Some(PreimageTarget::Key(key)) => {
				let key = <[u8; 32]>::try_from(key.as_slice()).map_err(|_| {
					PreimageReadError::Unknown { reason: "a preimage key is 32 bytes".to_string() }
				})?;
				Some(preimage_cid(&key))
			},
			Some(PreimageTarget::Cid(text)) => {
				Some(Cid::parse(text).map_or_else(|_| text.clone(), |cid| cid.to_string()))
			},
			Some(PreimageTarget::DotNs(name)) => {
				cache.resolve(name).await.ok().map(|cid| cid.to_string())
			},
			None => None,
		};
		cache.providers(cid.as_deref())
	}
}

impl<S: BlobSource> CacheFirst<S> {
	/// One read of `key` through `route`, with a report of each source that the host asked. The
	/// read makes one pass and does not poll, so a miss gives no value. `Auto` asks the cache nodes
	/// in quality order and then `fallback`, the Bulletin node. `Cache` and `CacheProvider` never
	/// ask Bulletin. They answer `NoCacheProviders` when there is no cache node to ask.
	pub async fn read_routed(
		&self,
		key: &[u8],
		route: PreimageReadRoute,
	) -> Result<RemotePreimageReadResponse, PreimageReadError> {
		let key = <[u8; 32]>::try_from(key).map_err(|_| PreimageReadError::Unknown {
			reason: "a preimage key is 32 bytes".to_string(),
		})?;
		let cid = preimage_cid(&key);
		let mut attempts = Vec::new();
		let cache_only = match route {
			PreimageReadRoute::Auto => None,
			PreimageReadRoute::Bulletin => Some(false),
			PreimageReadRoute::Cache | PreimageReadRoute::CacheProvider { .. } => Some(true),
		};
		if cache_only != Some(false) {
			let only = match route {
				PreimageReadRoute::CacheProvider { id } => Some(id),
				_ => None,
			};
			let read = match &self.cache {
				Some(cache) => cache.read_reported(&cid, only).await,
				None => Err(PreimageReadError::NoCacheProviders),
			};
			match read {
				Ok(read) => {
					attempts.extend(read.attempts);
					if read.value.is_some() {
						return Ok(read_response(read.value, read.served_by, attempts));
					}
				},
				// `Auto` reads Bulletin when no cache node can help, as a lookup does.
				Err(error) if cache_only == Some(true) => return Err(error),
				Err(_) => {},
			}
		}
		if cache_only == Some(true) {
			return Ok(read_response(None, None, attempts));
		}

		let via = BulletinReadVia::Rpc;
		let started = Instant::now();
		let outcome = match self.fallback.get(&cid).await {
			Ok(Some(value)) if preimage_key(&value) == key => {
				attempts.push(PreimageReadAttempt {
					source: PreimageReadAttemptSource::Bulletin { via },
					outcome: PreimageReadOutcome::Served,
					ms: elapsed_ms(started),
				});
				return Ok(read_response(
					Some(value),
					Some(PreimageReadSource::Bulletin { via }),
					attempts,
				));
			},
			Ok(Some(_)) => PreimageReadOutcome::BadBytes,
			Ok(None) => PreimageReadOutcome::Miss,
			Err(SourceError::Transient(reason) | SourceError::Permanent(reason)) => {
				PreimageReadOutcome::Failed { reason }
			},
		};
		attempts.push(PreimageReadAttempt {
			source: PreimageReadAttemptSource::Bulletin { via },
			outcome,
			ms: elapsed_ms(started),
		});
		Ok(read_response(None, None, attempts))
	}
}

/// A read response. The core sets `host_ms`, so it is 0 here.
fn read_response(
	value: Option<Vec<u8>>,
	served_by: Option<PreimageReadSource>,
	attempts: Vec<PreimageReadAttempt>,
) -> RemotePreimageReadResponse {
	RemotePreimageReadResponse {
		value,
		report: PreimageReadReport { served_by, attempts, host_ms: 0 },
	}
}

#[async_trait]
impl<S: BlobSource> BlobSource for CacheFirst<S> {
	async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
		if let Some(cache) = &self.cache &&
			let Some(value) = cache.read(cid).await
		{
			return Ok(Some(value));
		}
		self.fallback.get(cid).await
	}
}

#[cfg(test)]
mod tests {
	use tokio::{
		io::{AsyncReadExt, AsyncWriteExt},
		net::{TcpListener, TcpStream},
	};

	use super::*;

	/// The root block of the site `cache-demo.paseo` on Paseo Bulletin Next, from bulletin-deploy:
	/// a UnixFS file node that links three raw sha2-256 chunks of 1573, 245076 and 910 bytes.
	pub const CACHE_DEMO_ROOT: &str = "122b0a2401551220790e8fe960819b22c3e0aa271bf9f60b060e6ac93aa627cde27885ea897f92a5120018a50c122c0a2401551220b2e01a32e5ca22d806c76069e53d8986f3767e513de1e4d501b7bb47cfb4f342120018d4fa0e122b0a240155122018658392ed83c55d5e0e31e3c8c3eb2bbbde991e6064fefac153f93061652b221200188e070a10080218878e0f20a50c20d4fa0e208e07";
	/// The CID of that root block, which the DotNS contenthash of `cache-demo.paseo` names.
	pub const CACHE_DEMO_CID: &str = "bafybeicepvbzj6i4yflvozwz464yxasywzpf6uvabuz5ol2bs3johuoxxe";
	/// The size of all blocks of the site: the root block and the three chunks.
	const CACHE_DEMO_SIZE: u64 = 154 + 1573 + 245_076 + 910;

	fn blob() -> Vec<u8> {
		b"cache lookup test blob".to_vec()
	}

	fn cid() -> String {
		preimage_cid(&preimage_key(&blob()))
	}

	/// A Bulletin node that always holds `value` and counts the requests that it gets.
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
		Bulletin { value, calls: Mutex::new(0) }
	}

	/// A Bulletin node that holds blocks by CID text, as `bitswap_v1_get` finds them.
	struct Blocks(HashMap<String, Vec<u8>>);

	#[async_trait]
	impl BlobSource for Blocks {
		async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
			Ok(self.0.get(cid).cloned())
		}
	}

	/// A Bulletin node that holds the root block of the site `cache-demo.paseo`.
	fn site_blocks() -> Blocks {
		let raw = Cid::parse(CACHE_DEMO_CID).unwrap().raw().to_string();
		Blocks(HashMap::from([(raw, hex::decode(CACHE_DEMO_ROOT).unwrap())]))
	}

	/// The output of `ReviveApi_call` for a call that returns `data`: zero weights, deposits and
	/// gas, then the result `Ok` without flags.
	fn revive_output(data: &[u8]) -> Vec<u8> {
		use parity_scale_codec::Encode;
		let mut output = vec![0; 4 + 2 * 17 + 16 + 1 + 4];
		data.to_vec().encode_to(&mut output);
		output
	}

	/// The ABI form of one `bytes` value.
	fn abi_bytes(value: &[u8]) -> Vec<u8> {
		let mut data = vec![0; 64];
		data[31] = 32;
		data[56..64].copy_from_slice(&(value.len() as u64).to_be_bytes());
		data.extend_from_slice(value);
		data.resize(64 + value.len().div_ceil(32) * 32, 0);
		data
	}

	/// An Asset Hub on loopback with the DotNS content resolver: it answers each `state_call` with
	/// the contenthash of the name in `names` that has the namehash in the call, or with an empty
	/// contenthash.
	async fn fake_asset_hub(names: Vec<(&'static str, Vec<u8>)>) -> String {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://{}", listener.local_addr().unwrap());
		let contenthashes: HashMap<[u8; 32], Vec<u8>> = names
			.into_iter()
			.map(|(name, contenthash)| (content::namehash(name), contenthash))
			.collect();
		tokio::spawn(async move {
			loop {
				let (mut stream, _) = listener.accept().await.unwrap();
				let (_, request) = read_request(&mut stream).await;
				let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
				let args = request["params"][1].as_str().unwrap().trim_start_matches("0x");
				let args = hex::decode(args).unwrap();
				// The call data ends with the node, the one argument of `contenthash(bytes32)`.
				let node = <[u8; 32]>::try_from(&args[args.len() - 32..]).unwrap();
				let contenthash = contenthashes.get(&node).cloned().unwrap_or_default();
				let output = revive_output(&abi_bytes(&contenthash));
				let result = format!("0x{}", hex::encode(output));
				let body = json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string();
				let head = format!(
					"HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
					body.len()
				);
				stream.write_all(head.as_bytes()).await.unwrap();
				stream.write_all(body.as_bytes()).await.unwrap();
			}
		});
		url
	}

	type Requests = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

	/// A cache node on loopback. It answers `POST /acquire` with `status` and `body`, and
	/// `POST /receipt` with a settlement. It keeps the path and the body of each request.
	async fn fake_node(status: u16, body: Vec<u8>) -> (String, Requests) {
		routed_node(status, body, Vec::new()).await
	}

	/// A cache node on loopback, as `fake_node`. It also answers each path that starts with the
	/// prefix of a route with the status and the body of that route.
	async fn routed_node(
		status: u16,
		body: Vec<u8>,
		routes: Vec<(&'static str, u16, Vec<u8>)>,
	) -> (String, Requests) {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://{}", listener.local_addr().unwrap());
		let requests = Requests::default();
		let seen = requests.clone();
		tokio::spawn(async move {
			loop {
				let (mut stream, _) = listener.accept().await.unwrap();
				let (path, request) = read_request(&mut stream).await;
				let route = routes.iter().find(|(prefix, _, _)| path.starts_with(prefix));
				let (status, body) = match (path.as_str(), route) {
					("/receipt", _) => (200, b"{\"Charged\":10}".to_vec()),
					(_, Some((_, status, body))) => (*status, body.clone()),
					_ => (status, body.clone()),
				};
				let request = serde_json::from_slice(&request).unwrap_or_default();
				seen.lock().unwrap().push((path, request));
				let head = format!(
					"HTTP/1.1 {status} Fake\r\ncontent-length: {}\r\nx-cache-origin: local\r\nx-cache-elapsed-ms: 3\r\n\
                     x-cache-trace: [\"local\"]\r\nx-cache-price: 10\r\nconnection: close\r\n\r\n",
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
		Provider { id: [n; 32], url: url.to_string(), name: None, region: None }
	}

	fn test_payer() -> Keypair {
		payer_from_seed(&hex::encode([1u8; 32])).unwrap()
	}

	fn cache(providers: Vec<Provider>, payer: Option<Keypair>) -> Option<Arc<CacheNodes>> {
		cache_with_asset_hub(providers, payer, None)
	}

	fn cache_with_asset_hub(
		providers: Vec<Provider>,
		payer: Option<Keypair>,
		asset_hub: Option<String>,
	) -> Option<Arc<CacheNodes>> {
		let ids = providers.iter().map(|provider| provider.id).collect();
		CacheNodes::new(
			ProviderSet::Fixed(ids, providers),
			payer.map(|payer| PayerSource::Seed(Box::new(payer))),
			None,
			asset_hub,
		)
		.map(Arc::new)
	}

	/// The answer of `source` for the test blob. These sources never fail a read. They report a
	/// miss instead.
	async fn read<S: BlobSource>(source: &CacheFirst<S>) -> Option<Vec<u8>> {
		match source.get(&cid()).await {
			Ok(value) => value,
			Err(_) => panic!("the read failed instead of a miss"),
		}
	}

	/// The receipt requests that a fake node got, after the receipt task had time to send them.
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
		test_payer().public.verify_simple(context, message, &signature).is_ok()
	}

	// The cache has the same vectors (cache/src/logic.rs and cache/src/payment.rs). Hosts and nodes
	// must rank the same home nodes, derive the same payer from a seed and sign the same bytes.
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
				hex::encode(receipt_message("t-1", &payer, &[7; 32], key, 0)),
				hex::encode(read_message("t-1", &payer, &[7; 32], key, 1_791_500_000)),
				hex::encode(retention_message("t-1", &payer, &[7; 32], key, 1 << 20, (1000, 4600))),
			),
			(
				"1b90bef8509cc25b45d94885d8a1eca44bd751212310efc75c9e2628a663b98f".to_string(),
				"189dac29296d31814dc8c56cf3d36a0543372bba7538fa322a4aebfebc39e056".to_string(),
				format!(
					"63616368652d726563656970742f32{fields}00000000000000000000000000000000000000000000000000"
				),
				format!("63616368652d726561642f31{fields}e01ec86a00000000"),
				format!(
					"63616368652d726563656970742f32{fields}010000100000000000e803000000000000f811000000000000"
				),
			)
		);
		assert_eq!(
			hex::encode(leases_message(&payer, 1_790_000_000)),
			"63616368652d6c65617365732f31189dac29296d31814dc8c56cf3d36a0543372bba7538fa322a4aebfebc39e056803bb16a00000000"
		);
		// A lease of 1 MiB that renews (until 0), 5 credits a day at most, and a request to stop
		// it.
		assert_eq!(
			(
				hex::encode(lease_message("t-1", &payer, &[7; 32], key, 1 << 20, (1000, 0), 5)),
				hex::encode(stop_message(&payer, &[7; 32], key, 1_790_000_000)),
			),
			(
				format!(
					"63616368652d726563656970742f33{fields}020000100000000000e80300000000000000000000000000000500000000000000"
				),
				format!(
					"63616368652d73746f702f31{}{}3e000000{}803bb16a00000000",
					hex::encode(payer),
					"07".repeat(32),
					hex::encode(key)
				),
			)
		);
	}

	// A product gets a reason that it can act on from the answer of the node.
	#[test]
	fn pin_errors() {
		use reqwest::StatusCode;
		assert_eq!(
			[
				pin_error(StatusCode::PAYMENT_REQUIRED, "refused: payer has 0 credits"),
				pin_error(StatusCode::NOT_FOUND, "not found"),
				pin_error(
					StatusCode::BAD_REQUEST,
					"rejected: a pin ends 1 to 1209600 seconds after the node's clock 5, not at 9999999"
				),
			],
			[
				PreimageRetainError::NotAvailable,
				PreimageRetainError::NotFound,
				PreimageRetainError::PeriodTooLong { max_secs: 1_209_600 },
			]
		);
	}

	#[test]
	fn provider_set_files() {
		let text = format!(
			"# providers\n{a} 10.0.0.1:4433 http://a.example:8080/ name=A region=fr-par\n\n{b}  # no API\n\
             not-hex http://x\n{c} name= https://c.example\n",
			a = hex::encode([1u8; 32]),
			b = hex::encode([2u8; 32]),
			c = hex::encode([3u8; 32]),
		);
		let named = Provider {
			name: Some("A".to_string()),
			region: Some("fr-par".to_string()),
			..provider(1, "http://a.example:8080")
		};
		assert_eq!(
			parse_providers(&text),
			(vec![[1; 32], [2; 32], [3; 32]], vec![named, provider(3, "https://c.example")])
		);
	}

	/// A node reports `local`, `source`, or `peer:` and a full endpoint id. Other text is unknown.
	#[test]
	fn origin_headers() {
		let peer = format!("peer:{}", hex::encode([5u8; 32]));
		assert_eq!(
			["local", "source", peer.as_str(), "peer:abcdef1234", "elsewhere"].map(parse_origin),
			[
				CacheOrigin::Local,
				CacheOrigin::Source,
				CacheOrigin::Peer { id: [5; 32] },
				CacheOrigin::Unknown,
				CacheOrigin::Unknown,
			]
		);
	}

	/// A read report with every time set to 0, so that a test can compare the whole report.
	fn untimed(
		result: Result<RemotePreimageReadResponse, PreimageReadError>,
	) -> Result<RemotePreimageReadResponse, PreimageReadError> {
		result.map(|mut response| {
			for attempt in &mut response.report.attempts {
				attempt.ms = 0;
			}
			response
		})
	}

	fn attempt(
		source: PreimageReadAttemptSource,
		outcome: PreimageReadOutcome,
	) -> PreimageReadAttempt {
		PreimageReadAttempt { source, outcome, ms: 0 }
	}

	fn served_by_node(n: u8, rank: u32) -> PreimageReadSource {
		PreimageReadSource::CacheProvider {
			id: [n; 32],
			name: None,
			region: None,
			origin: CacheOrigin::Local,
			rank,
			home: true,
			provider_ms: Some(3),
			trace: Some("[\"local\"]".to_string()),
			price: Some(10),
		}
	}

	/// Each route asks only the sources that it names. The report shows the source that served and
	/// the sources that the host asked.
	#[tokio::test]
	async fn reads_report_the_route_that_each_route_takes() {
		let (holder, holder_requests) = fake_node(200, blob()).await;
		let (missing, _) = fake_node(404, b"not found: not held".to_vec()).await;
		let key = preimage_key(&blob());
		let source = CacheFirst::new(
			cache(vec![provider(1, &holder)], Some(test_payer())),
			bulletin(Some(blob())),
		);
		let cache_only = CacheFirst::new(
			cache(vec![provider(2, &missing)], Some(test_payer())),
			bulletin(Some(blob())),
		);
		let no_payer =
			CacheFirst::new(cache(vec![provider(1, &holder)], None), bulletin(Some(blob())));
		let node = |n: u8| PreimageReadAttemptSource::CacheProvider { id: [n; 32] };
		let rpc = PreimageReadAttemptSource::Bulletin { via: BulletinReadVia::Rpc };
		let results = [
			source.read_routed(&key, PreimageReadRoute::Auto).await,
			source.read_routed(&key, PreimageReadRoute::Bulletin).await,
			cache_only.read_routed(&key, PreimageReadRoute::Cache).await,
			cache_only.read_routed(&key, PreimageReadRoute::Auto).await,
			source.read_routed(&key, PreimageReadRoute::CacheProvider { id: [9; 32] }).await,
			no_payer.read_routed(&key, PreimageReadRoute::Cache).await,
		]
		.map(untimed);
		let paid = receipts(&holder_requests).await.len();
		let bulletin_calls =
			(*source.fallback.calls.lock().unwrap(), *cache_only.fallback.calls.lock().unwrap());
		assert_eq!(
			(results, bulletin_calls.0, bulletin_calls.1, paid),
			(
				[
					Ok(read_response(
						Some(blob()),
						Some(served_by_node(1, 0)),
						vec![attempt(node(1), PreimageReadOutcome::Served)],
					)),
					Ok(read_response(
						Some(blob()),
						Some(PreimageReadSource::Bulletin { via: BulletinReadVia::Rpc }),
						vec![attempt(rpc.clone(), PreimageReadOutcome::Served)],
					)),
					Ok(read_response(
						None,
						None,
						vec![attempt(node(2), PreimageReadOutcome::Miss)],
					)),
					Ok(read_response(
						Some(blob()),
						Some(PreimageReadSource::Bulletin { via: BulletinReadVia::Rpc }),
						vec![
							attempt(node(2), PreimageReadOutcome::Miss),
							attempt(rpc, PreimageReadOutcome::Served),
						],
					)),
					Err(PreimageReadError::UnknownProvider),
					Err(PreimageReadError::NoCacheProviders),
				],
				1,
				1,
				1,
			)
		);
	}

	/// A provider that sends bad bytes gets a `BadBytes` attempt and no receipt. The next provider
	/// serves.
	#[tokio::test]
	async fn a_bad_provider_is_reported_and_the_next_one_serves() {
		let (liar, liar_requests) = fake_node(200, b"forged".to_vec()).await;
		let (honest, _) = fake_node(200, blob()).await;
		let source = CacheFirst::new(
			cache(vec![provider(1, &liar), provider(2, &honest)], Some(test_payer())),
			bulletin(None),
		);
		if let Some(cache) = &source.cache {
			let mut quality = cache.quality.lock().unwrap();
			for (id, latency_ms) in [([1; 32], 1), ([2; 32], 1000)] {
				quality.insert(id, Quality { latency_ms: Some(latency_ms), ..Default::default() });
			}
		}
		let read =
			untimed(source.read_routed(&preimage_key(&blob()), PreimageReadRoute::Cache).await);
		assert_eq!(
			(read, receipts(&liar_requests).await.len()),
			(
				Ok(read_response(
					Some(blob()),
					Some(served_by_node(2, 1)),
					vec![
						attempt(
							PreimageReadAttemptSource::CacheProvider { id: [1; 32] },
							PreimageReadOutcome::BadBytes
						),
						attempt(
							PreimageReadAttemptSource::CacheProvider { id: [2; 32] },
							PreimageReadOutcome::Served
						),
					],
				)),
				0,
			)
		);
	}

	/// The measured latency sets the order. A home node counts at half its latency, and a recent
	/// failure goes last. An exploration read puts an unmeasured provider first.
	#[test]
	fn providers_are_ordered_by_quality() {
		let now = Instant::now();
		let providers: Vec<Provider> =
			(1..=4).map(|n| provider(n, &format!("http://{n}"))).collect();
		let latency = |ms| Quality { latency_ms: Some(ms), ..Default::default() };
		let quality = HashMap::from([
			([1; 32], latency(30)),
			([2; 32], latency(10)),
			([3; 32], Quality { failed_at: Some(now), ..latency(5) }),
		]);
		let ids = |ordered: Vec<Provider>| {
			ordered.iter().map(|provider| provider.id[0]).collect::<Vec<_>>()
		};
		assert_eq!(ids(order(&providers, &[], &quality, false, now)), vec![2, 1, 4, 3]);
		assert_eq!(ids(order(&providers, &[[1; 32]], &quality, false, now)), vec![2, 1, 4, 3]);
		// An unmeasured home node counts at 50 / 2 = 25 ms: ahead of 1 (30 ms), behind 2 (10 ms).
		assert_eq!(ids(order(&providers, &[[4; 32]], &quality, false, now)), vec![2, 4, 1, 3]);
		assert_eq!(ids(order(&providers, &[], &quality, true, now)), vec![4, 2, 1, 3]);
		let later = now + FAILURE_COOLDOWN;
		assert_eq!(ids(order(&providers, &[], &quality, false, later)), vec![3, 2, 1, 4]);
	}

	#[test]
	fn latency_is_a_moving_average_and_a_success_ends_a_failure() {
		let mut quality = Quality::default();
		quality.record_success(100);
		quality.record_failure(Instant::now());
		quality.record_success(0);
		assert_eq!(
			(quality.latency_ms, quality.successes, quality.failures, quality.failed_at),
			(Some(70), 2, 1, None)
		);
	}

	/// The host sends a read request that the payer signs for that provider. It pays for what it
	/// verified: one receipt for the same transfer and the size that it checked, to the provider
	/// that served, with the signature of the payer.
	#[tokio::test]
	async fn a_cache_hit_is_asked_and_paid_with_signatures_and_bulletin_is_not_asked() {
		let (node, requests) = fake_node(200, blob()).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
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
			SIGNING_CONTEXT,
			&read_message(transfer, &payer, &[1; 32], &cid(), read["issued_at"].as_u64().unwrap()),
			&acquire["signature"],
		);
		let receipt_signed = signed_by_test_payer(
			SIGNING_CONTEXT,
			&receipt_message(transfer, &payer, &[1; 32], &cid(), blob().len() as u64),
			&receipts[0]["signature"],
		);
		assert_eq!(
			(
				receipts.len(),
				read_signed,
				receipt_signed,
				[&read["provider"], &read["content"], &read["payer"]],
				[&receipt["transfer"], &receipt["provider"], &receipt["content"]],
				[&receipt["service"], &receipt["size"], &receipt["from"], &receipt["until"]],
			),
			(
				1,
				true,
				true,
				[&json!(hex::encode([1u8; 32])), &json!(cid()), &json!(hex::encode(payer))],
				[&json!(transfer), &json!(hex::encode([1u8; 32])), &json!(cid())],
				[&json!("Delivery"), &json!(blob().len()), &json!(0), &json!(0)],
			)
		);
	}

	/// The host does not trust a cache node. It drops bytes that do not hash to the CID, does not
	/// pay for them, and asks the next node.
	#[tokio::test]
	async fn forged_bytes_get_no_receipt_and_the_next_node_is_asked() {
		let (liar, liar_requests) = fake_node(200, b"forged".to_vec()).await;
		let (honest, honest_requests) = fake_node(200, blob()).await;
		let source = CacheFirst::new(
			cache(vec![provider(1, &liar), provider(2, &honest)], Some(test_payer())),
			bulletin(None),
		);
		// The liar measured faster, so the host asks it first.
		if let Some(cache) = &source.cache {
			let mut quality = cache.quality.lock().unwrap();
			for (id, latency_ms) in [([1; 32], 1), ([2; 32], 1000)] {
				quality.insert(id, Quality { latency_ms: Some(latency_ms), ..Default::default() });
			}
		}
		assert_eq!(
			(read(&source).await, *source.fallback.calls.lock().unwrap()),
			(Some(blob()), 0)
		);
		assert_eq!(
			(receipts(&liar_requests).await.len(), receipts(&honest_requests).await.len()),
			(0, 1)
		);
	}

	/// Cache nodes only make a read faster. When no cache node can help, Bulletin answers as it
	/// does without them.
	#[tokio::test]
	async fn nodes_that_miss_refuse_or_are_down_leave_the_read_to_bulletin() {
		let (missing, _) = fake_node(404, b"not found: not held".to_vec()).await;
		let (refusing, _) = fake_node(402, b"refused: payer has 0 credits".to_vec()).await;
		let providers =
			vec![provider(1, &missing), provider(2, &refusing), provider(3, "http://127.0.0.1:9")];
		let source = CacheFirst::new(cache(providers, Some(test_payer())), bulletin(Some(blob())));
		assert_eq!(
			(read(&source).await, *source.fallback.calls.lock().unwrap()),
			(Some(blob()), 1)
		);
	}

	/// A host that cannot pay does not ask cache nodes.
	#[tokio::test]
	async fn without_a_payer_cache_nodes_are_not_asked() {
		let (node, requests) = fake_node(200, blob()).await;
		let source = CacheFirst::new(cache(vec![provider(1, &node)], None), bulletin(Some(blob())));
		assert_eq!(
			(
				read(&source).await,
				*source.fallback.calls.lock().unwrap(),
				requests.lock().unwrap().len()
			),
			(Some(blob()), 1, 0)
		);
	}

	/// After a restart, the host knows the speed and the failure count of each provider, but not a
	/// recent failure.
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
				None,
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
		assert!(status.contains(&node) && status.contains("reads 1  failures 1"), "{status}");
		let payer = hex::encode(test_payer().public.to_bytes());
		assert!(status.starts_with(&format!("cache payer {payer}")), "{status}");
	}

	#[tokio::test]
	async fn without_cache_nodes_bulletin_answers_alone() {
		let source = CacheFirst::new(None, bulletin(Some(blob())));
		assert_eq!(
			(read(&source).await, *source.fallback.calls.lock().unwrap()),
			(Some(blob()), 1)
		);
	}

	/// The quote of a ledger at retention price 5: 30 free units for retention, 4 paid credits.
	fn account_route() -> (&'static str, u16, Vec<u8>) {
		let quote = json!({
			"price": 10,
			"retention_price": 5,
			"allowance": 100,
			"allowance_retention": 30,
			"paid": 4,
			"balance": 4,
		});
		("/account/", 200, quote.to_string().into_bytes())
	}

	/// An item for the test blob with a size of 4 started MiB, for 2 hours: 5 × 4 × 2 = 40 units.
	fn key_item(renew: bool) -> RetainItem {
		RetainItem {
			target: PreimageTarget::Key(preimage_key(&blob()).to_vec()),
			period_secs: 7200,
			size: Some(3 * MIB + 1),
			provider: None,
			renew,
		}
	}

	fn outcome(state: RetainState) -> RetainOutcome {
		RetainOutcome {
			cid: cid(),
			provider: [1; 32],
			name: None,
			region: None,
			operator: None,
			kind: RetainKind::Raw,
			size: 3 * MIB + 1,
			pieces: 1,
			until: 0,
			price: 40,
			charged: 0,
			free: 30,
			paid: 4,
			short: 6,
			renews: false,
			state,
		}
	}

	fn paths(requests: &Requests) -> Vec<String> {
		requests.lock().unwrap().iter().map(|(path, _)| path.clone()).collect()
	}

	/// A dry run shows the price of the ledger and what the allowance and the credits pay of it.
	/// It reads the account only, so it keeps and pays nothing.
	#[tokio::test]
	async fn a_dry_run_quotes_the_price_and_its_split_and_keeps_nothing() {
		let (node, requests) = routed_node(404, Vec::new(), vec![account_route()]).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
		let request = RemotePreimageRetainRequest { items: vec![key_item(false)], dry_run: true };
		let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
		let mut response = source.retain(request).await.unwrap();
		let after = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
		let until = core::mem::take(&mut response.items[0].until);
		let payer = hex::encode(test_payer().public.to_bytes());
		assert_eq!(
			(response.items, (before + 7200..=after + 7200).contains(&until), paths(&requests)),
			(vec![outcome(RetainState::Quote)], true, vec![format!("/account/{payer}")])
		);
	}

	/// A retention that does not renew pins the content with a retention authorization for its
	/// size, paid in advance. The node's answer gives the end, the pieces and the charge.
	#[tokio::test]
	async fn a_retention_of_a_key_pins_it_and_answers_kept() {
		let pin = json!({ "until": 99, "size": 3 * MIB + 1, "charged": 40, "chunks": 2 });
		let routes = vec![account_route(), ("/pin", 200, pin.to_string().into_bytes())];
		let (node, requests) = routed_node(404, Vec::new(), routes).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
		let request = RemotePreimageRetainRequest { items: vec![key_item(false)], dry_run: false };
		let response = source.retain(request).await;
		let receipt = requests.lock().unwrap()[1].1["authorization"]["receipt"].clone();
		assert_eq!(
			(
				response,
				paths(&requests).last().cloned(),
				[&receipt["service"], &receipt["content"], &receipt["size"]]
			),
			(
				Ok(RemotePreimageRetainResponse {
					items: vec![RetainOutcome {
						kind: RetainKind::File,
						pieces: 3,
						until: 99,
						charged: 40,
						..outcome(RetainState::Kept)
					}],
				}),
				Some("/pin".to_string()),
				[&json!("Retention"), &json!(cid()), &json!(3 * MIB + 1)]
			)
		);
	}

	/// The quote of a ledger that prices reads by the MiB and retention by the MiB-day, at
	/// retention price 5, with the allowance and credits of `account_route`.
	fn day_account_route() -> (&'static str, u16, Vec<u8>) {
		let quote = json!({
			"price": 10,
			"read_price_per_mib": 2,
			"retention_price": 5,
			"retention_unit_secs": 86_400,
			"allowance": 100,
			"allowance_retention": 30,
			"paid": 4,
			"balance": 4,
		});
		("/account/", 200, quote.to_string().into_bytes())
	}

	/// The account gives the units of the prices. A ledger without units has one price for each
	/// read and MiB-hours. A quote counts in the unit of the ledger: 2 hours of 4 MiB are 5 × 4 × 2
	/// = 40 units at MiB-hours, and 5 × 4 × 1 = 20 units at MiB-days. One day of a lease is 5 × 4
	/// × 24 = 480 units at MiB-hours.
	#[tokio::test]
	async fn prices_count_in_the_units_of_the_ledger() {
		let (hours, _) = routed_node(404, Vec::new(), vec![account_route()]).await;
		let (days, _) = routed_node(404, Vec::new(), vec![day_account_route()]).await;
		let source = |node: &str| {
			CacheFirst::new(cache(vec![provider(1, node)], Some(test_payer())), bulletin(None))
		};
		let (hours, days) = (source(&hours), source(&days));
		let units = async |source: &CacheFirst<Bulletin>| {
			source.cache_account(None).await.map(|account| {
				(account.read_price, account.read_price_per_mib, account.retention_unit_secs)
			})
		};
		let price = async |source: &CacheFirst<Bulletin>, renew: bool| {
			let request =
				RemotePreimageRetainRequest { items: vec![key_item(renew)], dry_run: true };
			source.retain(request).await.map(|response| response.items[0].price)
		};
		assert_eq!(
			(
				units(&hours).await,
				units(&days).await,
				price(&hours, false).await,
				price(&days, false).await,
				price(&hours, true).await,
				price(&days, true).await,
			),
			(Ok((10, 0, 3600)), Ok((10, 2, 86_400)), Ok(40), Ok(20), Ok(480), Ok(20))
		);
	}

	/// An item that renews is a lease. The payer signs a lease authorization with the price of one
	/// day as the most for each day, and the lease has no end when the period is 0. The outcome
	/// gives the price of one day and the end of the paid day from the node.
	#[tokio::test]
	async fn a_retention_that_renews_pins_it_with_a_lease() {
		let pin = json!({ "until": 86_500, "size": 3 * MIB + 1, "charged": 20, "chunks": 0 });
		let routes = vec![day_account_route(), ("/pin", 200, pin.to_string().into_bytes())];
		let (node, requests) = routed_node(404, Vec::new(), routes).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
		let item = RetainItem { period_secs: 0, ..key_item(true) };
		let request = RemotePreimageRetainRequest { items: vec![item], dry_run: false };
		let response = source.retain(request).await;
		let body = requests.lock().unwrap()[1].1["authorization"].clone();
		let receipt = &body["receipt"];
		let payer = test_payer().public.to_bytes();
		let message = lease_message(
			receipt["transfer"].as_str().unwrap(),
			&payer,
			&[1; 32],
			&cid(),
			3 * MIB + 1,
			(receipt["from"].as_u64().unwrap(), 0),
			20,
		);
		assert_eq!(
			(
				response,
				[&receipt["service"], &receipt["until"], &receipt["max_per_day"]],
				signed_by_test_payer(SIGNING_CONTEXT, &message, &body["signature"]),
			),
			(
				Ok(RemotePreimageRetainResponse {
					items: vec![RetainOutcome {
						until: 86_500,
						price: 20,
						charged: 20,
						free: 20,
						paid: 0,
						short: 0,
						renews: true,
						..outcome(RetainState::Kept)
					}],
				}),
				[&json!("Lease"), &json!(0), &json!(20)],
				true,
			)
		);
	}

	/// A dry run of a lease quotes one day, until the end of the first day. A lease with an end
	/// ends at the end of its period.
	#[tokio::test]
	async fn a_dry_run_of_a_lease_quotes_the_first_day() {
		let (node, _) = routed_node(404, Vec::new(), vec![day_account_route()]).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
		let quote = async |period_secs: u64| {
			let item = RetainItem { period_secs, ..key_item(true) };
			let request = RemotePreimageRetainRequest { items: vec![item], dry_run: true };
			let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
			let outcome = source.retain(request).await.unwrap().items.remove(0);
			(outcome.until - before, outcome.price, outcome.renews)
		};
		let (no_end, two_hours) = (quote(0).await, quote(7200).await);
		assert!((86_400..=86_401).contains(&no_end.0), "{no_end:?}");
		assert!((7200..=7201).contains(&two_hours.0), "{two_hours:?}");
		assert_eq!((no_end.1, no_end.2, two_hours.1, two_hours.2), (20, true, 20, true));
	}

	/// An item for the site `cache-demo.paseo` for 2 hours, without a size: 5 × 1 × 2 = 10 units.
	fn site_item(target: PreimageTarget) -> RetainItem {
		RetainItem { target, size: None, ..key_item(false) }
	}

	fn site_outcome(state: RetainState) -> RetainOutcome {
		RetainOutcome {
			cid: CACHE_DEMO_CID.to_string(),
			kind: RetainKind::File,
			size: CACHE_DEMO_SIZE,
			pieces: 4,
			price: 10,
			free: 10,
			paid: 0,
			short: 0,
			..outcome(state)
		}
	}

	/// A retention pays for all blocks of a site. The host reads the root block from Bulletin to
	/// learn their size: the root block and the size of each link. A dry run keeps nothing.
	#[tokio::test]
	async fn a_dry_run_of_a_site_root_quotes_the_size_of_all_its_blocks() {
		let (node, requests) = routed_node(404, Vec::new(), vec![account_route()]).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), site_blocks());
		let item = site_item(PreimageTarget::Cid(CACHE_DEMO_CID.to_string()));
		let request = RemotePreimageRetainRequest { items: vec![item], dry_run: true };
		let mut response = source.retain(request).await.unwrap();
		response.items[0].until = 0;
		let payer = hex::encode(test_payer().public.to_bytes());
		assert_eq!(
			(response.items, paths(&requests)),
			(vec![site_outcome(RetainState::Quote)], vec![format!("/account/{payer}")])
		);
	}

	/// The node keeps every block of the DAG under the root CID. The authorization pays for the
	/// size of all blocks, and the node answers the chunks after the root.
	#[tokio::test]
	async fn a_retention_of_a_site_root_pins_all_its_blocks() {
		let pin = json!({ "until": 99, "size": CACHE_DEMO_SIZE, "charged": 10, "chunks": 3 });
		let routes = vec![account_route(), ("/pin", 200, pin.to_string().into_bytes())];
		let (node, requests) = routed_node(404, Vec::new(), routes).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), site_blocks());
		let item = site_item(PreimageTarget::Cid(CACHE_DEMO_CID.to_string()));
		let request = RemotePreimageRetainRequest { items: vec![item], dry_run: false };
		let response = source.retain(request).await;
		let body = requests.lock().unwrap()[1].1.clone();
		let receipt = &body["authorization"]["receipt"];
		assert_eq!(
			(
				response,
				paths(&requests).last().cloned(),
				body["reference"].clone(),
				[&receipt["content"], &receipt["size"]],
			),
			(
				Ok(RemotePreimageRetainResponse {
					items: vec![RetainOutcome {
						until: 99,
						charged: 10,
						..site_outcome(RetainState::Kept)
					}],
				}),
				Some("/pin".to_string()),
				json!({
					"source": format!("bulletin:{CACHE_DEMO_CID}"),
					"cid": null,
					"size": CACHE_DEMO_SIZE,
				}),
				[&json!(CACHE_DEMO_CID), &json!(CACHE_DEMO_SIZE)],
			)
		);
	}

	/// The cache nodes keep raw and dag-pb content with sha2-256 or blake2b-256 hashes only. The
	/// host refuses other CIDs before it asks a node.
	#[tokio::test]
	async fn a_cid_that_the_cache_does_not_keep_is_refused() {
		let (node, requests) = routed_node(404, Vec::new(), vec![account_route()]).await;
		let source =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), site_blocks());
		// The CID of the site root with the codec dag-cbor (0x71) instead of dag-pb.
		let dag_cbor = CACHE_DEMO_CID.replacen("bafybe", "bafyre", 1);
		let item = site_item(PreimageTarget::Cid(dag_cbor));
		let request = RemotePreimageRetainRequest { items: vec![item], dry_run: true };
		assert_eq!(
			(source.retain(request).await, paths(&requests)),
			(
				Err(PreimageRetainError::Unknown { reason: content::UNSUPPORTED.to_string() }),
				vec![]
			)
		);
	}

	/// The host keeps a DotNS name by the CID of its contenthash: the contenthash of `app.<name>`,
	/// else of the name. The home nodes of the name are the home nodes of that CID.
	#[tokio::test]
	async fn a_dotns_name_is_kept_by_the_cid_of_its_contenthash() {
		let root = Cid::parse(CACHE_DEMO_CID).unwrap();
		let contenthash = [[0xe3, 0x01].as_slice(), &root.to_bytes()].concat();
		let asset_hub = fake_asset_hub(vec![("cache-demo.paseo", contenthash)]).await;
		let (node, _) = routed_node(404, Vec::new(), vec![account_route()]).await;
		let source = CacheFirst::new(
			cache_with_asset_hub(
				vec![provider(1, &node)],
				Some(test_payer()),
				Some(asset_hub.clone()),
			),
			site_blocks(),
		);
		let no_asset_hub =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), site_blocks());
		let quote = async |source: &CacheFirst<Blocks>, name: &str| {
			let item = site_item(PreimageTarget::DotNs(name.to_string()));
			let request = RemotePreimageRetainRequest { items: vec![item], dry_run: true };
			source.retain(request).await.map(|mut response| {
				response.items[0].until = 0;
				response.items
			})
		};
		let results = [
			quote(&source, "cache-demo.paseo").await,
			quote(&source, "nothing.paseo").await,
			quote(&no_asset_hub, "cache-demo.paseo").await,
		];

		let providers: Vec<Provider> =
			(1..=4).map(|n| provider(n, &format!("http://{n}"))).collect();
		let ordered =
			CacheFirst::new(cache_with_asset_hub(providers, None, Some(asset_hub)), site_blocks());
		let order = async |target: PreimageTarget| {
			ordered.cache_providers(Some(&target)).await.map(|response| response.providers)
		};
		let by_name = order(PreimageTarget::DotNs("cache-demo.paseo".to_string())).await;
		let by_cid = order(PreimageTarget::Cid(CACHE_DEMO_CID.to_string())).await;
		let by_unknown_name = order(PreimageTarget::DotNs("nothing.paseo".to_string())).await;
		assert_eq!(
			(results, by_name == by_cid, by_name == by_unknown_name),
			(
				[
					Ok(vec![site_outcome(RetainState::Quote)]),
					Err(PreimageRetainError::NotFound),
					Err(PreimageRetainError::Unknown {
						reason: "this host cannot resolve DotNS names without an Asset Hub RPC"
							.to_string(),
					}),
				],
				true,
				false,
			)
		);
	}

	/// The providers of a CID come in the order of a read of it: a home node counts at half its
	/// latency, so it goes before a faster provider that is not a home node. A provider that
	/// failed recently goes last. Without a target, no provider is a home node.
	#[tokio::test]
	async fn cache_providers_put_the_home_nodes_of_the_target_first() {
		let providers: Vec<Provider> =
			(1..=4).map(|n| provider(n, &format!("http://{n}"))).collect();
		let source = CacheFirst::new(cache(providers, None), bulletin(None));
		if let Some(cache) = &source.cache {
			let mut quality = cache.quality.lock().unwrap();
			for (n, latency_ms) in [(1, 20), (2, 30), (3, 30), (4, 30)] {
				quality.insert(
					[n; 32],
					Quality { latency_ms: Some(latency_ms), ..Default::default() },
				);
			}
			quality.get_mut(&[4; 32]).unwrap().failed_at = Some(Instant::now());
		}
		let info = |n: u8, home: bool| CacheProviderInfo {
			id: [n; 32],
			name: None,
			region: None,
			operator: None,
			source: CacheProviderSource::Registry,
			home,
			latency_ms: Some(if n == 1 { 20 } else { 30 }),
			failed_recently: n == 4,
		};
		let order = async |target: Option<PreimageTarget>| {
			source.cache_providers(target.as_ref()).await.map(|response| response.providers)
		};
		assert_eq!(
			(
				order(Some(PreimageTarget::Cid(cid()))).await,
				order(Some(PreimageTarget::Key(preimage_key(&blob()).to_vec()))).await,
				order(None).await,
				CacheFirst::new(None, bulletin(None)).cache_providers(None).await,
			),
			(
				Ok(vec![info(2, true), info(3, true), info(1, false), info(4, true)]),
				Ok(vec![info(2, true), info(3, true), info(1, false), info(4, true)]),
				Ok(vec![info(1, false), info(2, false), info(3, false), info(4, false)]),
				Err(PreimageReadError::NoCacheProviders),
			)
		);
	}

	/// The retentions of the test payer in a ledger, the latest end first. The provider `1` keeps
	/// the contents `kept`, `fetching`, `ended`, `running` and `failing`. The provider `9` is not
	/// in the provider set. `kept` is a lease that renews, paid until its end at 2 a day. The
	/// other retentions come from a ledger without leases.
	fn leases_route(now: u64) -> (&'static str, u16, Vec<u8>) {
		let lease = |content: &str, provider: u8, until: u64| {
			json!({
				"content": content,
				"provider": hex::encode([provider; 32]),
				"from": 100,
				"until": until,
				"size": 2048,
				"charged": 40,
			})
		};
		let mut renewing = lease("kept", 1, now + 7200);
		renewing["paid_until"] = json!(now + 7200);
		renewing["per_day"] = json!(2);
		renewing["renews"] = json!(true);
		let leases = json!({
			"payer": hex::encode(test_payer().public.to_bytes()),
			"leases": [
				renewing,
				lease("fetching", 1, now + 7200),
				lease("running", 1, now + 3600),
				lease("away", 9, now + 3600),
				lease("failing", 1, now + 3600),
				lease("ended", 1, 200),
			],
		});
		("/leases", 200, leases.to_string().into_bytes())
	}

	/// The answer of a cache node to `GET /pin-state/<content>` for each content of `leases_route`.
	fn pin_state_routes() -> Vec<(&'static str, u16, Vec<u8>)> {
		let state = |state: &str, blocks: u64, pieces: u64, total: u64| {
			json!({
				"content": "",
				"state": state,
				"until": 0,
				"blocks": blocks,
				"held": pieces,
				"pieces": pieces,
				"total_pieces": total,
			})
			.to_string()
			.into_bytes()
		};
		vec![
			("/pin-state/kept", 200, state("kept", 4, 4, 4)),
			("/pin-state/fetching", 200, state("fetching", 0, 2, 5)),
			("/pin-state/running", 200, state("none", 0, 0, 0)),
			("/pin-state/ended", 200, state("none", 0, 0, 0)),
			("/pin-state/failing", 500, b"down".to_vec()),
		]
	}

	/// The payer signs its request for the retentions, and the provider of the ledger gets it. The
	/// host maps the pin state of each retention on its provider. A pin state `none` is `Ended`
	/// after the end of the retention, else `Lost`. A provider that is not in the provider set, or
	/// that fails, gives `Unknown`. A retention from a ledger without leases is paid until its end.
	#[tokio::test]
	async fn retentions_are_signed_and_show_the_state_on_their_provider() {
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
		let mut routes = pin_state_routes();
		routes.push(leases_route(now));
		let (node, requests) = routed_node(404, Vec::new(), routes).await;
		let holder = Provider {
			name: Some("A".to_string()),
			region: Some("fr-par".to_string()),
			..provider(1, &node)
		};
		let source = CacheFirst::new(cache(vec![holder], Some(test_payer())), bulletin(None));
		let response = source.cache_retentions(None).await.unwrap();

		let body = requests
			.lock()
			.unwrap()
			.iter()
			.find(|(path, _)| path == "/leases")
			.unwrap()
			.1
			.clone();
		let payer = test_payer().public.to_bytes();
		let issued_at = body["request"]["issued_at"].as_u64().unwrap();
		assert_eq!(body["request"]["payer"], json!(hex::encode(payer)));
		assert!(issued_at.abs_diff(now) < 60);
		assert!(signed_by_test_payer(
			SIGNING_CONTEXT,
			&leases_message(&payer, issued_at),
			&body["signature"]
		));

		let retention =
			|cid: &str, provider: u8, until: u64, pieces: u32, state| PreimageRetention {
				cid: cid.to_string(),
				provider: [provider; 32],
				name: (provider == 1).then(|| "A".to_string()),
				region: (provider == 1).then(|| "fr-par".to_string()),
				operator: None,
				size: 2048,
				pieces,
				from: 100,
				until,
				paid_until: until,
				charged: 40,
				per_day: 0,
				renews: false,
				state,
			};
		assert_eq!(
			response,
			RemotePreimageRetentionsResponse {
				payer,
				retentions: vec![
					PreimageRetention {
						per_day: 2,
						renews: true,
						..retention("kept", 1, now + 7200, 4, RetentionState::Kept)
					},
					retention(
						"fetching",
						1,
						now + 7200,
						1,
						RetentionState::Fetching { pieces: 2, total: 5 }
					),
					retention("running", 1, now + 3600, 1, RetentionState::Lost),
					retention("away", 9, now + 3600, 1, RetentionState::Unknown),
					retention("failing", 1, now + 3600, 1, RetentionState::Unknown),
					retention("ended", 1, 200, 1, RetentionState::Ended),
				],
			}
		);
	}

	/// Without a payer, the host does not ask the ledger. Without cache nodes, there are no cache
	/// providers. A refusal of the ledger is the reason of the error.
	#[tokio::test]
	async fn retentions_need_a_payer_and_a_ledger_that_accepts_the_request() {
		let refusal = ("/leases", 400, b"rejected: stale".to_vec());
		let (node, requests) = routed_node(404, Vec::new(), vec![refusal]).await;
		let unpaid = CacheFirst::new(cache(vec![provider(1, &node)], None), bulletin(None));
		let paid =
			CacheFirst::new(cache(vec![provider(1, &node)], Some(test_payer())), bulletin(None));
		let unpaid = unpaid.cache_retentions(None).await;
		let asked = paths(&requests);
		assert_eq!(
			(
				matches!(unpaid, Err(PreimageReadError::Unknown { reason }) if reason.starts_with("no cache payer yet")),
				asked,
				CacheFirst::new(None, bulletin(None)).cache_retentions(None).await,
				paid.cache_retentions(Some([2; 32])).await,
				paid.cache_retentions(None).await,
			),
			(
				true,
				Vec::<String>::new(),
				Err(PreimageReadError::NoCacheProviders),
				Err(PreimageReadError::UnknownProvider),
				Err(PreimageReadError::Unknown {
					reason: "400 Bad Request: rejected: stale".to_string()
				}),
			)
		);
	}

	/// The payer signs a stop request for the content and the provider, and the provider of the
	/// retention gets it. The answer is the end of the paid time in the ledger after the stop. A
	/// provider that is not in the provider set gets no request.
	#[tokio::test]
	async fn a_stop_is_signed_and_answers_the_end_of_the_paid_time() {
		let leases = json!({
			"payer": hex::encode(test_payer().public.to_bytes()),
			"leases": [{
				"content": cid(),
				"provider": hex::encode([1u8; 32]),
				"from": 100,
				"until": 86_500,
				"size": 2048,
				"charged": 4,
				"paid_until": 86_500,
				"per_day": 0,
				"renews": false,
			}],
		});
		let routes = vec![
			("/leases/stop", 200, b"{\"stopped\":1}".to_vec()),
			("/leases", 200, leases.to_string().into_bytes()),
		];
		let (node, requests) = routed_node(404, Vec::new(), routes).await;
		let source = CacheFirst::new(
			cache(vec![provider(1, &node), provider(2, &node)], Some(test_payer())),
			bulletin(None),
		);
		let stop = async |provider: [u8; 32]| {
			let request = RemotePreimageStopRetentionRequest { cid: cid(), provider };
			source.stop_retention(request).await
		};
		let results = (stop([1; 32]).await, stop([2; 32]).await, stop([9; 32]).await);
		let body = requests.lock().unwrap()[0].1.clone();
		let payer = test_payer().public.to_bytes();
		let issued_at = body["request"]["issued_at"].as_u64().unwrap();
		let signed = signed_by_test_payer(
			SIGNING_CONTEXT,
			&stop_message(&payer, &[1; 32], &cid(), issued_at),
			&body["signature"],
		);
		assert_eq!(
			(
				results,
				signed,
				body["request"]["provider"].clone(),
				paths(&requests).into_iter().filter(|path| path == "/leases/stop").count(),
			),
			(
				(
					Ok(RemotePreimageStopRetentionResponse { until: 86_500 }),
					Ok(RemotePreimageStopRetentionResponse { until: 0 }),
					Err(PreimageRetainError::Unknown {
						reason: "the cache provider is not in the provider set".to_string(),
					}),
				),
				true,
				json!(hex::encode([1u8; 32])),
				2,
			)
		);
	}
}
