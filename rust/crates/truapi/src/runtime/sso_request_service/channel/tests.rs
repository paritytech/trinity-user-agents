use super::*;
use crate::{
	runtime::authority::AuthorityError,
	test_support::{chain_halted_rpc_error, rpc_refusal},
};

/// Only a lasting halt is retryable, since the request was never sent.
#[test]
fn only_a_halted_subscribe_is_reported_as_chain_unavailable() {
	let reason = |err| format!("SSO own statement-store subscribe failed: {err}");
	let reported = [chain_halted_rpc_error(), rpc_refusal(None)]
		.iter()
		.map(|err| {
			AuthorityError::from(subscribe_error(&CallContext::default(), "request", "own", err))
		})
		.collect::<Vec<_>>();

	assert_eq!(
		reported,
		vec![
			AuthorityError::ChainUnavailable { reason: reason(chain_halted_rpc_error()) },
			AuthorityError::Unknown { reason: reason(rpc_refusal(None)) },
		]
	);
}
