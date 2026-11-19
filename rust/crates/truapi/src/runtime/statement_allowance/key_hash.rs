//! Substrate storage-key hashers shared by the allowance storage readers.

use sp_crypto_hashing::{blake2_128, twox_64};

/// `Blake2_128Concat(x)` = `blake2_128(x) ‖ x`.
pub fn blake2_128_concat(x: &[u8]) -> Vec<u8> {
	[blake2_128(x).as_slice(), x].concat()
}

/// `Twox64Concat(x)` = `twox_64(x) ‖ x`.
pub fn twox_64_concat(x: &[u8]) -> Vec<u8> {
	[twox_64(x).as_slice(), x].concat()
}
