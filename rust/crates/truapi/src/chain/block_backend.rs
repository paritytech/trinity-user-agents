//! [`BlockBackend`]: block data at any block the node still keeps.

use subxt::utils::H256;

use super::HashAndNumber;
use crate::chain_runtime::RuntimeFailure;

/// Whether an included extrinsic dispatched successfully.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// `System.ExtrinsicSuccess` was emitted for it.
    Succeeded,
    /// `System.ExtrinsicFailed` was emitted for it.
    Failed,
}

/// Block data at any block the node still keeps.
#[async_trait::async_trait]
pub trait BlockBackend: Send + Sync {
    /// Hash of the block at `number`, canonical up to the finalized height.
    /// Above it, a reorg can change the hash returned for the same height.
    async fn block_hash(&self, genesis: H256, number: u64) -> Result<Option<H256>, RuntimeFailure>;

    /// Number of the block with `hash`.
    async fn block_number(&self, genesis: H256, hash: H256) -> Result<Option<u64>, RuntimeFailure>;

    /// Hashes of the extrinsics in the block with hash `at`, in block order.
    async fn extrinsic_hashes(
        &self,
        genesis: H256,
        at: H256,
    ) -> Result<Option<Vec<H256>>, RuntimeFailure>;

    /// How the extrinsic with hash `extrinsic_hash` dispatched in block `at`,
    /// or `None` when the block does not contain it.
    async fn dispatch_outcome(
        &self,
        genesis: H256,
        at: HashAndNumber,
        extrinsic_hash: H256,
    ) -> Result<Option<DispatchOutcome>, RuntimeFailure>;
}
