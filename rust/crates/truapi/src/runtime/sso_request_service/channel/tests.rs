use super::*;
use crate::{
	chain_runtime::RuntimeFailure,
	platform::ProviderError,
	runtime::authority::AuthorityError,
	test_support::{chain_halted_rpc_error, rpc_refusal},
};

/// A People chain that cannot be dialled, or stays halted while its
/// connection is checked, may answer a later request, so the product is
/// told it can retry. A People chain that restarted will not, and neither
/// will a host that refuses to serve the chain.
#[test]
fn only_an_unreachable_people_chain_is_reported_as_chain_unavailable() {
	let connect = |error| StatementStoreRpcClientError::Connect { label: "test", error };
	let offline = || ProviderError::Transport { reason: "network down".to_string() };
	let unserved = || ProviderError::Host { reason: "no such chain".to_string() };
	let halted = RuntimeFailure::unavailable_with_reason("remote_chain_connect", "halted");
	let restarted = RuntimeFailure::host_failure("remote_chain_connect", "another chain");

	let reported =
		[connect(offline()), halted.clone().into(), connect(unserved()), restarted.clone().into()]
			.map(|error| AuthorityError::from(connect_error(&error)));

	assert_eq!(
		reported,
		[
			AuthorityError::ChainUnavailable { reason: connect(offline()).to_string() },
			AuthorityError::ChainUnavailable { reason: halted.to_string() },
			AuthorityError::Unknown { reason: connect(unserved()).to_string() },
			AuthorityError::Unknown { reason: restarted.to_string() },
		]
	);
}

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
