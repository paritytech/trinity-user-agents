use super::*;

#[test]
fn mapping_domain_errors_preserves_framework_outcomes() {
	let errors = [
		CallError::Domain(7),
		CallError::Denied,
		CallError::Unsupported,
		CallError::MalformedFrame { reason: "invalid payload".into() },
		CallError::HostFailure { reason: "host disconnected".into() },
		CallError::Cancelled,
	];
	assert_eq!(
		errors.map(|error| error.map_domain(|domain| domain.to_string())),
		[
			CallError::Domain("7".to_string()),
			CallError::Denied,
			CallError::Unsupported,
			CallError::MalformedFrame { reason: "invalid payload".into() },
			CallError::HostFailure { reason: "host disconnected".into() },
			CallError::Cancelled,
		],
	);
}

#[test]
fn call_context_timeout_can_be_set_and_replaced() {
	let default = Duration::from_secs(180);
	let explicit = Duration::from_millis(25);

	let mut cx = CallContext::with_request_id("request-1".to_string());
	assert_eq!(cx.timeout(), None);
	cx.set_timeout(default);
	assert_eq!(cx.timeout(), Some(default));

	cx.set_timeout(explicit);
	assert_eq!(cx.timeout(), Some(explicit));
}

#[test]
fn cancellation_token_clones_share_cancellation() {
	let token = CancellationToken::default();
	let cloned = token.clone();
	let wait = cloned.cancelled();

	token.cancel();

	let reason = futures::executor::block_on(wait);
	assert_eq!(reason, CancellationReason::Cancelled);
	assert!(cloned.is_cancelled());
}
