//! Recovery's chain reads on Asset Hub.

use std::sync::Arc;

use subxt::utils::H256;

use super::{ItemHoldings, RecoveryChain, RecoveryError};
use crate::{
	chain::{ChainHeads, RpcClientAtBlock},
	chain_runtime::ChainRuntime,
	installation::InstallationId,
	installation_registry::RegisteredInstallations,
	nft::scarcity::ScarcityHoldings,
};

/// Reads the installation registry and `Scarcity::NftsByOwner` on Asset Hub.
pub struct AssetHubRecoveryChain {
	chain: ChainRuntime,
	genesis: H256,
	registry: RegisteredInstallations,
}

impl AssetHubRecoveryChain {
	/// Reads of the Asset Hub with `genesis` over `chain`, the registry at the
	/// contract `registry` names.
	pub fn new(chain: ChainRuntime, genesis: H256, registry: RegisteredInstallations) -> Self {
		Self { chain, genesis, registry }
	}
}

#[async_trait::async_trait]
impl RecoveryChain for AssetHubRecoveryChain {
	async fn finalized_block(&self) -> Result<H256, RecoveryError> {
		Ok(ChainHeads::heads(&self.chain, self.genesis).await?.finalized.hash)
	}

	async fn registered_installations(
		&self,
		at: H256,
	) -> Result<Vec<InstallationId>, RecoveryError> {
		let connection = self.chain.legacy_connection(self.genesis.as_bytes()).await?;
		let mut node = RpcClientAtBlock::new(connection.raw_rpc_client, at);
		Ok(self.registry.read_installations(&mut node).await?)
	}

	fn holdings_at(&self, at: H256) -> Arc<ItemHoldings> {
		let chain = Arc::new(self.chain.clone());
		Arc::new(ScarcityHoldings::new(chain.clone(), chain, self.genesis, at))
	}
}
