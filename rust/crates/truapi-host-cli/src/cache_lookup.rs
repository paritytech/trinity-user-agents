//! Preimage lookup through cache nodes first, then the Bulletin node.
//!
//! A cache node keeps verified copies of Bulletin blobs near its users and serves them faster than a Bulletin node.
//! `TRUAPI_CACHE_NODES` names the cache nodes in the order to ask. For each read the host asks them with
//! `POST /acquire` for `bulletin:<cid>`, and asks the Bulletin node only when no cache node supplies the blob. The host
//! does not trust a cache node: it checks that the bytes hash to the CID, and asks the next node when they do not. A
//! cache node that is down, refuses the payer or does not have the blob costs one request, never the lookup.

use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::json;
use tracing::{debug, info, warn};
use truapi::{preimage_cid, preimage_key};

use crate::bulletin_lookup::{BlobSource, SourceError};

/// Env var with the cache nodes to ask before Bulletin: base URLs separated by commas, for example
/// `http://127.0.0.1:8081,http://127.0.0.1:8082`.
const CACHE_NODES_ENV: &str = "TRUAPI_CACHE_NODES";
/// Env var with the name that pays the cache nodes in their ledger.
const CACHE_CLIENT_ENV: &str = "TRUAPI_CACHE_CLIENT";
const DEFAULT_CLIENT: &str = "truapi";
/// Bound on one request to one cache node. A node that does not have the blob reads it from Bulletin first.
const NODE_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The cache nodes to ask, in order, and the name that pays them.
pub struct CacheNodes {
    http: reqwest::Client,
    nodes: Vec<String>,
    client: String,
    requests: AtomicU64,
}

impl CacheNodes {
    /// The cache nodes that `TRUAPI_CACHE_NODES` names, paid as `TRUAPI_CACHE_CLIENT`, or `None` when it names none.
    pub fn from_env() -> Option<Self> {
        let nodes = parse_nodes(&std::env::var(CACHE_NODES_ENV).unwrap_or_default());
        if nodes.is_empty() {
            return None;
        }
        let client = std::env::var(CACHE_CLIENT_ENV)
            .ok()
            .filter(|client| !client.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CLIENT.to_string());
        info!(?nodes, %client, "preimage lookups ask these cache nodes before Bulletin");
        Self::new(nodes, client)
    }

    fn new(nodes: Vec<String>, client: String) -> Option<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(NODE_TIMEOUT)
            .build();
        match http {
            Ok(http) => Some(Self {
                http,
                nodes,
                client,
                requests: AtomicU64::new(0),
            }),
            Err(error) => {
                warn!(%error, "no HTTP client for the cache nodes, so lookups go to Bulletin only");
                None
            }
        }
    }

    /// The blob under `cid` from the first cache node that sends bytes that hash to it.
    async fn read(&self, cid: &str) -> Option<Vec<u8>> {
        for node in &self.nodes {
            match self.ask(node, cid).await {
                Ok(Some((value, origin))) if preimage_cid(&preimage_key(&value)) == cid => {
                    info!(
                        cid,
                        node,
                        origin,
                        size = value.len(),
                        "preimage read from a cache node"
                    );
                    return Some(value);
                }
                Ok(Some(_)) => warn!(
                    cid,
                    node, "cache node sent bytes that do not hash to the CID"
                ),
                Ok(None) => debug!(cid, node, "cache node cannot find the preimage"),
                Err(reason) => warn!(cid, node, %reason, "cache node failed"),
            }
        }
        None
    }

    /// One `POST /acquire`: the bytes and the source that the node reports, or `None` when the node answers that
    /// Bulletin does not hold the blob.
    async fn ask(&self, node: &str, cid: &str) -> Result<Option<(Vec<u8>, String)>, String> {
        let request = json!({
            "reference": { "source": format!("bulletin:{cid}"), "hash": null, "size": null },
            "client": self.client,
            "transfer": self.transfer_id(),
        });
        let response = self
            .http
            .post(format!("{node}/acquire"))
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

    /// A transfer id that the ledger of the cache sees once, so a read is never taken as a retry of another.
    fn transfer_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let count = self.requests.fetch_add(1, Ordering::Relaxed);
        format!("{}-{nanos}-{count}", self.client)
    }
}

