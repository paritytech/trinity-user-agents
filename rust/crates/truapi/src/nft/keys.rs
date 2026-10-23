//! Item keys and where they sit in the derivation tree.

use super::purse::PurseId;
use crate::{
	host_logic::product_account::{
		ProductAccountError, derive_hard_path_from_keypair, derive_root_keypair_from_entropy,
	},
	installation::InstallationId,
	runtime::AuthorityError,
};

/// Position of an item key under its purse and installation. Allocated in
/// order from 0, because recovery scans from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemIndex(pub u32);

/// Public key of one item. Its secret is derived again when it is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemKey(pub [u8; 32]);

/// Where an item key sits in the derivation tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSlot {
	/// Purse the key belongs to.
	pub purse: PurseId,
	/// Installation that allocated it.
	pub installation: InstallationId,
	/// Its position under the purse and installation.
	pub index: ItemIndex,
}

impl ItemSlot {
	/// The hard junctions of `//nft//<purse>//<installation>//<index>`. Every
	/// junction is hard, so a cheque that hands over one item's secret
	/// reveals nothing about its siblings. The installation is written as
	/// `0x` and lowercase hex, as coinage writes it.
	pub fn junctions(&self) -> [String; 4] {
		let installation = format!("0x{}", hex::encode(self.installation.0));
		["nft".to_string(), self.purse.0.to_string(), installation, self.index.0.to_string()]
	}
}

/// Derives item public keys from the user's root entropy.
pub trait ItemKeyDeriver: Send + Sync {
	/// The public key at each of `slots`, in order.
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError>;
}

/// The public key at each of `slots`, derived from the root of `entropy`.
pub fn item_keys_from_entropy(
	entropy: &[u8],
	slots: &[ItemSlot],
) -> Result<Vec<ItemKey>, ProductAccountError> {
	let root = derive_root_keypair_from_entropy(entropy)?;
	let item_key = |slot: &ItemSlot| {
		let junctions = slot.junctions();
		let junctions = junctions.each_ref().map(String::as_str);
		let keypair = derive_hard_path_from_keypair(root.clone(), &junctions)?;
		Ok(ItemKey(keypair.public.to_bytes()))
	};
	slots.iter().map(item_key).collect()
}
