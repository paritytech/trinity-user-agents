//! Asset Hub dotNS reads over plain RPC (`state_getStorage` / `state_call`).
//!
//! The transport half of username resolution. This module supplies the two RPC
//! primitives. `truapi::host_logic::dotns_gateway` walks the contract
//! chain over them. The CLI and the in-core `chainHead_v1` lookup therefore
//! resolve identically.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use subxt_rpcs::client::reconnecting_rpc_client::FixedInterval;
use subxt_rpcs::client::{ReconnectingRpcClient, RpcClient, rpc_params};
use truapi::host_logic::dotns_gateway::{
    DotnsIdentity, DotnsTransport, DotnsViewError, VIEW_CALL_ORIGIN, account_alias_key,
    classify_labels, discover_pop_controller, encode_revive_call, label_available,
    lite_label_owner_key, resolve_labels, timestamp_now_key, view_output,
};
use truapi::platform::async_trait;

/// Env var overriding the `DotnsPopController` H160 (hex), skipping on-chain
/// discovery.
///
/// Discovery reads `DotnsGateway.DispatcherAddress`, which holds either a
/// dispatcher or the controller itself, and resolves whichever it is; the
/// override is for networks where that fails. The
/// controller is `0xCC932348606cc1f3318cADeC5A5Cd2CA447f8a4b` on paseo-next-v2
/// and previewnet; `DEPLOYMENTS.md` in paritytech/dotns is the authority per
/// network.
pub const DOTNS_POP_CONTROLLER_ENV: &str = "HOST_CLI_DOTNS_POP_CONTROLLER";

/// Redials after a dropped socket: every 5 s for 2.5 minutes, longer than the username poll
/// in `attestation` lasts. The budget applies per drop; once it is spent the reader stays
/// disconnected for the rest of the run.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const RECONNECT_ATTEMPTS: usize = 30;

/// One Asset Hub RPC connection for a batch of dotNS reads.
pub struct AssetHubReader {
    rpc: RpcClient,
    /// `DotnsPopController` once resolved; it does not change within a run.
    pop_controller: Option<[u8; 20]>,
}

impl AssetHubReader {
    /// Connects to the Asset Hub WebSocket endpoint, failing at once if it cannot. After a
    /// dropped or silent socket the connection redials on its own: the call in flight fails,
    /// and calls made meanwhile wait for the redial.
    pub async fn connect(asset_hub_ws: &str) -> Result<Self> {
        Self::connect_with_redials(asset_hub_ws, RECONNECT_INTERVAL, RECONNECT_ATTEMPTS).await
    }

    async fn connect_with_redials(
        asset_hub_ws: &str,
        interval: Duration,
        attempts: usize,
    ) -> Result<Self> {
        // The client retries its first connect under the same policy as every redial, and
        // clones the policy afresh for each. Only a redial should retry, so the policy stays
        // empty until the first connect has succeeded.
        let connected = Arc::new(AtomicBool::new(false));
        let redials = FixedInterval::new(interval).take(attempts).take_while({
            let connected = Arc::clone(&connected);
            move |_| connected.load(Ordering::Relaxed)
        });
        let client = ReconnectingRpcClient::builder()
            .retry_policy(redials)
            .build(asset_hub_ws)
            .await
            .with_context(|| format!("connect {asset_hub_ws}"))?;
        connected.store(true, Ordering::Relaxed);
        Ok(Self {
            rpc: RpcClient::new(client),
            pop_controller: None,
        })
    }

    /// Asset Hub chain time in Unix seconds. `Timestamp.Now` itself is
    /// milliseconds.
    pub async fn timestamp_secs(&self) -> Result<u64> {
        let value = self
            .raw_storage(&timestamp_now_key())
            .await?
            .context("Timestamp.Now is unset")?;
        let millis: [u8; 8] = value
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Timestamp.Now is not a u64"))?;
        Ok(u64::from_le_bytes(millis) / 1000)
    }

