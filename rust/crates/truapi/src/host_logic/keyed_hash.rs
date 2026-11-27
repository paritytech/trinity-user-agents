//! Keyed BLAKE2b-256, which `sp_crypto_hashing` has no form of.
//!
//! [`blake2b_256_keyed`] is the one keyed hash the core derives keys and
//! handles with.

/// BLAKE2b with a 32-byte output over `message`, keyed with `key`.
///
/// Panics when `key` is longer than 64 bytes, the BLAKE2b limit. Every caller
/// passes a fixed key no longer than that.
pub fn blake2b_256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
	let hash = blake2b_simd::Params::new().hash_length(32).key(key).hash(message);
	hash.as_bytes().try_into().expect("the hash length is set to 32 bytes")
}

#[cfg(test)]
mod tests;
