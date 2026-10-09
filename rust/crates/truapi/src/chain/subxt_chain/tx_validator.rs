//! [`TxValidator`] for `ChainRuntime`.

use subxt::tx::ValidationResult;
use subxt::utils::H256;

use super::{best_block, failure};
use crate::chain::{EncodedExtrinsic, TxValidator};
use crate::chain_runtime::{ChainRuntime, RuntimeFailure};

#[async_trait::async_trait]
impl TxValidator for ChainRuntime {
    async fn validate(
        &self,
        genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<ValidationResult, RuntimeFailure> {
        const METHOD: &str = "validate";
        let legacy = self.legacy(genesis).await?;
        let best = best_block(&legacy, METHOD).await?;
        legacy
            .client
            .at_block_hash_and_number(best.hash, best.number)
            .await
            .map_err(|error| failure(METHOD, error))?
            .tx()
            .from_bytes(extrinsic.bytes().to_vec())
            .validate()
            .await
            .map_err(|error| failure(METHOD, error))
    }
}
