//! Preimage lookup for the CLI host: the blob behind a key, read from a Bulletin node by CID.
//!
//! Transaction storage serves every blob a node retains over bitswap, and the node exposes that
//! on its JSON-RPC as `bitswap_v1_get(cid)`. The CID is fixed by the key: CIDv1, the `raw`
//! codec, and the blake2b-256 multihash that is the preimage key itself. A lookup is therefore
//! one round trip to the endpoint the network preset already names for the chain, and the host
//! needs no IPFS stack of its own.
//!
//! A subscription emits the current answer at once. On a miss it keeps asking every
//! [`POLL_INTERVAL`] until the blob appears, since a submission from another host lands within
//! a block or two, and it ends once it has delivered a value. A node that cannot be reached is
//! reported as a miss as well, never as the end of the subscription, so a product waiting on a
//! blob survives a transport failure. A request the node can never answer, such as an invalid
//! CID or a node without `bitswap_v1_get`, ends the subscription with an error after the miss.
//! Every value is checked against the key before it is emitted.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::{self, FutureExt};
use futures::stream::{self, BoxStream, StreamExt};
use serde_json::Value;
use subxt_rpcs::UserError;
use subxt_rpcs::client::{RpcClient, rpc_params};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, info, warn};
use truapi::latest as api;
use truapi::{preimage_cid, preimage_key};

/// How long a miss waits before asking the node again: about one Bulletin block.
const POLL_INTERVAL: Duration = Duration::from_secs(6);
/// Bound on one round trip to the node.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

// `bitswap_v1_get` errors by code, the only part of the error the bitswap JSON-RPC spec
// (`bitswap_unstable_get`) makes stable.
/// The data is not held. It can still arrive, so a lookup keeps asking.
const FAIL: i32 = -32810;
/// The CID is invalid or unsupported, and must not be sent again.
const INVALID_PARAMS: i32 = -32602;
/// The node has no `bitswap_v1_get`.
const METHOD_NOT_FOUND: i32 = -32601;

/// Why a source could not answer.
pub enum SourceError {
    /// Worth asking again: the node was unreachable, busy, or dropped the connection.
    Transient(String),
    /// The node can never answer this request, so the lookup stops.
    Permanent(String),
}

/// Where blobs are fetched from, by CID. The seam the subscription logic is tested through.
#[async_trait]
pub trait BlobSource: Send + Sync {
    /// The blob stored under `cid`, or `None` when the source does not hold it.
    async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError>;
}

/// `bitswap_v1_get` on a Bulletin node's JSON-RPC. The connection is opened on first use and
/// dropped after a transport failure, so the next call reconnects.
pub struct BitswapRpc {
    url: &'static str,
    client: AsyncMutex<Option<RpcClient>>,
}

impl BitswapRpc {
    /// A source for the Bulletin node at `url`. Nothing connects before the first lookup.
    pub fn new(url: &'static str) -> Self {
        Self {
            url,
            client: AsyncMutex::new(None),
        }
    }

    async fn client(&self) -> Result<RpcClient, SourceError> {
        let mut slot = self.client.lock().await;
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let client = tokio::time::timeout(RPC_TIMEOUT, RpcClient::from_insecure_url(self.url))
            .await
            .map_err(|_| SourceError::Transient(format!("connecting to {} timed out", self.url)))?
            .map_err(|err| SourceError::Transient(format!("connecting to {}: {err}", self.url)))?;
        *slot = Some(client.clone());
        Ok(client)
    }

    async fn disconnect(&self) {
        *self.client.lock().await = None;
    }
}

#[async_trait]
impl BlobSource for BitswapRpc {
    async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
        let client = self.client().await?;
        let request = client.request::<Value>("bitswap_v1_get", rpc_params![cid]);
        let value = match tokio::time::timeout(RPC_TIMEOUT, request).await {
            Ok(Ok(value)) => value,
            Ok(Err(subxt_rpcs::Error::User(error))) => {
                // A load-balanced endpoint may put the next connection on a node that has it.
                if error.code == METHOD_NOT_FOUND {
                    self.disconnect().await;
                }
                return call_error(&error);
            }
            Ok(Err(error)) => {
                self.disconnect().await;
                return Err(SourceError::Transient(format!("bitswap_v1_get: {error}")));
            }
            Err(_) => {
                self.disconnect().await;
                return Err(SourceError::Transient(
                    "bitswap_v1_get timed out".to_string(),
                ));
            }
        };
        let hex = value.as_str().and_then(|value| value.strip_prefix("0x"));
        match hex.map(hex::decode) {
            Some(Ok(bytes)) => Ok(Some(bytes)),
            _ => Err(SourceError::Transient(format!(
                "bitswap_v1_get: expected 0x-prefixed hex, got {value}"
            ))),
        }
    }
}

