//! What item keys hold at one block, as the gap scan reads it.

use std::sync::Arc;

use subxt::utils::H256;

use super::chain::{ScarcityChain, ScarcityStorage};
use crate::{
	chain::{ChainHeads, StorageWatchError, WatchedStorage},
	gap_scan::HoldingLookup,
	nft::{HeldItem, ItemKey},
};

/// Reads `Scarcity::NftsByOwner` for item keys, every read at one block.
pub struct ScarcityHoldings {
	chain: Arc<dyn ScarcityChain>,
	at: H256,
}

impl ScarcityHoldings {
	/// Reads the pallet on the chain with `genesis`, every read at the block
	/// `at`.
	pub fn new(
		storage: Arc<dyn WatchedStorage>,
		heads: Arc<dyn ChainHeads>,
		genesis: H256,
		at: H256,
	) -> Self {
		Self { chain: Arc::new(ScarcityStorage::new(storage, heads, genesis)), at }
	}
}

#[async_trait::async_trait]
impl HoldingLookup<ItemKey> for ScarcityHoldings {
	type Holding = HeldItem;
	type Error = StorageWatchError;

	async fn read_holdings(
		&self,
		keys: &[ItemKey],
	) -> Result<Vec<Option<HeldItem>>, StorageWatchError> {
		let nfts = self.chain.nfts_at(self.at, keys).await?;
		Ok(nfts.into_iter().map(|nft| nft.map(HeldItem::from)).collect())
	}
}
