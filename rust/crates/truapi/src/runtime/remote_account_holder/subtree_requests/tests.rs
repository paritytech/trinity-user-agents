use core::time::Duration;

use futures::{FutureExt, executor::block_on, future};
use truapi::{CallContext, CancellationReason, CancellationToken};

use super::{
	super::super::{authority::AuthorityError, authority_cancellation_error},
	InFlightProductSubtree,
};

fn cancelled_context(reason: CancellationReason) -> CallContext {
	let cx = CallContext::default();
	cx.cancel().cancel_with_reason(reason);
	cx
}

#[test]
fn a_subtree_request_is_joined_only_while_pending_with_callers() {
	// A caller must not take a request that has already failed, nor one being
	// cancelled because every caller left it.
	let (pending, _pending_caller) =
		InFlightProductSubtree::start(future::pending().boxed(), CancellationToken::default());
	let (settled, settled_caller) = InFlightProductSubtree::start(
		future::ready(Ok([7; 32])).boxed(),
		CancellationToken::default(),
	);
	let _still_holding = settled.join().expect("a pending request is joinable");
	block_on(settled_caller.answer(&CallContext::default())).expect("the ready request settles");
	let (abandoned, abandoned_caller) =
		InFlightProductSubtree::start(future::pending().boxed(), CancellationToken::default());
	drop(abandoned_caller);

	assert_eq!(
		[pending.join().is_some(), settled.join().is_some(), abandoned.join().is_some()],
		[true, false, false],
	);
}

#[test]
fn only_the_last_caller_to_withdraw_cancels_the_shared_request() {
	// The request must keep running for the callers still waiting, and must
	// learn the last caller's reason so an explicit cancel can withdraw it
	// from the Account Holder.
	let cancel = CancellationToken::default();
	let unwinds_on_cancel = {
		let cancel = cancel.clone();
		async move {
			cancel.cancelled().await;
			Err(AuthorityError::Disconnected)
		}
		.boxed()
	};
	let (in_flight, first) = InFlightProductSubtree::start(unwinds_on_cancel, cancel.clone());
	let second = in_flight.join().expect("a pending request is joinable");
	let timed_out = CancellationReason::TimedOut { timeout: Duration::from_secs(1) };
	let first_cx = cancelled_context(CancellationReason::Cancelled);
	let second_cx = cancelled_context(timed_out.clone());

	let first_left = block_on(first.answer(&first_cx));
	let reason_after_first = cancel.reason();
	let second_left = block_on(second.answer(&second_cx));

	assert_eq!(
		(first_left, reason_after_first, second_left, cancel.reason(), in_flight.join().is_some()),
		(
			Err(authority_cancellation_error(&first_cx, CancellationReason::Cancelled)),
			None,
			Err(authority_cancellation_error(&second_cx, timed_out.clone())),
			Some(timed_out),
			false,
		),
	);
}
