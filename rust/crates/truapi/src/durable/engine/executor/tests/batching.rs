//! Which rows a policy is asked about together.

use futures::{
	channel::oneshot,
	future::{pending, ready},
};
use parking_lot::Mutex;

use super::*;
use crate::durable::{
	engine::DurableRequest,
	model::GroupId,
	testing::{PrepareScript, ScriptedPolicy},
};

/// Waits until `policy` was asked about exactly `batch`, built as `what`.
fn wait_for_batch(policy: &ScriptedPolicy, batch: Vec<DurableTxId>, what: &str) {
	wait_until(|| policy.batches().contains(&batch), what);
}

/// `count` transactions of `domain` in `group`, built by `policy`.
fn scheduled_in(domain: DomainId, group: &str, policy: &PolicyId, count: usize) -> DurableRequest {
	let plan = SubmissionPlan { policy: policy.clone(), params: PolicyParams::default() };
	DurableRequest::scheduled(domain, Some(GroupId::new(group)), vec![plan; count]).unwrap()
}

/// A script whose first build waits until the returned sender fires, and
/// every build answers with new bytes.
fn first_build_gated() -> (oneshot::Sender<()>, PrepareScript) {
	let (release, released) = oneshot::channel::<()>();
	let gate = Mutex::new(Some(released));
	let builds = AtomicU8::new(0);
	let script: PrepareScript = Box::new(move |ids| {
		let built = extrinsic_for_index(builds.fetch_add(1, Ordering::SeqCst));
		let answer = answer_all_with(ids, Preparation::Ready(built));
		let first_gate = gate.lock().take();
		Box::pin(async move {
			if let Some(released) = first_gate {
				let _ = released.await;
			}
			answer
		})
	});
	(release, script)
}

/// A policy batches what it builds, so a batch must never mix domains or
/// groups: one domain's fee payer or nonce sequence is not another's.
#[test]
fn each_domain_policy_and_group_is_built_in_its_own_batch() {
	let second_policy_id = PolicyId::new("second");
	let harness = ScheduledTxHarness::with_policies(vec![
		ScriptedPolicy::answering_nothing(),
		ScriptedPolicy::answering_nothing().named(second_policy_id.clone()),
	]);
	let test_policy = harness.policy();
	let second_policy = harness.policy_named(&second_policy_id);

	let together = harness.schedule(scheduled_in(test_domain(), "g1", &test_policy_id(), 2));
	let other_policy = harness.schedule(scheduled_in(test_domain(), "g1", &second_policy_id, 1));
	let other_group = harness.schedule(scheduled_in(test_domain(), "g2", &test_policy_id(), 1));
	let other_domain = harness.schedule(scheduled_in(other_domain(), "g1", &test_policy_id(), 1));

	wait_for_batch(test_policy, together, "the shared batch");
	wait_for_batch(test_policy, other_group, "the other group alone");
	wait_for_batch(test_policy, other_domain, "the other domain alone");
	wait_for_batch(second_policy, other_policy, "the other policy alone");
}

/// One group's build waits on something slow, such as a remote signer. The
/// other groups of the same policy must not wait behind it.
#[test]
fn a_batch_stuck_in_its_policy_does_not_hold_up_another_group() {
	let first_row = DurableTxId(1);
	let builds = AtomicU8::new(0);
	let stuck_on_first_row: PrepareScript = Box::new(move |ids| {
		if ids.contains(&first_row) {
			return Box::pin(pending());
		}
		let built = extrinsic_for_index(builds.fetch_add(1, Ordering::SeqCst));
		Box::pin(ready(answer_all_with(ids, Preparation::Ready(built))))
	});
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::from_script(stuck_on_first_row));
	let blocked = harness.schedule(scheduled_in(test_domain(), "g1", &test_policy_id(), 1))[0];
	assert_eq!(blocked, first_row);

	let free = harness.schedule(scheduled_in(test_domain(), "g2", &test_policy_id(), 1))[0];

	harness.wait_for(free, DurableTxStatus::Pending);
	assert_eq!(harness.status(blocked), DurableTxStatus::PendingSubmission);
}

/// A row scheduled while its bucket is mid-build must not be lost when that
/// build ends: it is built in the next round.
#[test]
fn a_transaction_scheduled_during_a_build_is_built_in_the_next_round() {
	let (release, script) = first_build_gated();
	let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::from_script(script));
	let first = harness.schedule_one();
	wait_until(|| harness.policy().batches().len() == 1, "the first build starts");

	let second = harness.schedule_one();
	release.send(()).unwrap();

	harness.wait_for(second, DurableTxStatus::Pending);
	assert_eq!(harness.policy().batches(), vec![vec![first], vec![second]]);
}
