//! [`AccountNonces`] for `ChainRuntime`.

use subxt::utils::{AccountId32, H256};

use super::failure;
use crate::{
	chain::AccountNonces,
	chain_runtime::{ChainRuntime, RuntimeFailure},
};

const NEXT_NONCE: &str = "chain_next_nonce";

#[async_trait::async_trait]
impl AccountNonces for ChainRuntime {
	async fn next_nonce(
		&self,
		genesis: H256,
		account: &AccountId32,
	) -> Result<u64, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		legacy
			.methods
			.system_account_next_index(account)
			.await
			.map_err(|error| failure(NEXT_NONCE, error))
	}
}
