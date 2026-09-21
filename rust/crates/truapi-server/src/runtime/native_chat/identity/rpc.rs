// SPDX-License-Identifier: AGPL-3.0-only
// Derived from the Host-owned Coinage RPC edge and deployed Chat recipient decoder.

use super::super::NativeChatContext;
use crate::host_logic::dotns_gateway::{
    DotnsTransport, DotnsViewError, VIEW_CALL_ORIGIN, encode_revive_call, view_output,
};
use crate::host_rpc_client::HostRpcClient;
use core::time::Duration;
use futures::FutureExt;
use serde_json::{Value, json};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use subxt_rpcs::{RpcClient, client::RpcParams};
use truapi_platform::JsonRpcConnection;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

pub(super) const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_STORAGE_BYTES: usize = 64 * 1024;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(45);
const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// A finalized hash, metadata, storage and runtime calls share this connection.
/// HostRpcClient owns request correlation, cancellation and connection cleanup.
pub(super) struct Snapshot {
    rpc: RpcClient,
    at: [u8; 32],
    started: Instant,
    session_valid: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Snapshot {
    pub(super) async fn open(
        context: &NativeChatContext,
        genesis: [u8; 32],
    ) -> Result<Self, String> {
        if genesis == [0; 32] || !(context.session_valid)() {
            return Err("Chat chain is unconfigured or session expired".into());
        }
        let started = Instant::now();
        let connect = context.services.platform.connect(genesis).fuse();
        let timeout = futures_timer::Delay::new(STEP_TIMEOUT).fuse();
        futures::pin_mut!(connect, timeout);
        let connection: Arc<dyn JsonRpcConnection> = futures::select! {
            result = connect => result.map_err(|_| "configured Chat chain connection failed")?.into(),
            _ = timeout => return Err("configured Chat chain connection timed out".into()),
        };
        let mut snapshot = Self {
            rpc: RpcClient::new(HostRpcClient::new(
                connection,
                context.services.spawner.clone(),
            )),
            at: [0; 32],
            started,
            session_valid: context.session_valid.clone(),
        };
        if hash(&snapshot.call("chain_getBlockHash", json!([0])).await?)? != genesis {
            return Err("Chat chain genesis differs from configured network".into());
        }
        snapshot.at = hash(&snapshot.call("chain_getFinalizedHead", json!([])).await?)?;
        if snapshot.at == [0; 32] {
            return Err("Chat chain finalized hash is zero".into());
        }
        Ok(snapshot)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        if !(self.session_valid)() {
            return Err("Chat session expired".into());
        }
        let remaining = LOOKUP_TIMEOUT
            .checked_sub(self.started.elapsed())
            .ok_or("Chat identity lookup timed out")?
            .min(STEP_TIMEOUT);
        let mut encoded = RpcParams::new();
        for value in params.as_array().ok_or("invalid Chat RPC parameters")? {
            encoded
                .push(value)
                .map_err(|_| "invalid Chat RPC parameter")?;
        }
        let request = self.rpc.request::<Value>(method, encoded).fuse();
        let timeout = futures_timer::Delay::new(remaining).fuse();
        futures::pin_mut!(request, timeout);
        let result = futures::select! {
            result = request => result.map_err(|_| format!("Chat identity RPC failed ({method})")),
            _ = timeout => Err(format!("Chat identity RPC timed out ({method})")),
        };
        if !(self.session_valid)() {
            return Err("Chat session expired".into());
        }
        result
    }

    pub(super) async fn metadata(&self) -> Result<Vec<u8>, String> {
        bytes(
            &self
                .call("state_getMetadata", json!([hex0x(&self.at)]))
                .await?,
            MAX_METADATA_BYTES,
        )
    }

    pub(super) async fn runtime_call(
        &self,
        function: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        bytes(
            &self
                .call(
                    "state_call",
                    json!([function, hex0x(input), hex0x(&self.at)]),
                )
                .await?,
            MAX_STORAGE_BYTES,
        )
    }

    pub(super) async fn storage_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let response = self
            .call(
                "state_queryStorageAt",
                json!([[hex0x(key)], hex0x(&self.at)]),
            )
            .await?;
        storage_response(&response, key, self.at)
    }
}

#[truapi_platform::async_trait]
impl DotnsTransport for Snapshot {
    async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
        self.storage_value(&key).await
    }

    async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
        let output = self
            .runtime_call(
                "ReviveApi_call",
                &encode_revive_call(&VIEW_CALL_ORIGIN, dest, &input),
            )
            .await
            .map_err(DotnsViewError::Failed)?;
        let output = view_output(&output)?;
        validate_view_bounds(&input, &output).map_err(DotnsViewError::Failed)?;
        Ok(output)
    }
}

