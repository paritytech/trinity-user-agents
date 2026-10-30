//! [`AccountNonces`]: the nonce an account's next transaction takes.

use subxt::utils::{AccountId32, H256};

use crate::chain_runtime::RuntimeFailure;

/// The nonce an account's next transaction takes on a chain.
#[async_trait::async_trait]
pub trait AccountNonces: Send + Sync {
	/// The account's next nonce, counting its transactions still in the
	/// node's pool, so two transactions built in a row do not share one.
	async fn next_nonce(&self, genesis: H256, account: &AccountId32)
	-> Result<u64, RuntimeFailure>;
}
