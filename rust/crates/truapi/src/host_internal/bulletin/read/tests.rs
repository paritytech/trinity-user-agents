use super::*;

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
