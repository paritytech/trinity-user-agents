//! How long a worker waits before asking its policy again.

use core::time::Duration;

use super::{
	super::backoff::{ASK_AGAIN, REBUILD_COOLDOWN},
	*,
};
use crate::durable::{
	engine::verdict::write_verdict,
	model::{VerdictFailureKind, VerdictStatus},
};

/// Waits for the first attempt of row `id` to start, then fails it with a
/// failure the policy retries, which hands it back to the policy. The test
/// writes the verdict in place of the watch, so it also lets go of the
/// transaction as the watch would.
fn hand_back_first_attempt(harness: &DeferredTxHarness, id: DurableTxId) {
	harness.wait_for(id, DurableTxStatus::Pending);
	wait_until(|| harness.broadcast() == first_builds(1), "the first attempt is sent");
	harness.engine().ownership.release(id);
	let expired = VerdictStatus::Failure(VerdictFailureKind::Expired).into();
	block_on(write_verdict(harness.engine(), &harness.entry(id), expired)).unwrap();
}

fn fee_estimate_unavailable() -> PrepareAnswer {
	Err(PolicyError { reason: "fee estimate unavailable".into() })
}

/// A fee estimate cannot be read while the node is unreachable. That is no
/// verdict: the transaction is asked about again a while later, and built
/// once the policy can answer.
#[test]
fn a_policy_that_cannot_answer_is_asked_again_later() {
	let asked = AtomicU8::new(0);
	let unavailable_twice =
		ScriptedPolicy::answering_with(move |ids| match asked.fetch_add(1, Ordering::SeqCst) {
			0 | 1 => fee_estimate_unavailable(),
			_ => answer_all_with(
				ids,
				Preparation::Ready { extrinsic: extrinsic_for_index(0), params: None },
			),
		});
	let harness = DeferredTxHarness::with_policy(unavailable_twice);
	let id = harness.defer_one();
	wait_until(|| harness.policy().batches().len() == 1, "the policy is asked");

	harness.timer().advance(ASK_AGAIN);
	wait_until(|| harness.policy().batches().len() == 2, "the policy is asked again");
	harness.timer().advance(ASK_AGAIN - Duration::from_millis(1));
	harness.settle();
	assert_eq!(harness.policy().batches().len(), 2);
	harness.timer().advance(Duration::from_millis(1));

	harness.wait_for(id, DurableTxStatus::Pending);
}

/// The policy has nothing to say yet, for example because the funds it
/// spends have not arrived. Waiting is not failing.
#[test]
fn a_transaction_the_policy_never_answers_keeps_waiting() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let id = harness.defer_one();

	for round in 1..=3 {
		wait_until(|| harness.policy().batches().len() == round, "the policy is asked");
		harness.timer().advance(ASK_AGAIN);
	}

	wait_until(|| harness.policy().batches().len() == 4, "the batch is asked again");
	assert_eq!(harness.status(id), DurableTxStatus::PendingSubmission);
}

/// A transaction handed back to its policy is built again, but only after a
/// cooldown, so a policy whose builds keep failing cannot spin.
#[test]
fn a_transaction_handed_back_is_built_again_after_a_cooldown() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::building_everything());
	let id = harness.defer_one();
	hand_back_first_attempt(&harness, id);
	wait_until(
		|| harness.timer().remaining().contains(&REBUILD_COOLDOWN),
		"the rebuild cools down",
	);

	harness.timer().advance(REBUILD_COOLDOWN - Duration::from_millis(1));
	harness.settle();
	assert_eq!(harness.policy().batches().len(), 1);
	harness.timer().advance(Duration::from_millis(1));

	wait_until(|| harness.broadcast() == first_builds(2), "the rebuilt bytes are broadcast");
	assert_eq!(harness.policy().batches(), vec![vec![id], vec![id]]);
}

/// One transaction's builds keep failing, so it cools down before each
/// rebuild. A transaction deferred meanwhile in the same domain is built at
/// once: a cooldown belongs to its own transaction.
#[test]
fn a_cooling_transaction_does_not_delay_a_new_one() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::building_everything());
	let cooling = harness.defer_one();
	hand_back_first_attempt(&harness, cooling);
	wait_until(
		|| harness.timer().remaining().contains(&REBUILD_COOLDOWN),
		"the rebuild cools down",
	);

	let new = harness.defer_one();

	harness.wait_for(new, DurableTxStatus::Pending);
	assert_eq!(harness.status(cooling), DurableTxStatus::PendingSubmission);
	assert_eq!(harness.policy().batches(), vec![vec![cooling], vec![new]]);
}

/// The policy had nothing to say about a transaction, perhaps because it
/// waits for another transaction of its domain. Any change in the domain may
/// be what it waits for, so it is asked about again at once, here together
/// with a transaction registered meanwhile.
#[test]
fn a_change_in_the_domain_asks_again_at_once() {
	let harness = DeferredTxHarness::with_policy(ScriptedPolicy::answering_nothing());
	let first = harness.defer_one();
	wait_until(|| harness.policy().batches().len() == 1, "the policy is asked");

	let second = harness.defer_one();

	let asked_together = || harness.policy().batches().contains(&vec![first, second]);
	wait_until(asked_together, "both are asked about without waiting");
}

/// The policy builds the first row of a batch and has nothing to say about
/// the second yet. Building the first is a change in the domain, so the
/// second is asked about again at once.
#[test]
fn a_row_left_unanswered_is_asked_again_when_another_is_built() {
	let builds_first_row_only = ScriptedPolicy::answering_with(|ids| {
		let first_row = ids.iter().filter(|id| **id == DurableTxId(1));
		Ok(first_row
			.map(|id| (*id, Preparation::Ready { extrinsic: extrinsic_for_index(0), params: None }))
			.collect())
	});
	let harness = DeferredTxHarness::with_policy(builds_first_row_only);

	let ids = harness.defer_repeated(2);

	assert_eq!(ids, vec![DurableTxId(1), DurableTxId(2)]);
	let asked_again = || harness.policy().batches().contains(&vec![ids[1]]);
	wait_until(asked_again, "the undecided row is asked again at once");
}
