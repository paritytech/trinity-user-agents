//! Item keys derived a range at a time, as backup recovery scans them.

use core::ops::Range;

use super::{WalletAccountHolder, WalletKeys};
use crate::{
	host_logic::product_account::{ProductAccountError, derive_root_keypair_from_entropy},
	installation::InstallationId,
	nft::{ItemKey, PurseId, item_key_range_from_root},
	runtime::AuthorityError,
};

impl WalletAccountHolder {
	/// The item keys of `purse` allocated by `installation` at each of
	/// `indices`, in order.
	pub fn item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, AuthorityError> {
		self.with_installed_keys(|keys| Ok(keys.item_key_range(purse, installation, indices)?))
	}
}

impl WalletKeys {
	/// The item keys of `purse` allocated by `installation` at each of
	/// `indices`, in order, derived from the seed's root key.
	pub fn item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, ProductAccountError> {
		let root = derive_root_keypair_from_entropy(&self.entropy)?;
		item_key_range_from_root(root, purse, installation, indices)
	}
}
