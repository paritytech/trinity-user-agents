//! Limits on the secrets a product keeps, checked before anything is stored,
//! and the id each secret is kept under.

use parity_scale_codec::Encode;
use truapi::latest::HostSecretsStorageWriteError;

/// Most characters a key may have.
pub const MAX_KEY_CHARS: usize = 64;

/// Most bytes a value may take. Keychain and Keystore entries are meant for
/// credentials, not documents.
pub const MAX_VALUE_BYTES: u32 = 8192;

/// Most keys one product may keep, per account.
pub const MAX_KEYS_PER_PRODUCT: u32 = 16;

/// The id `product_id`'s secret under `key` is kept under, for the account
/// `owner` signed in with. Hashing in the account and the product means no
/// user or product can name another's secret.
pub fn secret_id(owner: &[u8; 32], product_id: &str, key: &str) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(&(b"truapi/secrets-storage", owner, product_id, key).encode())
}

/// Whether `key` is 1 to [`MAX_KEY_CHARS`] characters of `a-z`, `0-9`, `.`,
/// `_` and `-`.
pub fn is_valid_key(key: &str) -> bool {
	(1..=MAX_KEY_CHARS).contains(&key.len()) &&
		key.bytes()
			.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

/// Check a write against every limit, so a refused value never reaches the
/// host.
pub fn check_write(key: &str, value: &[u8]) -> Result<(), HostSecretsStorageWriteError> {
	if !is_valid_key(key) {
		return Err(HostSecretsStorageWriteError::InvalidKey);
	}
	if value.is_empty() {
		return Err(HostSecretsStorageWriteError::EmptyValue);
	}
	if value.len() > MAX_VALUE_BYTES as usize {
		return Err(HostSecretsStorageWriteError::TooLarge { max: MAX_VALUE_BYTES });
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	// A key is a short name a product picks for one of its secrets, so only
	// plain characters get through, and nothing longer than a name should be.
	#[test]
	fn keys_are_short_plain_names() {
		let longest = "a".repeat(MAX_KEY_CHARS);
		let too_long = "a".repeat(MAX_KEY_CHARS + 1);
		let cases = [
			("onramp", true),
			("my-svc_2.prod", true),
			(longest.as_str(), true),
			("", false),
			(too_long.as_str(), false),
			("OnRamp", false),
			("on ramp", false),
			("on/ramp", false),
			("onrämp", false),
		];

		let verdicts: Vec<_> = cases.iter().map(|(key, _)| (*key, is_valid_key(key))).collect();
		let expected: Vec<_> = cases.iter().map(|(key, valid)| (*key, *valid)).collect();
		assert_eq!(verdicts, expected);
	}

	// The limit sits exactly at the stated byte count, and an empty value is
	// refused rather than kept as an absent one.
	#[test]
	fn a_value_is_refused_when_empty_or_past_the_limit() {
		let limit = MAX_VALUE_BYTES as usize;
		let cases = [
			(vec![1; limit], Ok(())),
			(
				vec![1; limit + 1],
				Err(HostSecretsStorageWriteError::TooLarge { max: MAX_VALUE_BYTES }),
			),
			(vec![], Err(HostSecretsStorageWriteError::EmptyValue)),
		];

		let verdicts: Vec<_> =
			cases.iter().map(|(value, _)| check_write("onramp", value)).collect();
		let expected: Vec<_> = cases.into_iter().map(|(_, verdict)| verdict).collect();
		assert_eq!(verdicts, expected);
	}

	// A secret is found by this id alone, so it has to separate every account,
	// product and key, including pairs that would collide if the parts were
	// joined with a character a product id or key may contain.
	#[test]
	fn secret_ids_separate_accounts_products_and_keys() {
		let alice = [1; 32];
		let bob = [2; 32];
		let ids = [
			secret_id(&alice, "wallet.dot", "onramp"),
			secret_id(&bob, "wallet.dot", "onramp"),
			secret_id(&alice, "other.dot", "onramp"),
			secret_id(&alice, "wallet.dot", "offramp"),
			secret_id(&alice, "a.dot", "x"),
			secret_id(&alice, "a", "dot.x"),
		];

		let distinct: std::collections::HashSet<_> = ids.iter().collect();
		assert_eq!(distinct.len(), ids.len());
	}

	#[test]
	fn an_invalid_key_is_reported_before_the_value_is_looked_at() {
		assert_eq!(check_write("", &[]), Err(HostSecretsStorageWriteError::InvalidKey));
	}
}