/// Base URLs from a list separated by commas, without blanks or a final `/`.
fn parse_nodes(list: &str) -> Vec<String> {
    list.split(',')
        .map(|node| node.trim().trim_end_matches('/'))
        .filter(|node| !node.is_empty())
        .map(str::to_string)
        .collect()
}

/// A blob source that asks the cache nodes first, when there are any, and then `fallback`.
pub struct CacheFirst<S> {
    cache: Option<CacheNodes>,
    fallback: S,
}

impl<S> CacheFirst<S> {
    /// Ask `cache` before `fallback`. Without cache nodes, `fallback` answers alone.
    pub fn new(cache: Option<CacheNodes>, fallback: S) -> Self {
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
    use std::sync::{Arc, Mutex};

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

    /// A cache node on loopback that answers every request with `status` and `body`, and keeps the request bodies.
    async fn fake_node(status: u16, body: Vec<u8>) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_body(&mut stream).await;
                seen.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&request).unwrap());
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

    async fn read_body(stream: &mut TcpStream) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            buffer.extend_from_slice(&chunk[..read]);
            let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&buffer[..end]).to_ascii_lowercase();
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map_or(0, |length| length.trim().parse().unwrap());
            while buffer.len() < end + 4 + length {
                let read = stream.read(&mut chunk).await.unwrap();
                buffer.extend_from_slice(&chunk[..read]);
            }
            return buffer[end + 4..end + 4 + length].to_vec();
        }
    }

    /// The answer of `source` for the test blob. These sources never fail a read: they report a miss instead.
    async fn read<S: BlobSource>(source: &CacheFirst<S>) -> Option<Vec<u8>> {
        match source.get(&cid()).await {
            Ok(value) => value,
            Err(_) => panic!("the read failed instead of a miss"),
        }
    }

    fn cache(nodes: &[&str]) -> Option<CacheNodes> {
        CacheNodes::new(
            nodes.iter().map(|node| node.to_string()).collect(),
            "alice".into(),
        )
    }

    #[tokio::test]
    async fn a_cache_node_with_the_blob_answers_and_bulletin_is_not_asked() {
        let (node, requests) = fake_node(200, blob()).await;
        let source = CacheFirst::new(cache(&[&node]), bulletin(None));
        let value = read(&source).await;
        let request = requests.lock().unwrap()[0].clone();
        assert_eq!(
            (
                value,
                *source.fallback.calls.lock().unwrap(),
                request["reference"]["source"].clone(),
                request["client"].clone()
            ),
            (
                Some(blob()),
                0,
                json!(format!("bulletin:{}", cid())),
                json!("alice")
            )
        );
    }

    /// A cache node is not trusted: bytes that do not hash to the CID are dropped, and the next node is asked.
    #[tokio::test]
    async fn bytes_that_do_not_hash_to_the_cid_go_to_the_next_node() {
        let (liar, _) = fake_node(200, b"forged".to_vec()).await;
        let (honest, _) = fake_node(200, blob()).await;
        let source = CacheFirst::new(cache(&[&liar, &honest]), bulletin(None));
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 0)
        );
    }

    /// Cache nodes only make a read faster: when none of them can help, Bulletin answers as without them.
    #[tokio::test]
    async fn nodes_that_miss_refuse_or_are_down_leave_the_read_to_bulletin() {
        let (missing, _) = fake_node(404, b"not found: not held".to_vec()).await;
        let (refusing, _) = fake_node(402, b"refused: client alice has 0 credits".to_vec()).await;
        let source = CacheFirst::new(
            cache(&[&missing, &refusing, "http://127.0.0.1:9"]),
            bulletin(Some(blob())),
        );
        assert_eq!(
            (read(&source).await, *source.fallback.calls.lock().unwrap()),
            (Some(blob()), 1)
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

    #[test]
    fn node_lists() {
        assert_eq!(
            parse_nodes(" http://127.0.0.1:8081/ , ,http://cache.example:8080"),
            vec!["http://127.0.0.1:8081", "http://cache.example:8080"]
        );
        assert_eq!(parse_nodes(""), Vec::<String>::new());
    }
}