/// A `bitswap_v1_get` call error, by code. A CID the node does not hold is answered with an
/// error, not with null.
fn call_error(error: &UserError) -> Result<Option<Vec<u8>>, SourceError> {
    let reason = format!("bitswap_v1_get: {} ({})", error.message, error.code);
    match error.code {
        FAIL => Ok(None),
        INVALID_PARAMS | METHOD_NOT_FOUND => Err(SourceError::Permanent(reason)),
        _ => Err(SourceError::Transient(reason)),
    }
}

/// Lookups over one blob source: the CID for each key, and the check that what comes back
/// hashes to it.
pub struct BulletinLookup<S> {
    source: S,
    poll_interval: Duration,
}

impl<S: BlobSource + 'static> BulletinLookup<S> {
    /// Lookups over `source` that ask again every [`POLL_INTERVAL`] after a miss.
    pub fn new(source: S) -> Self {
        Self::with_poll_interval(source, POLL_INTERVAL)
    }

    fn with_poll_interval(source: S, poll_interval: Duration) -> Self {
        Self {
            source,
            poll_interval,
        }
    }

    /// The stream `PreimageHost::lookup_preimage` returns for `key`: the current answer at once,
    /// then the value once it can be read, then the end.
    pub fn subscribe(
        self: &Arc<Self>,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, api::GenericError>> {
        let Ok(key) = <[u8; 32]>::try_from(key.as_slice()) else {
            // Not a blake2b-256 digest, so nothing can ever hash to it.
            return stream::once(future::ready(Ok(None))).boxed();
        };
        let lookup = Arc::clone(self);
        async move {
            let cid = preimage_cid(&key);
            match lookup.read(&cid, &key).await {
                Ok(Some(value)) => return stream::once(future::ready(Ok(Some(value)))).boxed(),
                Ok(None) => {}
                Err(SourceError::Transient(reason)) => warn!(
                    key = %hex::encode(key),
                    %reason,
                    "preimage lookup failed, still trying"
                ),
                Err(SourceError::Permanent(reason)) => {
                    return stream::iter([Ok(None), Err(api::GenericError { reason })]).boxed();
                }
            }
            let held = lookup.until_held(cid, key).map(|held| held.map(Some));
            stream::once(future::ready(Ok(None)))
                .chain(stream::once(held))
                .boxed()
        }
        .flatten_stream()
        .boxed()
    }

    /// Asks again every poll interval until the source holds the blob, or fails once it never
    /// can.
    async fn until_held(
        self: Arc<Self>,
        cid: String,
        key: [u8; 32],
    ) -> Result<Vec<u8>, api::GenericError> {
        loop {
            tokio::time::sleep(self.poll_interval).await;
            match self.read(&cid, &key).await {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => {}
                Err(SourceError::Transient(reason)) => debug!(
                    key = %hex::encode(key),
                    %reason,
                    "preimage lookup failed, still trying"
                ),
                Err(SourceError::Permanent(reason)) => return Err(api::GenericError { reason }),
            }
        }
    }

    /// One read: the blob if the source holds it and it hashes to `key`.
    async fn read(&self, cid: &str, key: &[u8; 32]) -> Result<Option<Vec<u8>>, SourceError> {
        let Some(value) = self.source.get(cid).await? else {
            return Ok(None);
        };
        if preimage_key(&value) != *key {
            warn!(
                key = %hex::encode(key),
                "preimage source returned bytes that do not hash to the key"
            );
            return Ok(None);
        }
        info!(key = %hex::encode(key), size = value.len(), "preimage read from Bulletin");
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    /// A source that answers from a script, one entry per call, and counts the calls.
    struct Scripted {
        answers: Mutex<VecDeque<Result<Option<Vec<u8>>, SourceError>>>,
        calls: Mutex<u32>,
    }

    #[async_trait]
    impl BlobSource for Scripted {
        async fn get(&self, _cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
            *self.calls.lock().unwrap() += 1;
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted source asked more often than scripted")
        }
    }

    fn blob() -> Vec<u8> {
        b"bulletin lookup test blob".to_vec()
    }

    fn scripted(
        answers: Vec<Result<Option<Vec<u8>>, SourceError>>,
    ) -> Arc<BulletinLookup<Scripted>> {
        let source = Scripted {
            answers: Mutex::new(answers.into()),
            calls: Mutex::new(0),
        };
        Arc::new(BulletinLookup::with_poll_interval(
            source,
            Duration::from_millis(1),
        ))
    }

    /// Every item of the subscription, errors by their reason.
    async fn events(
        lookup: &Arc<BulletinLookup<Scripted>>,
        key: &[u8],
    ) -> Vec<Result<Option<Vec<u8>>, String>> {
        lookup
            .subscribe(key.to_vec())
            .map(|item| item.map_err(|error| error.reason))
            .collect()
            .await
    }

    fn calls(lookup: &Arc<BulletinLookup<Scripted>>) -> u32 {
        *lookup.source.calls.lock().unwrap()
    }

    fn never() -> Result<Option<Vec<u8>>, SourceError> {
        Err(SourceError::Permanent("invalid CID".to_string()))
    }

    #[tokio::test]
    async fn a_held_blob_is_emitted_at_once_and_the_subscription_ends() {
        let lookup = scripted(vec![Ok(Some(blob()))]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(Some(blob()))]
        );
    }

    #[tokio::test]
    async fn a_miss_is_reported_at_once_then_the_blob_when_it_lands_then_the_end() {
        // The product hears the miss immediately, so it can show "waiting", and is not asked
        // to resubscribe: the same subscription delivers the blob once another host's
        // submission has landed.
        let lookup = scripted(vec![Ok(None), Ok(None), Ok(Some(blob()))]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(None), Ok(Some(blob()))]
        );
        assert_eq!(calls(&lookup), 3);
    }

    #[tokio::test]
    async fn bytes_that_do_not_hash_to_the_key_are_never_emitted() {
        let lookup = scripted(vec![Ok(Some(b"forged".to_vec())), Ok(Some(blob()))]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(None), Ok(Some(blob()))]
        );
    }

    #[tokio::test]
    async fn a_failing_source_is_a_miss_not_the_end_of_the_subscription() {
        let transient = Err(SourceError::Transient("node down".to_string()));
        let lookup = scripted(vec![transient, Ok(Some(blob()))]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(None), Ok(Some(blob()))]
        );
    }

    /// The product must learn that the host can never look this up, not see an ordinary end.
    #[tokio::test]
    async fn a_request_the_node_can_never_answer_ends_the_subscription_with_an_error() {
        let lookup = scripted(vec![never()]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(None), Err("invalid CID".to_string())]
        );
    }

    #[tokio::test]
    async fn a_request_that_turns_permanent_while_polling_stops_the_poll() {
        let lookup = scripted(vec![Ok(None), never()]);
        assert_eq!(
            events(&lookup, &preimage_key(&blob())).await,
            vec![Ok(None), Err("invalid CID".to_string())]
        );
        assert_eq!(calls(&lookup), 2);
    }

    #[tokio::test]
    async fn a_key_that_is_not_a_digest_is_a_miss_and_the_end() {
        let lookup = scripted(vec![]);
        assert_eq!(events(&lookup, &[9]).await, vec![Ok(None)]);
    }

    /// The bitswap spec makes only the code stable, and `Fail` covers data that is not held yet,
    /// so every `Fail` is a miss to poll on, whatever its message or data.
    #[test]
    fn call_errors_are_classified_by_code() {
        let kind = |code| {
            let error = UserError {
                code,
                message: "from the node".to_string(),
                data: None,
            };
            match call_error(&error) {
                Ok(None) => "miss",
                Ok(Some(_)) => "value",
                Err(SourceError::Transient(_)) => "retry",
                Err(SourceError::Permanent(_)) => "stop",
            }
        };
        assert_eq!(
            [FAIL, INVALID_PARAMS, METHOD_NOT_FOUND, -32811, -32812].map(kind),
            ["miss", "stop", "stop", "retry", "retry"]
        );
    }
}
