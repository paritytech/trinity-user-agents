//! H1 · Receivable keys: the X25519 keypairs that name a payment and that
//! its cheque is sealed to.
//!
//! A receivable is a fresh random draw, never derived from the seed
//! (invariant 1): no value is lost when one cannot be recovered, because a
//! cheque nobody can open stays with its sender, who takes the coins back
//! after the claim deadline.

use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// A receivable: an X25519 public key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, parity_scale_codec::Encode, parity_scale_codec::Decode)]
pub struct Receivable(pub [u8; 32]);

/// A receivable's X25519 secret key. Its `Debug` output never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct ReceivableSecret(pub Zeroizing<[u8; 32]>);

impl core::fmt::Debug for ReceivableSecret {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter.write_str("ReceivableSecret(..)")
	}
}

impl ReceivableSecret {
	/// The secret as x25519 uses it, clamped as RFC 7748 specifies.
	pub fn static_secret(&self) -> StaticSecret {
		StaticSecret::from(*self.0)
	}
}

/// The keypair whose secret is `draw`, 32 bytes from the layer's randomness,
/// clamped as RFC 7748 specifies.
pub fn new_receivable(draw: [u8; 32]) -> (Receivable, ReceivableSecret) {
	let secret = StaticSecret::from(draw);
	let receivable = Receivable(PublicKey::from(&secret).to_bytes());
	(receivable, ReceivableSecret(Zeroizing::new(secret.to_bytes())))
}

#[cfg(test)]
mod tests;
