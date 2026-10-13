//! Limits on the auth tokens a product keeps with the host, checked before
//! the host's secure storage sees a request.

use truapi::latest::{AuthToken, HostAuthTokensStoreError};

/// Most characters a service id may have.
pub const MAX_SERVICE_CHARS: usize = 64;

/// Most bytes `token` and `refresh_token` may take together. Keychain and
/// Keystore entries are meant for credentials, not documents.
pub const MAX_TOKEN_BYTES: u32 = 8192;

/// Whether `service` is 1 to [`MAX_SERVICE_CHARS`] characters of `a-z`,
/// `0-9`, `.`, `_` and `-`. Hosts may build storage keys from it, so it never
/// carries a separator or anything a key format would need to escape.
pub fn is_valid_service(service: &str) -> bool {
	(1..=MAX_SERVICE_CHARS).contains(&service.len()) &&
		service
			.bytes()
			.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

/// Check a store request against every limit, so a refused token never
/// reaches the host.
pub fn check_store(service: &str, token: &AuthToken) -> Result<(), HostAuthTokensStoreError> {
	if !is_valid_service(service) {
		return Err(HostAuthTokensStoreError::InvalidService);
	}
	if token.token.is_empty() {
		return Err(HostAuthTokensStoreError::Unknown {
			reason: "token must not be empty".to_string(),
		});
	}
	let size = token.token.len() + token.refresh_token.as_ref().map_or(0, String::len);
	if size > MAX_TOKEN_BYTES as usize {
		return Err(HostAuthTokensStoreError::TooLarge { max: MAX_TOKEN_BYTES });
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn token(token: &str, refresh_token: Option<&str>) -> AuthToken {
		AuthToken {
			token: token.to_string(),
			refresh_token: refresh_token.map(str::to_string),
			expires_at: None,
		}
	}

	// The service id ends up in a host's storage key, so only the plain
	// characters every key format accepts get through, and nothing longer
	// than a key segment should be.
	#[test]
	fn service_ids_are_short_plain_names() {
		let longest = "a".repeat(MAX_SERVICE_CHARS);
		let too_long = "a".repeat(MAX_SERVICE_CHARS + 1);
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

		let verdicts: Vec<_> =
			cases.iter().map(|(service, _)| (*service, is_valid_service(service))).collect();
		let expected: Vec<_> = cases.iter().map(|(service, valid)| (*service, *valid)).collect();
		assert_eq!(verdicts, expected);
	}

	// The limit covers both secrets together, since both land in the same
	// secure storage entry, and sits exactly at the stated byte count.
	#[test]
	fn a_token_is_refused_only_past_the_combined_limit() {
		let limit = MAX_TOKEN_BYTES as usize;
		let half = "t".repeat(limit / 2);
		let too_large = Err(HostAuthTokensStoreError::TooLarge { max: MAX_TOKEN_BYTES });
		let cases = [
			(token(&"t".repeat(limit), None), Ok(())),
			(token(&half, Some(&half)), Ok(())),
			(token(&half, Some(&format!("{half}r"))), too_large.clone()),
			(token(&"t".repeat(limit + 1), None), too_large),
			(
				token("", Some("refresh")),
				Err(HostAuthTokensStoreError::Unknown {
					reason: "token must not be empty".to_string(),
				}),
			),
		];

		let verdicts: Vec<_> =
			cases.iter().map(|(token, _)| check_store("onramp", token)).collect();
		let expected: Vec<_> = cases.into_iter().map(|(_, verdict)| verdict).collect();
		assert_eq!(verdicts, expected);
	}

	#[test]
	fn an_invalid_service_is_reported_before_the_token_is_looked_at() {
		assert_eq!(
			check_store("", &token("", None)),
			Err(HostAuthTokensStoreError::InvalidService)
		);
	}
}
