use super::*;
use crate::host_internal::{
	sso_messages::{ProductSubtreeRequest, ProductSubtreeResponse},
	sso_wire::SsoRequest,
};

#[test]
fn finish_addresses_the_response_to_the_request() {
	let answer = SsoReply::<ProductSubtreeResponse>::from(Err("nope".to_string()))
		.finish("m-1", ProductSubtreeRequest::response_into_message);

	assert_eq!(answer.message.message_id, "m-1:response");
	let RemoteMessageData::V1(data) = answer.message.data;
	let response = ProductSubtreeRequest::response_from_message(data).unwrap();
	assert_eq!(response.responding_to, "m-1");
	assert_eq!(response.payload, Err("nope".to_string()));
	assert_eq!(answer.outcome.outcome, "error");
	assert_eq!(answer.outcome.reason.as_deref(), Some("nope"));
}

/// A peer that sends only `Cancel`s must not grow the set without bound,
/// and what it forgets is the withdrawal least likely to still matter.
#[test]
fn the_oldest_early_withdrawal_is_forgotten_once_the_set_is_full() {
	let withdrawals = SsoWithdrawals::default();
	for index in 0..=MAX_EARLY_WITHDRAWALS {
		withdrawals.withdraw(&format!("m-{index}"));
	}

	let still_withdrawn = |message_id: &str| withdrawals.begin(message_id).is_none();
	assert_eq!(
		(still_withdrawn("m-0"), still_withdrawn(&format!("m-{MAX_EARLY_WITHDRAWALS}"))),
		(false, true)
	);
}