    /// Alias `account` registered with, per `DotnsGateway.AccountAlias`.
    pub async fn account_alias(&self, account: &[u8; 32]) -> Result<Option<[u8; 32]>> {
        let value = self.raw_storage(&account_alias_key(account)).await?;
        account_id_value("DotnsGateway.AccountAlias", value)
    }

    /// Account that reserved the dotted lite username `lite_label` through the
    /// gateway, per `DotnsGateway.LiteLabelOwner`.
    pub async fn lite_label_owner(&self, lite_label: &str) -> Result<Option<[u8; 32]>> {
        let value = self
            .raw_storage(&lite_label_owner_key(lite_label.as_bytes()))
            .await?;
        account_id_value("DotnsGateway.LiteLabelOwner", value)
    }

    /// Usernames of `account` as recorded by the dotNS contracts.
    pub async fn dotns_identity(&mut self, account: &[u8; 32]) -> Result<DotnsIdentity> {
        let controller = self.pop_controller().await?;
        let labels = resolve_labels(self, &controller, account)
            .await
            .map_err(anyhow::Error::msg)?;
        classify_labels(self, &controller, labels)
            .await
            .map_err(anyhow::Error::msg)
    }

    /// Whether the registrar would still mint `label` under the network TLD.
    /// `false` once the name is registered, whichever flow minted it.
    pub async fn label_available(&mut self, label: &str) -> Result<bool> {
        let controller = self.pop_controller().await?;
        label_available(self, &controller, label)
            .await
            .map_err(anyhow::Error::msg)
    }

    /// `DotnsPopController` address, resolved once per reader. The env override
    /// wins, otherwise on-chain discovery from `DotnsGateway.DispatcherAddress`.
    async fn pop_controller(&mut self) -> Result<[u8; 20]> {
        if let Some(controller) = self.pop_controller {
            return Ok(controller);
        }
        let controller = self.resolve_pop_controller().await?;
        self.pop_controller = Some(controller);
        Ok(controller)
    }

    async fn resolve_pop_controller(&mut self) -> Result<[u8; 20]> {
        if let Ok(value) = std::env::var(DOTNS_POP_CONTROLLER_ENV) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                let bytes = hex::decode(trimmed.strip_prefix("0x").unwrap_or(trimmed))
                    .with_context(|| format!("{DOTNS_POP_CONTROLLER_ENV} is not valid hex"))?;
                return bytes.try_into().map_err(|bytes: Vec<u8>| {
                    anyhow::anyhow!(
                        "{DOTNS_POP_CONTROLLER_ENV} must be 20 bytes, got {}",
                        bytes.len()
                    )
                });
            }
        }
        discover_pop_controller(self)
            .await
            .map_err(anyhow::Error::msg)?
            .with_context(|| {
                format!(
                    "dotNS gateway has no reachable controller; set \
                     {DOTNS_POP_CONTROLLER_ENV} to the DotnsPopController H160"
                )
            })
    }

    /// Dry-runs a contract view via the `ReviveApi_call` runtime API and returns
    /// its data.
    ///
    /// Views originate from the synthetic always-mapped account. They work
    /// regardless of the queried account's revive mapping.
    async fn raw_view(&self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
        let args = encode_revive_call(&VIEW_CALL_ORIGIN, dest, &input);
        let output: Value = self
            .rpc
            .request(
                "state_call",
                rpc_params!["ReviveApi_call", format!("0x{}", hex::encode(args))],
            )
            .await
            .map_err(|err| {
                DotnsViewError::Failed(format!("rpc state_call ReviveApi_call: {err}"))
            })?;
        let Some(output) = output.as_str() else {
            return Err(DotnsViewError::Failed(
                "state_call returned a non-string response".to_string(),
            ));
        };
        let bytes = hex::decode(output.strip_prefix("0x").unwrap_or(output)).map_err(|err| {
            DotnsViewError::Failed(format!("state_call output is not valid hex: {err}"))
        })?;
        view_output(&bytes)
    }

    /// One `state_getStorage` read at the best block.
    async fn raw_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value: Value = self
            .rpc
            .request(
                "state_getStorage",
                rpc_params![format!("0x{}", hex::encode(key))],
            )
            .await
            .context("rpc state_getStorage")?;
        value
            .as_str()
            .map(|hex_value| {
                hex::decode(hex_value.strip_prefix("0x").unwrap_or(hex_value))
                    .context("storage value is not valid hex")
            })
            .transpose()
    }
}

