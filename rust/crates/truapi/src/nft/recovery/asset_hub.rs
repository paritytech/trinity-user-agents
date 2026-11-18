//! Recovery's chain reads on Asset Hub.

use std::{collections::HashSet, sync::Arc};

use subxt::utils::H256;

use super::{ItemHoldings, RecoveryChain, RecoveryError};
use crate::{
	chain::ChainHeads,
	chain_runtime::ChainAccess,
	installation::InstallationId,
	installation_registry::RegisteredInstallations,
	nft::scarcity::{ScarcityChain, ScarcityHoldings, ScarcityStorage},
};

/// Reads the installation registry and `Scarcity::NftsByOwner` on Asset Hub.
pub struct AssetHubRecoveryChain {
	heads: Arc<dyn ChainHeads>,
	genesis: H256,
	scarcity: Arc<dyn ScarcityChain>,
	registry: RegisteredInstallations,
}

impl AssetHubRecoveryChain {
	/// Reads the heads and the pallet of the Asset Hub `chain` reaches, and the
	/// seed's installations through `registry`.
	pub fn new(chain: &ChainAccess, registry: RegisteredInstallations) -> Self {
		let runtime = Arc::new(chain.runtime().clone());
		let heads: Arc<dyn ChainHeads> = runtime.clone();
		let scarcity = Arc::new(ScarcityStorage::new(runtime, heads.clone(), chain.genesis()));
		Self { heads, genesis: chain.genesis(), scarcity, registry }
	}
}

#[async_trait::async_trait]
impl RecoveryChain for AssetHubRecoveryChain {
	async fn finalized_block(&self) -> Result<H256, RecoveryError> {
		Ok(self.heads.finalized_block(self.genesis).await?.hash)
	}

	async fn registered_installations(
		&self,
		at: H256,
	) -> Result<HashSet<InstallationId>, RecoveryError> {
		Ok(self.registry.read_installations(at).await?)
	}

	fn holdings_at(&self, at: H256) -> Arc<ItemHoldings> {
		Arc::new(ScarcityHoldings::new(self.scarcity.clone(), at))
	}
}
