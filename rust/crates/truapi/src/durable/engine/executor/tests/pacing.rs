//! How long a bucket waits before asking its policy again.

use super::{
	super::backoff::{INITIAL_BACKOFF, MAX_BACKOFF, REBUILD_COOLDOWN},
	*,
};
use crate::durable::{engine::verdict::write_verdict, model::FailureKind};

/// Waits for the first attempt of row `id` to start, then hands it back to
/// its policy, as a retried failure does.
fn hand_back_first_attempt(fixture: &Fixture, id: DurableTxId) {
	fixture.wait_for(id, DurableTxStatus::Pending);
	let watch_waits = || fixture.timer.sleeps() == 1;
	wait_until(watch_waits, "the first attempt's watch waits");
	let read = fixture.engine.db.read(move |conn| dao::entry(conn, id));
	let attempt = block_on(read).unwrap().unwrap();
	let back = DurableTxStatus::PendingSubmission.into();
	block_on(write_verdict(&fixture.engine, &attempt, back)).unwrap();
}

/// A fee estimate cannot be read while the node is unreachable. That is no
/// verdict: the batch is asked again, later each time, and built once the
/// policy can answer.
#[test]
fn a_policy_that_cannot_answer_is_asked_again_after_a_growing_backoff() {
	let asked = AtomicU8::new(0);
	let policy =
		built_by(answer_every_batch(move |ids| match asked.fetch_add(1, Ordering::SeqCst) {
			0 | 1 => unavailable(),
			_ => answer_all_with(ids, Preparation::Ready(extrinsic(100, 100, 64))),
		}));
	let fixture = fixture(vec![policy.clone()]);
	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);
	wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");

	fixture.timer.advance(INITIAL_BACKOFF);
	wait_until(|| fixture.timer.sleeps() == 2, "the bucket backs off again");
	fixture.timer.advance(INITIAL_BACKOFF);
	fixture.settle();
	assert_eq!(policy.batches().len(), 2);
	fixture.timer.advance(INITIAL_BACKOFF);

	fixture.wait_for(id, DurableTxStatus::Pending);
}

/// The policy has nothing to say yet, for example because the funds it
/// spends have not arrived. Waiting is not failing.
#[test]
fn a_transaction_the_policy_never_answers_keeps_waiting() {
	let policy = built_by(answer_every_batch(|_| Ok(HashMap::new())));
	let fixture = fixture(vec![policy.clone()]);
	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);

	for round in 1..=3 {
		wait_until(|| fixture.timer.sleeps() == round, "the bucket backs off");
		fixture.timer.advance(MAX_BACKOFF);
	}

	wait_until(|| policy.batches().len() == 4, "the batch is asked again");
	assert_eq!(fixture.status(id), DurableTxStatus::PendingSubmission);
}

/// The policy hands back the exact bytes of an attempt that was already
/// released. Those bytes were decided once; broadcasting them again could
/// only confuse that decision, so the row keeps waiting for new ones.
#[test]
fn a_build_repeating_a_released_attempt_is_not_broadcast() {
	let repeated = extrinsic(100, 100, 64);
	let policy = built_by(answer_every_batch(move |ids| {
		answer_all_with(ids, Preparation::Ready(repeated.clone()))
	}));
	let fixture = fixture(vec![policy]);
	let next_id = DurableTxId(1);
	let repeated_hash = extrinsic(100, 100, 64).extrinsic.hash();
	fixture.engine.ownership.release(next_id, repeated_hash);

	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);

	wait_until(|| fixture.timer.sleeps() == 1, "the bucket backs off");
	assert_eq!(id, next_id);
	assert_eq!(fixture.status(id), DurableTxStatus::PendingSubmission);
	assert_eq!(fixture.submitted(), vec![]);
}

