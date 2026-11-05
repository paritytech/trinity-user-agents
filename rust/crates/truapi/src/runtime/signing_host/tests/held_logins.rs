//! A signed-out login request held for a test host waits for its answer.

use super::*;
use futures::{FutureExt, executor::block_on};
use truapi::latest::HostRequestLoginResponse;

#[test]
fn a_held_login_is_answered_as_the_test_decides() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	let mut first = Box::pin(host.request_login(&product));
	let mut second = Box::pin(host.request_login(&product));
	assert!((&mut first).now_or_never().is_none(), "held until answered");
	assert!((&mut second).now_or_never().is_none(), "held until answered");
	block_on(host.activate_local_session(ENTROPY.to_vec())).expect("activation succeeds");
	host.answer_held_logins(true);
	assert_eq!(
		(block_on(first).ok(), block_on(second).ok()),
		(Some(HostRequestLoginResponse::Success), Some(HostRequestLoginResponse::Success)),
	);
}

#[test]
fn a_declined_login_is_rejected_even_when_a_session_is_active() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	let mut declined = Box::pin(host.request_login(&product));
	assert!((&mut declined).now_or_never().is_none(), "held until answered");
	// A suite that signed in some other way still meant to decline.
	block_on(host.activate_local_session(ENTROPY.to_vec())).expect("activation succeeds");
	host.answer_held_logins(false);
	assert_eq!(block_on(declined).ok(), Some(HostRequestLoginResponse::Rejected));
}

#[test]
fn an_answer_given_before_the_request_answers_the_next_one() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	// The test declines before the product's request reaches the core.
	host.answer_held_logins(false);
	let answered = host.request_login(&product).now_or_never();
	assert_eq!(answered.and_then(Result::ok), Some(HostRequestLoginResponse::Rejected));

	let mut held = Box::pin(host.request_login(&product));
	assert!((&mut held).now_or_never().is_none(), "the earlier answer is spent");
}

#[test]
fn turning_holding_off_declines_held_logins_and_holds_no_more() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	let mut held = Box::pin(host.request_login(&product));
	assert!((&mut held).now_or_never().is_none(), "held until answered");
	host.set_hold_logins(false);
	assert_eq!(block_on(held).ok(), Some(HostRequestLoginResponse::Rejected));
	assert_eq!(
		host.request_login(&product).now_or_never().and_then(Result::ok),
		Some(HostRequestLoginResponse::Rejected),
	);
}

#[test]
fn holding_again_declines_held_logins_and_keeps_holding() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	// What a test host's reset does: a request from before it is declined.
	let mut held = Box::pin(host.request_login(&product));
	assert!((&mut held).now_or_never().is_none(), "held until answered");
	host.set_hold_logins(true);
	assert_eq!(block_on(held).ok(), Some(HostRequestLoginResponse::Rejected));

	let mut next = Box::pin(host.request_login(&product));
	assert!((&mut next).now_or_never().is_none(), "later requests are held");
}

#[test]
fn an_approval_given_first_answers_the_next_request_once() {
	let (_services, host) = signing_runtime();
	host.set_hold_logins(true);
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));

	// What `approveLogin` does before the product asks: sign in, then answer.
	block_on(host.activate_local_session(ENTROPY.to_vec())).expect("activation succeeds");
	host.answer_held_logins(true);
	assert_eq!(
		host.request_login(&product).now_or_never().and_then(Result::ok),
		Some(HostRequestLoginResponse::Success),
	);

	// Spent: signed out again, the next request waits for its own answer.
	block_on(host.disconnect());
	let mut held = Box::pin(host.request_login(&product));
	assert!((&mut held).now_or_never().is_none(), "the earlier answer is spent");
}
