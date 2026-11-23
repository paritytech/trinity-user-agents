//! Item keys derived a range at a time, as backup recovery scans them.

use core::ops::Range;

use super::WalletAccountHolder;
use crate::{
	installation::InstallationId,
	nft::{ItemKey, PurseId},
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
		self.with_installed_keys(|keys| {
			Ok(keys.entropy.item_key_range(purse, installation, indices)?)
		})
	}
}