/// A transaction handed back to its policy is built again, but only after a
/// cooldown, so a policy whose builds keep failing cannot spin.
#[test]
fn a_transaction_handed_back_is_built_again_after_a_cooldown() {
	let policy = built_by(builds_everything());
	let fixture = fixture(vec![policy.clone()]);
	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);
	hand_back_first_attempt(&fixture, id);
	wait_until(|| fixture.timer.sleeps() == 2, "the rebuild cools down");

	let almost = REBUILD_COOLDOWN - Duration::from_millis(1);
	fixture.timer.advance(almost);
	fixture.settle();
	assert_eq!(policy.batches().len(), 1);
	fixture.timer.advance(Duration::from_millis(1));

	let both = broadcast_bytes(&[100, 101]);
	let rebuilt_sent = || fixture.submitted() == both;
	wait_until(rebuilt_sent, "the rebuilt bytes are broadcast");
	assert_eq!(policy.batches(), vec![vec![id], vec![id]]);
}

/// How many attempts the executor remembers starting for row `id`.
fn remembered_attempts(fixture: &Fixture, id: DurableTxId) -> Option<u32> {
	let started = fixture.engine.executor.attempts_started.lock();
	started.get(&id).copied()
}

/// Builds every row once, and gives up on every row asked about again.
fn builds_once_then_gives_up() -> PrepareScript {
	let asked = AtomicU8::new(0);
	answer_every_batch(move |ids| match asked.fetch_add(1, Ordering::SeqCst) {
		0 => answer_all_with(ids, Preparation::Ready(extrinsic(100, 100, 64))),
		_ => answer_all_with(ids, Preparation::GiveUp),
	})
}

/// A long-running process builds many transactions. The count behind the
/// rebuild cooldown must go once a row is decided for good, or it is kept
/// for every row the process ever built.
#[test]
fn a_finalized_row_leaves_no_attempt_count_behind() {
	let fixture = fixture(vec![built_by(builds_everything())]);
	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);
	fixture.wait_for(id, DurableTxStatus::Pending);
	let read = fixture.engine.db.read(move |conn| dao::entry(conn, id));
	let attempt = block_on(read).unwrap().unwrap();
	let finalized = DurableTxStatus::FinalizedSuccess { at: None }.into();

	block_on(write_verdict(&fixture.engine, &attempt, finalized)).unwrap();

	assert_eq!(remembered_attempts(&fixture, id), None);
}

#[test]
fn a_row_given_up_after_a_rebuild_leaves_no_attempt_count_behind() {
	let fixture = fixture(vec![built_by(builds_once_then_gives_up())]);
	let id = fixture.schedule_one(test_domain(), None, REBUILD_POLICY);
	hand_back_first_attempt(&fixture, id);
	wait_until(|| fixture.timer.sleeps() == 2, "the rebuild cools down");

	fixture.timer.advance(REBUILD_COOLDOWN);

	fixture.wait_for(id, DurableTxStatus::Failure(FailureKind::Abandoned));
	assert_eq!(remembered_attempts(&fixture, id), None);
}

/// New bytes for row 1, and for any other row the bytes a test released.
fn first_row_new_bytes(id: &DurableTxId) -> (DurableTxId, Preparation) {
	let tag = if *id == DurableTxId(1) { 100 } else { 200 };
	(*id, Preparation::Ready(extrinsic(tag, 100, 64)))
}

/// One row of a batch starts an attempt while the policy repeats released
/// bytes for the other. The round still decided something, so the next one
/// runs at once instead of backing off.
#[test]
fn a_round_that_decides_one_row_runs_the_next_without_backing_off() {
	let script = answer_every_batch(|ids| {
		let answers = ids.iter().map(first_row_new_bytes);
		Ok(answers.collect())
	});
	let policy = built_by(script);
	let fixture = fixture(vec![policy.clone()]);
	let released = extrinsic(200, 100, 64).extrinsic.hash();
	fixture.engine.ownership.release(DurableTxId(2), released);

	let ids = fixture.schedule_together(test_domain(), None, REBUILD_POLICY, 2);

	assert_eq!(ids, vec![DurableTxId(1), DurableTxId(2)]);
	let asked_again = || policy.batches().contains(&vec![ids[1]]);
	wait_until(asked_again, "the undecided row is asked again at once");
}
