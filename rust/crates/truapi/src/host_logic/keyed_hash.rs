//! Keyed BLAKE2b-256, which `sp_crypto_hashing` has no form of.
//!
//! [`blake2b_256_keyed`] is the one keyed hash the core derives keys and
//! handles with.

/// BLAKE2b with a 32-byte output over `message`, keyed with `key`.
///
/// Panics when `key` is longer than 64 bytes, the BLAKE2b limit. Callers stay
/// within it: the longest key is a 64-byte signing secret, and a key a product
/// supplies is checked to be 1 to 32 bytes before it is hashed.
pub fn blake2b_256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
	let hash = blake2b_simd::Params::new().hash_length(32).key(key).hash(message);
	hash.as_bytes().try_into().expect("the hash length is set to 32 bytes")
}

#[cfg(test)]
mod tests;
