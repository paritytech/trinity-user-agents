use super::*;

/// An SSO answer is only a string, so a chain the signing host could not
/// reach must survive the trip to reach the product as `ChainUnavailable`,
/// while every other failure reads back as it was sent.
#[test]
fn a_signing_host_failure_keeps_its_kind_through_the_sso_answer() {
	let failures = [
		AuthorityError::ChainUnavailable { reason: "cannot select a metadata block".to_string() },
		AuthorityError::Rejected,
		AuthorityError::Unknown { reason: "the payload did not decode".to_string() },
	];

	let relayed: Vec<AuthorityError> = failures
		.iter()
		.cloned()
		.map(|failure| SsoRemoteResponseError::Failure(failure.into_sso_reason()).into())
		.collect();

	assert_eq!(relayed, failures);
}
