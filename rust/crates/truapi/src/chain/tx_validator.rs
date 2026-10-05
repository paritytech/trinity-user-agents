//! [`TxValidator`]: checks an extrinsic against the transaction pool rules.

use subxt::tx::ValidationResult;
use subxt::utils::H256;

use super::EncodedExtrinsic;
use crate::chain_runtime::RuntimeFailure;

/// Checks an extrinsic against the chain's transaction pool rules.
#[async_trait::async_trait]
pub trait TxValidator: Send + Sync {
    /// Validate `extrinsic` against the best block, as the transaction pool
    /// does.
    async fn validate(
        &self,
        genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<ValidationResult, RuntimeFailure>;
}
