//! The user's root BIP-39 entropy.
//!
//! [`RootEntropy`] owns the seed a host hands over and wipes it when dropped.
//! Only this module reads the bytes, through [`RootEntropy::derive_root_keypair`],
//! [`RootEntropy::keyed_hash`] and [`RootEntropy::hash`]. Every other derivation
//! is a method its own module adds on top of those three.

use schnorrkel::{ExpansionMode, Keypair};
use zeroize::Zeroizing;

use super::{keyed_hash::blake2b_256_keyed, product_account::ProductAccountError};

/// The user's root BIP-39 entropy, wiped from memory when dropped.
pub struct RootEntropy(Zeroizing<Vec<u8>>);

impl RootEntropy {
	/// Takes ownership of the raw entropy a host hands over.
	pub fn new(entropy: Vec<u8>) -> Self {
		Self(Zeroizing::new(entropy))
	}

	/// Derive the root sr25519 keypair.
	///
	/// Host-spec C.1 defines the BIP-39 entropy to sr25519 mini-secret path:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L24-L41>
	///
	/// Matches the Substrate mini-secret scheme (`sp_core::sr25519::Pair::from_entropy`)
	/// used by polkadot-app-ios-v2: PBKDF2 over the entropy to a 32-byte mini
	/// secret, then Ed25519-mode expansion. The public key of this keypair is the
	/// `rootAccountId` shared with paired hosts.
	pub fn derive_root_keypair(&self) -> Result<Keypair, ProductAccountError> {
		let mini_secret = substrate_bip39::mini_secret_from_entropy(&self.0, "")
			.map_err(|err| ProductAccountError::InvalidEntropy(format!("{err:?}")))?;
		Ok(mini_secret.expand_to_keypair(ExpansionMode::Ed25519))
	}

	/// BLAKE2b-256 of the entropy, keyed with `key`.
	///
	/// Panics when `key` is longer than 64 bytes, the BLAKE2b limit.
	pub fn keyed_hash(&self, key: &[u8]) -> [u8; 32] {
		blake2b_256_keyed(&self.0, key)
	}

	/// BLAKE2b-256 of the entropy, with no key.
	pub fn hash(&self) -> [u8; 32] {
		sp_crypto_hashing::blake2_256(&self.0)
	}
}