fn validate_view_bounds(input: &[u8], output: &[u8]) -> Result<(), String> {
    use crate::host_logic::dotns_gateway::selector;
    if input.get(..4) == Some(selector("getLabels(uint256,uint256)").as_slice())
        || input.get(..4) == Some(selector("pendingClaims(address,uint256,uint256)").as_slice())
    {
        // Both shared gateway readers request pages of sixteen. Check dynamic
        // array lengths before their ABI decoders allocate or follow offsets;
        // repeated offsets must not amplify a 64-KiB reply without a bound.
        if output.get(..31) != Some([0; 31].as_slice()) || output.get(31) != Some(&32) {
            return Err("noncanonical dotNS page array offset".into());
        }
        let count = output.get(32..64).ok_or("truncated dotNS page length")?;
        if count[..31] != [0; 31] || count[31] > 16 {
            return Err("dotNS page exceeded requested label count".into());
        }
        if output.len() < 64 + usize::from(count[31]) * 32 {
            return Err("truncated dotNS page offset table".into());
        }
    }
    Ok(())
}

fn storage_response(response: &Value, key: &[u8], at: [u8; 32]) -> Result<Option<Vec<u8>>, String> {
    let blocks = response.as_array().ok_or("invalid Chat storage response")?;
    if blocks.len() != 1 || hash(&blocks[0]["block"])? != at {
        return Err("Chat storage response substituted finalized block".into());
    }
    let rows = blocks[0]["changes"]
        .as_array()
        .ok_or("invalid Chat storage changes")?;
    if rows.len() != 1 {
        return Err("Chat storage response omitted or duplicated requested key".into());
    }
    let row = rows[0].as_array().ok_or("invalid Chat storage row")?;
    if row.len() != 2 || bytes(&row[0], MAX_STORAGE_BYTES)? != key {
        return Err("Chat storage response substituted requested key".into());
    }
    if row[1].is_null() {
        Ok(None)
    } else {
        bytes(&row[1], MAX_STORAGE_BYTES).map(Some)
    }
}

fn bytes(value: &Value, max: usize) -> Result<Vec<u8>, String> {
    let encoded = value
        .as_str()
        .and_then(|value| value.strip_prefix("0x"))
        .ok_or("Chat chain bytes are not prefixed hex")?;
    if encoded.len() > max * 2 || encoded.len() % 2 != 0 {
        return Err("Chat chain bytes exceed decode bound or have odd length".into());
    }
    hex::decode(encoded).map_err(|_| "Chat chain bytes contain invalid hex".into())
}

fn hash(value: &Value) -> Result<[u8; 32], String> {
    bytes(value, 32)?
        .try_into()
        .map_err(|_| "Chat chain hash is not 32 bytes".into())
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_block_key_duplicate_and_missing_storage_substitutions() {
        let at = [7; 32];
        let good = json!([{"block": hex0x(&at), "changes": [["0x0102", "0x42"]]}]);
        assert_eq!(
            storage_response(&good, &[1, 2], at).unwrap(),
            Some(vec![0x42])
        );
        assert!(storage_response(&good, &[1, 3], at).is_err());
        assert!(storage_response(&good, &[1, 2], [8; 32]).is_err());
        for changes in [
            json!([]),
            json!([["0x0102", null], ["0x0102", "0x42"]]),
            json!([["0x0102", false]]),
        ] {
            assert!(
                storage_response(
                    &json!([{"block": hex0x(&at), "changes": changes}]),
                    &[1, 2],
                    at
                )
                .is_err()
            );
        }
        assert_eq!(
            storage_response(
                &json!([{"block": hex0x(&at), "changes": [["0x0102", null]]}]),
                &[1, 2],
                at
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn dynamic_directory_pages_cannot_amplify_unbounded_claimed_counts() {
        let input =
            crate::host_logic::dotns_gateway::call_u256_pair("getLabels(uint256,uint256)", 0, 16);
        let mut page = vec![0; 64];
        page[31] = 32;
        assert!(validate_view_bounds(&input, &page).is_ok());
        page[63] = 17;
        assert!(validate_view_bounds(&input, &page).is_err());
        page[63] = 1;
        assert!(validate_view_bounds(&input, &page).is_err());
        page.resize(96, 0);
        assert!(validate_view_bounds(&input, &page).is_ok());
        page[32] = 1;
        assert!(validate_view_bounds(&input, &page).is_err());
    }
}
