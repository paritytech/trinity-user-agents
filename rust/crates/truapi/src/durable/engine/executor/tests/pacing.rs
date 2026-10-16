//! How long a bucket waits before asking its policy again.

use core::time::Duration;

use super::{
	super::backoff::{INITIAL_BACKOFF, MAX_BACKOFF, REBUILD_COOLDOWN},
	*,
};
use crate::durable::{
	engine::verdict::write_verdict,
	model::{VerdictFailureKind, VerdictStatus},
};

/// Waits for the first attempt of row `id` to start, then fails it with a
/// failure the policy retries, which hands it back to the policy.
fn hand_back_first_attempt(harness: &ScheduledTxHarness, id: DurableTxId) {
	harness.wait_for(id, DurableTxStatus::Pending);
	wait_until(|| harness.timer().sleeps() == 1, "the first attempt's watch waits");
	let expired = VerdictStatus::Failure(VerdictFailureKind::Expired).into();
	block_on(write_verdict(harness.engine(), &harness.entry(id), expired)).unwrap();
}

fn fee_estimate_unavailable() -> PrepareAnswer {
	Err(PolicyError { reason: "fee estimate unavailable".into() })
}

/// A fee estimate cannot be read while the node is unreachable. That is no
/// verdict: the batch is asked again, later each time, and built once the
/// policy can answer.
#[test]
fn a_policy_that_cannot_answer_is_asked_again_after_a_growing_backoff() {
	let asked = AtomicU8::new(0);
	let unavailable_twice =
		ScriptedPolicy::answering_with(move |ids| match asked.fetch_add(1, Ordering::SeqCst) {
			0 | 1 => fee_estimate_unavailable(),
			_ => answer_all_with(ids, Preparation::Ready(nth_build(0))),
		});
	let harness = ScheduledTxHarness::with_policy(unavailable_twice);
	let id = harness.schedule_one();
	wait_until(|| harness.timer().sleeps() == 1, "the bucket backs off");

	harness.timer().advance(INITIAL_BACKOFF);
	wait_until(|| harness.timer().sleeps() == 2, "the bucket backs off again");
	harness.timer().advance(INITIAL_BACKOFF);
	harness.settle();
	assert_eq!(harness.policy().batches().len(), 2);
	harness.timer().advance(INITIAL_BACKOFF);

	harness.wait_for(id, DurableTxStatus::Pending);
}

/// The policy has nothing to say yet, for example because the funds it
/// spends have not arrived. Waiting is not failing.
#[test]
fn a_transaction_the_policy_never_answers_keeps_waiting() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let id = harness.schedule_one();

	for round in 1..=3 {
		wait_until(|| harness.timer().sleeps() == round, "the bucket backs off");
		harness.timer().advance(MAX_BACKOFF);
	}

	wait_until(|| harness.policy().batches().len() == 4, "the batch is asked again");
	assert_eq!(harness.status(id), DurableTxStatus::PendingSubmission);
}

/// A transaction handed back to its policy is built again, but only after a
/// cooldown, so a policy whose builds keep failing cannot spin.
#[test]
fn a_transaction_handed_back_is_built_again_after_a_cooldown() {
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::building_everything());
	let id = harness.schedule_one();
	hand_back_first_attempt(&harness, id);
	wait_until(|| harness.timer().sleeps() == 2, "the rebuild cools down");

	harness.timer().advance(REBUILD_COOLDOWN - Duration::from_millis(1));
	harness.settle();
	assert_eq!(harness.policy().batches().len(), 1);
	harness.timer().advance(Duration::from_millis(1));

	wait_until(|| harness.broadcast() == first_builds(2), "the rebuilt bytes are broadcast");
	assert_eq!(harness.policy().batches(), vec![vec![id], vec![id]]);
}

/// The policy builds the first row of a batch and has nothing to say about
/// the second yet. The round still decided something, so the next one runs
/// at once instead of backing off.
#[test]
fn a_round_that_decides_one_row_runs_the_next_without_backing_off() {
	let builds_first_row_only = ScriptedPolicy::answering_with(|ids| {
		let first_row = ids.iter().filter(|id| **id == DurableTxId(1));
		Ok(first_row.map(|id| (*id, Preparation::Ready(nth_build(0)))).collect())
	});
	let harness = ScheduledTxHarness::with_policy(builds_first_row_only);

	let ids = harness.schedule_repeated(2);

	assert_eq!(ids, vec![DurableTxId(1), DurableTxId(2)]);
	let asked_again = || harness.policy().batches().contains(&vec![ids[1]]);
	wait_until(asked_again, "the undecided row is asked again at once");
}