/// Decodes an optional 32-byte account id storage value. A value of another
/// length is an error, not "absent": treating it as absent would let a
/// registration proceed that the gateway then rejects.
fn account_id_value(entry: &str, value: Option<Vec<u8>>) -> Result<Option<[u8; 32]>> {
    value
        .map(|bytes| {
            let len = bytes.len();
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| anyhow::anyhow!("{entry} value is {len} bytes, expected 32"))
        })
        .transpose()
}

#[async_trait]
impl DotnsTransport for AssetHubReader {
    async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
        self.raw_storage(&key)
            .await
            .map_err(|err| format!("{err:#}"))
    }

    async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
        self.raw_view(dest, input).await
    }
}

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;

    /// `Timestamp.Now` as the fake node reports it, in milliseconds.
    const NOW_MS: u64 = 1_700_000_000_000;

    /// Answers `answers` JSON-RPC requests on one accepted socket with [`NOW_MS`], then returns,
    /// which drops the socket.
    async fn serve(listener: &TcpListener, answers: usize) {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let now = format!("0x{}", hex::encode(NOW_MS.to_le_bytes()));
        let mut answered = 0;
        while answered < answers {
            let Some(Ok(message)) = socket.next().await else {
                return;
            };
            let Message::Text(text) = message else {
                continue;
            };
            let request: Value = serde_json::from_str(&text).unwrap();
            let response =
                serde_json::json!({ "jsonrpc": "2.0", "id": request["id"], "result": now });
            socket
                .send(Message::Text(response.to_string()))
                .await
                .unwrap();
            answered += 1;
        }
    }

    /// The username poll reads through one reader for two minutes, so a node that drops the
    /// socket and stays away for a while must cost reads, not the reader and with it the
    /// attestation.
    #[tokio::test]
    async fn a_reader_keeps_reading_after_the_node_drops_its_socket_and_comes_back() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            serve(&listener, 1).await;
            // Away long enough to refuse several redials, then back on the same address.
            drop(listener);
            tokio::time::sleep(Duration::from_millis(300)).await;
            let listener = TcpListener::bind(address).await.unwrap();
            serve(&listener, usize::MAX).await;
        });

        let url = format!("ws://{address}");
        let reader = AssetHubReader::connect_with_redials(&url, Duration::from_millis(50), 100)
            .await
            .unwrap();
        assert_eq!(reader.timestamp_secs().await.unwrap(), NOW_MS / 1000);

        let after_the_drop = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(secs) = reader.timestamp_secs().await {
                    break secs;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the reader never recovered");
        assert_eq!(after_the_drop, NOW_MS / 1000);
    }

    /// Callers such as the signing host's startup lookup rely on a bad endpoint failing at once
    /// with its cause, not after the redial budget.
    #[tokio::test]
    async fn a_refused_connect_fails_at_once() {
        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", unused.local_addr().unwrap());
        drop(unused);

        let started = tokio::time::Instant::now();
        let error = AssetHubReader::connect(&url)
            .await
            .err()
            .expect("nothing listens there");
        assert!(
            started.elapsed() < RECONNECT_INTERVAL,
            "took {:?}",
            started.elapsed()
        );
        assert!(format!("{error:#}").contains(&url), "{error:#}");
    }
}
