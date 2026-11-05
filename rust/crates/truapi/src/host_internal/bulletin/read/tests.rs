use super::*;

/// An image reference names its blob by CID; the core reads the blob by key.
#[test]
fn a_preimage_cid_parses_back_to_its_key() {
	let cid = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";

	assert_eq!(
		preimage_key_from_cid(cid).map(hex::encode),
		Some("0bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c".to_string())
	);
	assert_eq!(preimage_key_from_cid(&cid.to_uppercase()), None);
	assert_eq!(preimage_key_from_cid(&cid[..cid.len() - 1]), None);
	assert_eq!(
		preimage_key_from_cid("bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
		None,
		"sha2-256 CIDs are not Bulletin preimages"
	);
}

/// The bitswap spec makes only the code stable, and `Fail` covers data that is not held yet,
/// so every `Fail` is a miss to poll on, whatever its message or data.
#[test]
fn call_errors_are_classified_by_code() {
	let kind = |code| {
		let error = UserError { code, message: "from the node".to_string(), data: None };
		match call_error(&error) {
			Ok(None) => "miss",
			Ok(Some(_)) => "value",
			Err(BitswapError::Unanswerable(_)) => "stop",
			Err(BitswapError::MethodMissing(_)) => "stop and reconnect",
			Err(BitswapError::Connection(_)) => "reconnect",
			Err(BitswapError::Failed(_)) => "retry",
		}
	};
	assert_eq!(
		[FAIL, INVALID_PARAMS, METHOD_NOT_FOUND, -32811, -32812].map(kind),
		["miss", "stop", "stop and reconnect", "retry", "retry"]
	);
}
