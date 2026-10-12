//! Host-backed JSON-RPC helpers for statement-store allowance registration.

use core::time::Duration;
use std::collections::HashMap;

use futures::{FutureExt, pin_mut};
use serde_json::{Value, json};
use subxt_rpcs::RpcClient as HostRpcClient;
#[cfg(not(target_arch = "wasm32"))]
use subxt_rpcs::client::RpcClient as NativeRpcClient;
use subxt_rpcs::client::{RpcParams, rpc_params};
use thiserror::Error;

use super::StatementAllowanceError;

/// Timeout for an allowance registration extrinsic to reach a block.
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Error from the native JSON-RPC surface used by allowance allocation.
#[derive(Debug, Error)]
pub enum RpcError {
    /// Opening a direct RPC URL failed.
    #[error("connect {url}: {source}")]
    Connect {
        /// RPC URL.
        url: String,
        /// RPC failure.
        #[source]
        source: subxt_rpcs::Error,
    },
    /// JSON-RPC request failed.
    #[error("{method}: {source}")]
    Request {
        /// RPC method.
        method: String,
        /// RPC failure.
        #[source]
        source: subxt_rpcs::Error,
    },
    /// RPC params were not supplied as a JSON array.
    #[error("RPC params must be a JSON array")]
    ParamsNotArray,
    /// Encoding one JSON-RPC param failed.
    #[error("RPC param encode failed: {0}")]
    ParamEncode(#[source] subxt_rpcs::Error),
    /// `state_getStorage` returned invalid hex.
    #[error("decode hex storage value: {0}")]
    StorageHex(#[source] hex::FromHexError),
    /// `chain_getFinalizedHead` did not return a hash string.
    #[error("chain_getFinalizedHead returned non-string")]
    FinalizedHeadNotString,
    /// Extrinsic status subscription ended before inclusion.
    #[error("author_submitAndWatchExtrinsic subscription ended")]
    SubmitSubscriptionEnded,
    /// Extrinsic status subscription timed out before inclusion.
    #[error("timed out waiting for author_submitAndWatchExtrinsic inclusion")]
    SubmitTimeout,
    /// Extrinsic status subscription yielded a terminal rejection status.
    #[error("extrinsic {status}")]
    ExtrinsicRejected {
        /// Terminal status key.
        status: String,
    },
}

/// Thin adapter matching the allowance allocator's minimal RPC surface.
#[derive(Clone)]
pub struct RpcClient {
    inner: HostRpcClient,
}

impl RpcClient {
    /// Open a JSON-RPC connection to `url`.
    ///
    /// Native only. A browser cannot dial a URL from Rust; there the host
    /// supplies the connection and the client is built with [`Self::new`].
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn connect(url: &str) -> Result<Self, StatementAllowanceError> {
        let inner = NativeRpcClient::from_insecure_url(url)
            .await
            .map_err(|err| RpcError::Connect {
                url: url.to_string(),
                source: err,
            })?;
        Ok(Self { inner })
    }

    /// Wrap a platform-backed Subxt RPC client.
    pub fn new(inner: HostRpcClient) -> Self {
        Self { inner }
    }

    /// Call `method` with JSON-array `params`, returning the result value.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, StatementAllowanceError> {
        self.inner
            .request(method, value_to_params(params)?)
            .await
            .map_err(|err| {
                RpcError::Request {
                    method: method.to_string(),
                    source: err,
                }
                .into()
            })
    }

    /// `state_getStorage(key)` at the current best block -> raw value bytes,
    /// or `None` if absent.
    pub async fn get_storage(
        &self,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, StatementAllowanceError> {
        self.get_storage_maybe_at(key, None).await
    }

    /// `state_getStorage(key, at)` pinned to block `at` -> raw value bytes,
    /// or `None` if absent.
    pub async fn get_storage_at(
        &self,
        key: &[u8],
        at: &str,
    ) -> Result<Option<Vec<u8>>, StatementAllowanceError> {
        self.get_storage_maybe_at(key, Some(at)).await
    }

    async fn get_storage_maybe_at(
        &self,
        key: &[u8],
        at: Option<&str>,
    ) -> Result<Option<Vec<u8>>, StatementAllowanceError> {
        let key_hex = format!("0x{}", hex::encode(key));
        let params = match at {
            Some(at) => rpc_params![key_hex, at],
            None => rpc_params![key_hex],
        };
        match self
            .inner
            .request::<Value>("state_getStorage", params)
            .await
            .map_err(|err| RpcError::Request {
                method: "state_getStorage".to_string(),
                source: err,
            })? {
            Value::String(hex_value) => Ok(Some(decode_hex(&hex_value)?)),
            _ => Ok(None),
        }
    }

    /// `state_queryStorageAt(keys)` at the current best block -> each key's raw
    /// value in the order asked, `None` where the key is absent.
    ///
    /// One round trip for the whole set. Scanning a slot table key by key costs a
    /// round trip per slot, which dominates everything else the scan does.
    pub async fn get_storage_many(
        &self,
        keys: &[Vec<u8>],
    ) -> Result<Vec<Option<Vec<u8>>>, StatementAllowanceError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let hex_keys: Vec<String> = keys
            .iter()
            .map(|key| format!("0x{}", hex::encode(key)))
            .collect();
        let response = self
            .inner
            .request::<Value>("state_queryStorageAt", rpc_params![hex_keys.clone()])
            .await
            .map_err(|err| RpcError::Request {
                method: "state_queryStorageAt".to_string(),
                source: err,
            })?;
        // `[{ block, changes: [[key, value|null], ..] }]`, and the changes are not
        // required to come back in the order asked, so index them by key.
        let mut found: HashMap<&str, &str> = HashMap::new();
        let changes = response
            .as_array()
            .and_then(|blocks| blocks.first())
            .and_then(|block| block.get("changes"))
            .and_then(Value::as_array);
        for change in changes.into_iter().flatten() {
            let Some(pair) = change.as_array() else {
                continue;
            };
            if let [Value::String(key), Value::String(value)] = pair.as_slice() {
                found.insert(key.as_str(), value.as_str());
            }
        }
        hex_keys
            .iter()
            .map(|key| match found.get(key.as_str()) {
                Some(value) => decode_hex(value).map(Some),
                None => Ok(None),
            })
            .collect()
    }

    /// `chain_getFinalizedHead` -> hash of the latest finalized block.
    pub async fn finalized_head(&self) -> Result<String, StatementAllowanceError> {
        let value = self.call("chain_getFinalizedHead", json!([])).await?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| RpcError::FinalizedHeadNotString.into())
    }

