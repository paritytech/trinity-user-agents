//! The pallet's `NftsByOwner` entry and the value it stores.

use subxt::{
	ext::scale_decode::DecodeAsType,
	storage::{DynamicAddress, dynamic},
};

use crate::nft::HeldItem;

/// The `Scarcity::NftsByOwner` entry, naming what each account holds.
pub fn nfts_by_owner() -> DynamicAddress<([u8; 32],), ScarcityNft> {
	dynamic("Scarcity", "NftsByOwner")
}

/// What one account holds, decoded from `Scarcity::NftsByOwner` by field
/// name. Every field of the pallet's `Nft` is listed, because a value only
/// decodes whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DecodeAsType)]
pub struct ScarcityNft {
	/// Global serial of the instance.
	pub instance: u64,
	/// Collection the instance belongs to.
	pub collection: u32,
	/// Item definition within the collection.
	pub item: u32,
	/// Unix seconds at mint.
	pub minted_at: u64,
	/// Unix seconds of the last move.
	pub last_moved: u64,
	/// Revision a move must sign. Read fresh when a move is built, never
	/// cached.
	pub state_nonce: u64,
	/// Fee-less moves already used.
	pub moves: u16,
}

impl From<ScarcityNft> for HeldItem {
	fn from(nft: ScarcityNft) -> Self {
		HeldItem { collection: nft.collection, item: nft.item, instance: nft.instance }
	}
}
