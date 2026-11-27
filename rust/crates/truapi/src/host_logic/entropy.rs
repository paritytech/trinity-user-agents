//! Product-scoped deterministic entropy derivation.
//!
//! Matches the host product entropy contract: three keyed BLAKE2b-256 layers
//! over the session secret, product id, and caller key.
//! Host-spec C.8 defines the RFC-0007 product entropy algorithm:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L129-L147>

use thiserror::Error;

use super::keyed_hash;
use crate::platform::ProductId;

/// Error deriving product-scoped entropy.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductEntropyError {
	/// Caller key is outside the 1..=32 byte range the contract allows.
	#[error("\"key\" must be between 1 and 32 bytes, got {0}")]
	InvalidKeyLength(usize),
	/// Session has no entropy secret to derive from.
	#[error("entropy secret is missing")]
	MissingSecret,
}

/// Derive product-scoped entropy when the session already stores the
/// pre-hashed root entropy source.
pub fn derive_product_entropy_from_source(
	root_entropy_source: &[u8; 32],
	product_id: &ProductId,
	key: &[u8],
) -> Result<[u8; 32], ProductEntropyError> {
	if key.is_empty() || key.len() > 32 {
		return Err(ProductEntropyError::InvalidKeyLength(key.len()));
	}

	let product_id_hash = keyed_hash::blake2b_256_keyed(product_id.as_str().as_bytes(), &[]);
	let per_product_entropy = keyed_hash::blake2b_256_keyed(root_entropy_source, &product_id_hash);
	Ok(keyed_hash::blake2b_256_keyed(&per_product_entropy, key))
}

#[cfg(test)]
mod tests;