    /// Submit an extrinsic and wait for `inBlock` or `finalized`; returns the block hash.
    pub async fn submit_and_watch(
        &self,
        extrinsic: &[u8],
    ) -> Result<String, StatementAllowanceError> {
        let extrinsic_hex = format!("0x{}", hex::encode(extrinsic));
        let mut subscription = self
            .inner
            .subscribe::<Value>(
                "author_submitAndWatchExtrinsic",
                rpc_params![extrinsic_hex],
                "author_unwatchExtrinsic",
            )
            .await
            .map_err(|err| RpcError::Request {
                method: "author_submitAndWatchExtrinsic".to_string(),
                source: err,
            })?;
        let timeout = futures_timer::Delay::new(SUBMIT_TIMEOUT).fuse();
        pin_mut!(timeout);

        loop {
            let next = subscription.next().fuse();
            pin_mut!(next);
            let status = futures::select! {
                item = next => item.ok_or_else(|| {
                    RpcError::SubmitSubscriptionEnded
                })?.map_err(|err| RpcError::Request {
                    method: "author_submitAndWatchExtrinsic".to_string(),
                    source: err,
                })?,
                () = timeout => return Err(RpcError::SubmitTimeout.into()),
            };
            tracing::debug!(?status, "allowance extrinsic status");
            match extrinsic_status(&status) {
                ExtrinsicStatus::Included(hash) => return Ok(hash),
                ExtrinsicStatus::Rejected(reason) => {
                    return Err(RpcError::ExtrinsicRejected { status: reason }.into());
                }
                ExtrinsicStatus::Pending => {}
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum ExtrinsicStatus {
    Included(String),
    Rejected(String),
    Pending,
}

fn extrinsic_status(status: &Value) -> ExtrinsicStatus {
    for key in ["finalized", "inBlock"] {
        if let Some(hash) = status.get(key).and_then(Value::as_str) {
            return ExtrinsicStatus::Included(hash.to_string());
        }
    }
    for key in [
        "invalid",
        "dropped",
        "usurped",
        "retracted",
        "finalityTimeout",
    ] {
        if status.get(key).is_some() {
            return ExtrinsicStatus::Rejected(key.to_string());
        }
    }
    ExtrinsicStatus::Pending
}

fn value_to_params(value: Value) -> Result<RpcParams, StatementAllowanceError> {
    let Value::Array(values) = value else {
        return Err(RpcError::ParamsNotArray.into());
    };
    let mut params = RpcParams::new();
    for value in values {
        params.push(value).map_err(RpcError::ParamEncode)?;
    }
    Ok(params)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, StatementAllowanceError> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|err| RpcError::StorageHex(err).into())
}

#[cfg(test)]
pub mod testing {
    //! Scripted JSON-RPC transport for exercising request shapes in tests.

    use std::sync::{Arc, Mutex};

    use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClientT};

    /// Records every request as `(method, params)` and replays canned JSON
    /// results in order; subscriptions replay the scripted notification items.
    #[derive(Clone, Default)]
    pub struct ScriptedRpc(Arc<Inner>);

    #[derive(Default)]
    struct Inner {
        calls: Mutex<Vec<(String, String)>>,
        responses: Mutex<Vec<String>>,
        subscription_batches: Mutex<Vec<Vec<String>>>,
        subscription_errors: Mutex<Vec<String>>,
    }

    impl ScriptedRpc {
        /// A script answering requests with `responses`, in order.
        pub fn new<'a>(responses: impl IntoIterator<Item = &'a str>) -> Self {
            let scripted = Self::default();
            *scripted.0.responses.lock().unwrap() =
                responses.into_iter().map(str::to_owned).collect();
            scripted
        }

        /// Queue the notification items for one subscription. Call once per
        /// expected submission; batches are replayed in order.
        pub fn script_subscription<'a>(&self, items: impl IntoIterator<Item = &'a str>) {
            self.0
                .subscription_batches
                .lock()
                .unwrap()
                .push(items.into_iter().map(str::to_owned).collect());
        }

        /// Fail the next `n` subscriptions with `message`, as the node does when
        /// it rejects a submission outright.
        pub fn script_subscription_errors(&self, message: &str, count: usize) {
            *self.0.subscription_errors.lock().unwrap() =
                std::iter::repeat_n(message.to_owned(), count).collect();
        }

        /// The `(method, params)` pairs seen so far.
        pub fn calls(&self) -> Vec<(String, String)> {
            self.0.calls.lock().unwrap().clone()
        }
    }

