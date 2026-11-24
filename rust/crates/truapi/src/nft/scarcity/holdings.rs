//! What item keys hold, as backup recovery reads it.

use std::sync::Arc;

use subxt::utils::H256;

use super::chain::{ScarcityChain, ScarcityStorage};
use crate::{
	chain::{ChainHeads, StorageWatchError, WatchedStorage},
	nft::{HeldItem, ItemKey},
};

/// Reads `Scarcity::NftsByOwner` for item keys.
pub struct ScarcityHoldings(ScarcityStorage);

impl ScarcityHoldings {
	/// Reads the pallet through `storage` and `heads` of the chain with
	/// `genesis`.
	pub fn new(
		storage: Arc<dyn WatchedStorage>,
		heads: Arc<dyn ChainHeads>,
		genesis: H256,
	) -> Self {
		Self(ScarcityStorage::new(storage, heads, genesis))
	}

	/// What each of `keys` holds at `at`, in the same order, with `None` for a
	/// key that holds nothing.
	pub async fn holdings_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> Result<Vec<Option<HeldItem>>, StorageWatchError> {
		let nfts = self.0.nfts_at(at, keys).await?;
		Ok(nfts.into_iter().map(|nft| nft.map(HeldItem::from)).collect())
	}
}
