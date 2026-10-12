//! The chain capabilities implemented directly on `ChainRuntime`, over its
//! per-chain subxt clients: the legacy client for reads and validation, the
//! shared chainHead client for submission. One file per capability.

use core::error::Error;

use subxt::backend::Backend;
use subxt::config::Header;
use subxt::utils::H256;
use subxt_rpcs::Error as RpcError;

use super::HashAndNumber;
use crate::chain_runtime::{ChainRuntime, LegacyConnection, RuntimeFailure};

mod block_backend;
mod heads;
mod tx_submitter;
mod tx_validator;

impl ChainRuntime {
    async fn legacy(&self, genesis: H256) -> Result<LegacyConnection, RuntimeFailure> {
        self.legacy_connection(genesis.as_bytes()).await
    }
}

/// Number of the block with `hash`, which the node must know.
async fn numbered(
    legacy: &LegacyConnection,
    hash: H256,
    method: &'static str,
) -> Result<HashAndNumber, RuntimeFailure> {
    let header = legacy
        .backend
        .block_header(hash)
        .await
        .map_err(|error| failure(method, error))?
        .ok_or_else(|| RuntimeFailure::host_failure(method, format!("unknown block {hash:?}")))?;
    Ok(HashAndNumber {
        hash,
        number: header.number(),
    })
}

/// The best block, as the node sees it now.
async fn best_block(
    legacy: &LegacyConnection,
    method: &'static str,
) -> Result<HashAndNumber, RuntimeFailure> {
    let best = legacy
        .methods
        .chain_get_block_hash(None)
        .await
        .map_err(|error| failure(method, error))?
        .ok_or_else(|| RuntimeFailure::host_failure(method, "node reported no best block"))?;
    numbered(legacy, best, method).await
}

/// The latest finalized block.
async fn finalized_block(
    legacy: &LegacyConnection,
    method: &'static str,
) -> Result<HashAndNumber, RuntimeFailure> {
    let finalized = legacy
        .backend
        .latest_finalized_block_ref()
        .await
        .map_err(|error| failure(method, error))?
        .hash();
    numbered(legacy, finalized, method).await
}

/// A transport that failed or closed means the chain is unavailable for now;
/// anything else is a failure of the node or of decoding its answer.
fn failure(method: &'static str, error: impl Error + 'static) -> RuntimeFailure {
    let mut source: Option<&(dyn Error + 'static)> = Some(&error);
    while let Some(cause) = source {
        if let Some(RpcError::Client(_) | RpcError::DisconnectedWillReconnect(_)) =
            cause.downcast_ref()
        {
            return RuntimeFailure::unavailable_with_reason(method, error.to_string());
        }
        source = cause.source();
    }
    RuntimeFailure::host_failure(method, error.to_string())
}