    /// Answer a batched storage read with one scripted answer per key, in key
    /// order, as if each key had been read on its own.
    fn batched_storage_answer(responses: &mut Vec<String>, params: &str) -> String {
        let params: serde_json::Value =
            serde_json::from_str(params).expect("batched read params are JSON");
        let keys = params[0].as_array().expect("batched read names its keys");
        assert!(
            responses.len() >= keys.len(),
            "unscripted batched read of {} keys",
            keys.len()
        );
        let changes: Vec<serde_json::Value> = keys
            .iter()
            .map(|key| {
                let value: serde_json::Value = serde_json::from_str(&responses.remove(0))
                    .expect("scripted response is valid JSON");
                serde_json::json!([key, value])
            })
            .collect();
        serde_json::json!([{ "block": "0xscripted", "changes": changes }]).to_string()
    }

    fn params_json(params: Option<Box<RawValue>>) -> String {
        params.map_or_else(|| "[]".to_string(), |p| p.get().to_owned())
    }

    impl RpcClientT for ScriptedRpc {
        fn request_raw<'a>(
            &'a self,
            method: &'a str,
            params: Option<Box<RawValue>>,
        ) -> RawRpcFuture<'a, Box<RawValue>> {
            let params = params_json(params);
            self.0
                .calls
                .lock()
                .unwrap()
                .push((method.to_owned(), params.clone()));
            let mut responses = self.0.responses.lock().unwrap();
            assert!(!responses.is_empty(), "unscripted request `{method}`");
            let response = if method == "state_queryStorageAt" {
                batched_storage_answer(&mut responses, &params)
            } else {
                responses.remove(0)
            };
            Box::pin(async move {
                Ok(RawValue::from_string(response).expect("scripted response is valid JSON"))
            })
        }

        fn subscribe_raw<'a>(
            &'a self,
            sub: &'a str,
            params: Option<Box<RawValue>>,
            _unsub: &'a str,
        ) -> RawRpcFuture<'a, RawRpcSubscription> {
            self.0
                .calls
                .lock()
                .unwrap()
                .push((sub.to_owned(), params_json(params)));
            let failure = {
                let mut errors = self.0.subscription_errors.lock().unwrap();
                (!errors.is_empty()).then(|| errors.remove(0))
            };
            if let Some(message) = failure {
                return Box::pin(async move { Err(subxt_rpcs::Error::Client(message.into())) });
            }
            let batch = {
                let mut batches = self.0.subscription_batches.lock().unwrap();
                if batches.is_empty() {
                    Vec::new()
                } else {
                    batches.remove(0)
                }
            };
            let items: Vec<_> = batch
                .into_iter()
                .map(|item| Ok(RawValue::from_string(item).expect("scripted item is valid JSON")))
                .collect();
            Box::pin(async move {
                Ok(RawRpcSubscription {
                    stream: Box::pin(futures::stream::iter(items)),
                    id: Some("scripted".to_string()),
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testing::ScriptedRpc;
    use super::{ExtrinsicStatus, HostRpcClient, RpcClient, extrinsic_status};

    #[test]
    fn in_block_status_completes_submission() {
        let status = extrinsic_status(&json!({"inBlock": "0x1234"}));

        assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0x1234"));
    }

    #[test]
    fn finalized_status_completes_submission() {
        let status = extrinsic_status(&json!({"finalized": "0xabcd"}));

        assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0xabcd"));
    }

    #[test]
    fn terminal_pool_statuses_reject_the_submission() {
        let statuses: Vec<ExtrinsicStatus> = [
            "invalid",
            "dropped",
            "usurped",
            "retracted",
            "finalityTimeout",
        ]
        .into_iter()
        .map(|key| extrinsic_status(&json!({key: "0x1234"})))
        .collect();

        assert_eq!(
            statuses,
            vec![
                ExtrinsicStatus::Rejected("invalid".to_string()),
                ExtrinsicStatus::Rejected("dropped".to_string()),
                ExtrinsicStatus::Rejected("usurped".to_string()),
                ExtrinsicStatus::Rejected("retracted".to_string()),
                ExtrinsicStatus::Rejected("finalityTimeout".to_string()),
            ],
        );
    }

    #[test]
    fn get_storage_at_pins_the_read_to_a_block() {
        let scripted = ScriptedRpc::new([r#""0x0102""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let value = futures::executor::block_on(rpc.get_storage_at(b"key", "0xat")).unwrap();

        assert_eq!(value, Some(vec![0x01, 0x02]));
        assert_eq!(
            scripted.calls(),
            vec![(
                "state_getStorage".to_string(),
                r#"["0x6b6579","0xat"]"#.to_string(),
            )],
        );
    }

    #[test]
    fn get_storage_reads_at_the_current_block() {
        let scripted = ScriptedRpc::new(["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let value = futures::executor::block_on(rpc.get_storage(b"key")).unwrap();

        assert_eq!(value, None);
        assert_eq!(
            scripted.calls(),
            vec![(
                "state_getStorage".to_string(),
                r#"["0x6b6579"]"#.to_string()
            )],
        );
    }

    #[test]
    fn finalized_head_returns_the_hash() {
        let scripted = ScriptedRpc::new([r#""0xfeed""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let head = futures::executor::block_on(rpc.finalized_head()).unwrap();

        assert_eq!(head, "0xfeed");
        assert_eq!(
            scripted.calls(),
            vec![("chain_getFinalizedHead".to_string(), "[]".to_string())],
        );
    }
}
