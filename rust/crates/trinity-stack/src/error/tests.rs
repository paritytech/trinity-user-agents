use super::*;

/// Codes are a published contract: readers in other languages match on them.
#[test]
fn codes_keep_their_published_spelling() {
	let codes = [
		(Error::NotJson(String::new()), "not-json"),
		(Error::InvalidBase64, "invalid-base64"),
		(Error::NonCanonicalR, "non-canonical-r"),
		(Error::ParaId(String::new()), "para-id"),
		(Error::InvalidHex { field: "f", bytes: 1 }, "invalid-hex"),
		(Error::EnvelopeVersionMismatch { envelope: 1, payload: 2 }, "envelope-version-mismatch"),
		(Error::ForkSignedWithSourceKey, "fork-signed-with-source-key"),
		(Error::SecretKeyLength(3), "secret-key-length"),
	];
	for (error, code) in codes {
		assert_eq!(error.code(), code);
	}
}
