//! Recovery's chain reads on Asset Hub.

use std::{collections::HashSet, sync::Arc};

use subxt::utils::H256;

use super::{NftRecoveryError, RecoveryChain};
use crate::{
	chain::ChainHeads,
	chain_runtime::ChainAccess,
	installation::InstallationId,
	installation_registry::RegisteredInstallations,
	nft::{HeldItem, ItemKey, scarcity::ScarcityHoldings},
};

/// Reads the installation registry and `Scarcity::NftsByOwner` on Asset Hub.
pub struct AssetHubRecoveryChain {
	heads: Arc<dyn ChainHeads>,
	genesis: H256,
	holdings: ScarcityHoldings,
	registry: RegisteredInstallations,
}

impl AssetHubRecoveryChain {
	/// Reads the heads and the pallet of the Asset Hub `chain` reaches, and the
	/// seed's installations through `registry`.
	pub fn new(chain: &ChainAccess, registry: RegisteredInstallations) -> Self {
		let runtime = Arc::new(chain.runtime.clone());
		let heads: Arc<dyn ChainHeads> = runtime.clone();
		let holdings = ScarcityHoldings::new(runtime, heads.clone(), chain.genesis);
		Self { heads, genesis: chain.genesis, holdings, registry }
	}
}

#[async_trait::async_trait]
impl RecoveryChain for AssetHubRecoveryChain {
	async fn finalized_block(&self) -> Result<H256, NftRecoveryError> {
		Ok(self.heads.finalized_block(self.genesis).await?.hash)
	}

	async fn registered_installations(
		&self,
		at: H256,
	) -> Result<HashSet<InstallationId>, NftRecoveryError> {
		Ok(self.registry.read_installations(at).await?)
	}

	async fn holdings_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> anyhow::Result<Vec<Option<HeldItem>>> {
		Ok(self.holdings.holdings_at(at, keys).await?)
	}
}
