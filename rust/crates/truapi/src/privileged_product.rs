//! The products every host trusts as part of itself.

use crate::{ProductId, ProductIdError};

/// A product every host trusts as part of itself, on whichever network it
/// runs.
///
/// Each is a dotNS label reserved for it on every network, so `peopl.dot` and
/// `peopl.paseo` are the same privileged product. A privileged product gets
/// every remote permission without a prompt; device permissions still need the
/// OS grant.
///
/// Parses from its label, the whole dotNS name without a TLD.
#[derive(Debug, Clone, Copy, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
#[cfg_attr(test, derive(strum::VariantArray))]
pub enum PrivilegedProduct {
	/// Humanity: its ring-VRF domain holds the person keys and its account 0
	/// is the RFC-0028 identity account.
	#[strum(serialize = "peopl")]
	Personhood,
	/// Jollity, the game.
	#[strum(serialize = "dim2")]
	Game,
	/// Collectibles, the NFT product.
	#[strum(serialize = "stash")]
	Collectibles,
}

impl PrivilegedProduct {
	/// The dotNS label reserved for the product, without a TLD.
	pub fn label(self) -> &'static str {
		self.into()
	}

	/// The product's id on the network with `network_suffix`.
	///
	/// Fails when `network_suffix` is not a dotNS TLD.
	pub fn product_id(self, network_suffix: &str) -> Result<ProductId, ProductIdError> {
		ProductId::from_name(self.label(), network_suffix)
	}
}

#[cfg(test)]
mod tests;
