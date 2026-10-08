//! Which rows a policy is asked about together.

use futures::channel::oneshot;
use parking_lot::Mutex;

use super::*;

const SECOND_POLICY: PolicyId = PolicyId::from_static("second");

/// Waits until `policy` was asked about exactly `batch`, built as `what`.
fn wait_for_batch(policy: &ScriptedPolicy, batch: Vec<DurableTxId>, what: &str) {
    wait_until(|| policy.batches().contains(&batch), what);
}

/// Waits until the receiver of a gate fires, or not at all without one.
async fn wait_for_gate(gate: Option<oneshot::Receiver<()>>) {
    if let Some(released) = gate {
        let _ = released.await;
    }
}

/// A script whose first build waits until the returned sender fires, and
/// every later one answers at once.
fn first_build_gated() -> (oneshot::Sender<()>, PrepareScript) {
    let (release, released) = oneshot::channel::<()>();
    let gate = Mutex::new(Some(released));
    let builds = Arc::new(AtomicU8::new(0));
    let script: PrepareScript = Box::new(move |ids| {
        let ready_now = Preparation::Ready(new_bytes(&builds));
        let answer = answer_all_with(ids, ready_now);
        let first_gate = gate.lock().take();
        Box::pin(async move {
            wait_for_gate(first_gate).await;
            answer
        })
    });
    (release, script)
}

/// A policy batches what it builds, so a batch must never mix domains or
/// groups: one domain's fee payer or nonce sequence is not another's.
#[test]
fn each_domain_policy_and_group_is_built_in_its_own_batch() {
    let first = built_by(answer_every_batch(|_| Ok(HashMap::new())));
    let silent = answer_every_batch(|_| Ok(HashMap::new()));
    let second = ScriptedPolicy::new(SECOND_POLICY, RetryAnswer::Retry, silent);
    let fixture = fixture(vec![first.clone(), second.clone()]);

    let together = fixture.schedule_together(TEST, Some("g1"), REBUILD_POLICY, 2);
    let other_policy = fixture.schedule_one(TEST, Some("g1"), SECOND_POLICY);
    let other_group = fixture.schedule_one(TEST, Some("g2"), REBUILD_POLICY);
    let other_domain = fixture.schedule_one(OTHER, Some("g1"), REBUILD_POLICY);

    wait_for_batch(&first, together, "the shared batch");
    wait_for_batch(&first, vec![other_group], "the other group alone");
    wait_for_batch(&first, vec![other_domain], "the other domain alone");
    wait_for_batch(&second, vec![other_policy], "the other policy alone");
}

/// One group's build waits on something slow, such as a remote signer. The
/// other groups of the same policy must not wait behind it.
#[test]
fn a_batch_stuck_in_its_policy_does_not_hold_up_another_group() {
    let first_row = DurableTxId(1);
    let builds = Arc::new(AtomicU8::new(0));
    let script: PrepareScript = Box::new(move |ids| {
        if ids.contains(&first_row) {
            return Box::pin(pending());
        }
        let ready_now = Preparation::Ready(new_bytes(&builds));
        Box::pin(ready(answer_all_with(ids, ready_now)))
    });
    let fixture = fixture(vec![built_by(script)]);
    let blocked = fixture.schedule_one(TEST, Some("g1"), REBUILD_POLICY);
    assert_eq!(blocked, first_row);

    let free = fixture.schedule_one(TEST, Some("g2"), REBUILD_POLICY);

    fixture.wait_for(free, DurableTxStatus::Pending);
    assert_eq!(fixture.status(blocked), DurableTxStatus::PendingSubmission);
}

/// A row scheduled while its bucket is mid-build must not be lost when that
/// build ends: it is built in the next round.
#[test]
fn a_transaction_scheduled_during_a_build_is_built_in_the_next_round() {
    let (release, script) = first_build_gated();
    let policy = built_by(script);
    let fixture = fixture(vec![policy.clone()]);
    let first = fixture.schedule_one(TEST, None, REBUILD_POLICY);
    wait_until(|| policy.batches().len() == 1, "the first build starts");

    let second = fixture.schedule_one(TEST, None, REBUILD_POLICY);
    release.send(()).unwrap();

    fixture.wait_for(second, DurableTxStatus::Pending);
    assert_eq!(policy.batches(), vec![vec![first], vec![second]]);
}
