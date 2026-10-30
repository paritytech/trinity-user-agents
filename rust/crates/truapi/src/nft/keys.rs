//! Item keys and where they sit in the derivation tree.

use std::sync::Weak;

use parity_scale_codec::{Decode, Encode};
use schnorrkel::Keypair;

use super::purse::PurseId;
use crate::{
	host_internal::extrinsic::Sr25519Signer,
	host_logic::product_account::{
		ProductAccountError, derive_hard_path_from_keypair, derive_root_keypair_from_entropy,
	},
	installation::InstallationId,
	runtime::AuthorityError,
};

/// Position of an item key under its purse and installation. Allocated in
/// order from 0, because recovery scans from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ItemIndex(pub u32);

/// Public key of one item. Its secret is derived again when it is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub struct ItemKey(pub [u8; 32]);

/// Where an item key sits in the derivation tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
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

/// For a deriver that owns the ledger, which must not keep it alive in turn.
/// Once the deriver is gone there are no keys to derive.
impl<Deriver: ItemKeyDeriver> ItemKeyDeriver for Weak<Deriver> {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		let Some(deriver) = self.upgrade() else {
			let reason = "the item key deriver was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		deriver.derive_item_keys(slots)
	}
}

/// Signs with item keys. The secrets never leave the implementor.
pub trait ItemSigner: Send + Sync {
	/// A signer for the key at `slot`. Fails without a session.
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError>;
}

/// For a signer that owns the durable engine, which must not keep it alive
/// in turn. Once the signer is gone there is nothing to sign with.
impl<Signer: ItemSigner> ItemSigner for Weak<Signer> {
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError> {
		let Some(signer) = self.upgrade() else {
			let reason = "the item signer was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		signer.item_signer(slot)
	}
}

/// The public key at each of `slots`, derived from the root of `entropy`.
pub fn item_keys_from_entropy(
	entropy: &[u8],
	slots: &[ItemSlot],
) -> Result<Vec<ItemKey>, ProductAccountError> {
	let root = derive_root_keypair_from_entropy(entropy)?;
	let item_key = |slot: &ItemSlot| {
		let keypair = item_keypair_under(root.clone(), slot)?;
		Ok(ItemKey(keypair.public.to_bytes()))
	};
	slots.iter().map(item_key).collect()
}

/// A signer for the key at `slot`, derived from the root of `entropy`.
pub fn item_signer_from_entropy(
	entropy: &[u8],
	slot: ItemSlot,
) -> Result<Sr25519Signer, ProductAccountError> {
	let root = derive_root_keypair_from_entropy(entropy)?;
	let keypair = item_keypair_under(root, &slot)?;
	Ok(Sr25519Signer::from_keypair(&keypair))
}

/// The keypair at `slot` under `root`.
fn item_keypair_under(root: Keypair, slot: &ItemSlot) -> Result<Keypair, ProductAccountError> {
	let junctions = slot.junctions();
	let junctions = junctions.each_ref().map(String::as_str);
	derive_hard_path_from_keypair(root, &junctions)
}

#[cfg(test)]
mod tests {
	use subxt::tx::Signer;

	use super::*;

	#[test]
	fn the_signer_at_a_slot_signs_for_the_key_derived_at_it() {
		let entropy = [0x42; 16];
		let slot = ItemSlot {
			purse: PurseId(3),
			installation: InstallationId([0x1d; 32]),
			index: ItemIndex(7),
		};

		let signer = item_signer_from_entropy(&entropy, slot).unwrap();
		let keys = item_keys_from_entropy(&entropy, &[slot]).unwrap();

		assert_eq!(signer.account_id().0, keys[0].0);
	}
}
