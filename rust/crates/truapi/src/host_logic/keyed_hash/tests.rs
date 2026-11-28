use super::*;

/// Hosts on every platform derive the same keys and handles from this hash,
/// so it must match BLAKE2b as other implementations compute it. The expected
/// values come from Python's `hashlib.blake2b(digest_size=32)`. An empty key
/// is the unkeyed hash, which product entropy relies on to hash the product id.
#[test]
fn matches_an_independent_blake2b_256() {
	let key: Vec<u8> = (0..32).collect();
	assert_eq!(
		[blake2b_256_keyed(b"abc", &key), blake2b_256_keyed(b"abc", &[])].map(hex::encode),
		[
			"d63a32d3e44738d7907f964316c241adaba0abfeabc32349677578a15a203f7f",
			"bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319",
		]
	);
}
