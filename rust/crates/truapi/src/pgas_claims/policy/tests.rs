use futures::executor::block_on;
use subxt::utils::{AccountId32, H256};

use super::*;
use crate::{
	durable::{DomainId, DurableTxStatus, SubmissionPlan, Window, extrinsic},
	pgas_claims::{
		ClaimIdentity,
		testing::{BuildCall, FakeClaimBuilder, TEST_DAY, TEST_IDENTITY, destination},
	},
};

/// A claim registered just now, waiting for a session for ten minutes.
fn waiting_claim(id: i64, dest: AccountId32) -> DeferredDurableTx {
	let give_up_after = current_unix_secs() + 600;
	deferred(
		id,
		0,
		ClaimParams { target: dest, identity: TEST_IDENTITY, give_up_after, attempt: None },
	)
}

fn deferred(id: i64, attempts: u32, params: ClaimParams) -> DeferredDurableTx {
	DeferredDurableTx {
		id: DurableTxId(id),
		domain: DomainId::new("pgas"),
		group: None,
		plan: SubmissionPlan { params: params.into() },
		attempts,
		dependencies: Vec::new(),
	}
}

fn slot(slot_index: u32) -> ClaimSlot {
	ClaimSlot { day: TEST_DAY, slot_index }
}

fn prepare_with(
	policy: &PgasClaimPolicy,
	batch: &[DeferredDurableTx],
) -> HashMap<DurableTxId, Preparation> {
	block_on(policy.prepare(batch)).unwrap()
}

/// The answer for one claim of a policy over a builder failing with `failure`.
fn answer_when_build_fails(
	claim: DeferredDurableTx,
	failure: fn() -> ClaimBuildError,
) -> Option<Preparation> {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::failing_with(failure));
	prepare_with(&policy, &[claim]).remove(&DurableTxId(1))
}

/// The slot each answer of `answers` was built in, from the params it
/// records.
fn built_slot(answers: &HashMap<DurableTxId, Preparation>, id: i64) -> ClaimSlot {
	let Some(Preparation::Ready { params: Some(params), .. }) = answers.get(&DurableTxId(id))
	else {
		panic!("claim {id} was not built: {answers:?}")
	};
	ClaimParams::try_from(params).unwrap().attempt.unwrap().slot
}

/// Two claims of this process never take one slot, so neither attempt is
/// refused for the other.
#[test]
fn claims_of_one_batch_are_built_in_different_slots() {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::building());
	let batch = [waiting_claim(1, destination(7)), waiting_claim(2, destination(8))];

	let answers = prepare_with(&policy, &batch);

	assert_eq!(built_slot(&answers, 1), slot(0));
	assert_eq!(built_slot(&answers, 2), slot(1));
}

/// A slot whose attempt may still land is not handed to the next batch.
#[test]
fn a_slot_built_a_moment_ago_is_not_handed_out_again() {
	let builder = FakeClaimBuilder::building();
	let policy = PgasClaimPolicy::new(builder.clone());
	prepare_with(&policy, &[waiting_claim(1, destination(7))]);

	prepare_with(&policy, &[waiting_claim(2, destination(8))]);

	let second = BuildCall {
		target: destination(8),
		identity: TEST_IDENTITY,
		excluded: BTreeSet::from([slot(0)]),
	};
	assert_eq!(builder.calls().last(), Some(&second));
}

/// The oracle proves a claim by the slot and alias of its live attempt,
/// so a built claim records both.
#[test]
fn a_built_claim_records_its_slot_and_alias() {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::building());
	let claim = waiting_claim(1, destination(7));

	let answer = prepare_with(&policy, std::slice::from_ref(&claim)).remove(&DurableTxId(1));

	let mut expected = ClaimParams::try_from(&claim.plan.params).unwrap();
	expected.attempt = Some(ClaimAttempt { slot: slot(0), alias: [0; 32] });
	let ready =
		Preparation::Ready { extrinsic: extrinsic(0, 130, 64), params: Some(expected.into()) };
	assert_eq!(answer, Some(ready));
}

/// Nobody's personhood pays for a claim another account registered.
#[test]
fn a_claim_under_another_identity_gives_up_at_once() {
	let answer = answer_when_build_fails(waiting_claim(1, destination(7)), || {
		ClaimBuildError::OtherIdentity
	});

	assert_eq!(answer, Some(Preparation::GiveUp));
}

#[test]
fn a_claim_without_personhood_gives_up() {
	let answer =
		answer_when_build_fails(waiting_claim(1, destination(7)), || ClaimBuildError::NoPersonhood);

	assert_eq!(answer, Some(Preparation::GiveUp));
}

#[test]
fn a_claim_with_no_free_slot_gives_up() {
	let answer = answer_when_build_fails(waiting_claim(1, destination(7)), || {
		ClaimBuildError::NoFreeSlot { day: TEST_DAY, max: 4 }
	});

	assert_eq!(answer, Some(Preparation::GiveUp));
}

/// A user signing in again within the deadline still gets the claim.
#[test]
fn a_claim_without_a_session_waits_until_its_deadline() {
	let answer =
		answer_when_build_fails(waiting_claim(1, destination(7)), || ClaimBuildError::NoSession);

	assert_eq!(answer, None);
}

/// No wait and no rebuild outlives the deadline: a lost session, a
/// failure that keeps repeating or a refused attempt all end there, and the
/// next request for the destination registers a fresh claim.
#[test]
fn a_claim_past_its_deadline_gives_up_without_building() {
	let builder = FakeClaimBuilder::building();
	let policy = PgasClaimPolicy::new(builder.clone());
	let past_deadline = ClaimParams {
		target: destination(7),
		identity: TEST_IDENTITY,
		give_up_after: current_unix_secs() - 1,
		attempt: None,
	};

	let answer = prepare_with(&policy, &[deferred(1, 1, past_deadline)]).remove(&DurableTxId(1));

	assert_eq!(answer, Some(Preparation::GiveUp));
	assert_eq!(builder.calls(), Vec::new());
}

/// A slow ring revision or a dropped connection is not a reason to fail the
/// claim before its deadline: it is asked about again.
#[test]
fn a_transient_build_failure_keeps_the_claim_waiting() {
	let answer = answer_when_build_fails(waiting_claim(1, destination(7)), || {
		ClaimBuildError::Transient(anyhow::anyhow!("ring revision 9 has not arrived"))
	});

	assert_eq!(answer, None);
}

/// A refusal that repeats cannot loop forever.
#[test]
fn a_claim_built_five_times_gives_up_without_building() {
	let builder = FakeClaimBuilder::building();
	let policy = PgasClaimPolicy::new(builder.clone());
	let params = ClaimParams::try_from(&waiting_claim(1, destination(7)).plan.params).unwrap();

	let answer = prepare_with(&policy, &[deferred(1, 5, params)]).remove(&DurableTxId(1));

	assert_eq!(answer, Some(Preparation::GiveUp));
	assert_eq!(builder.calls(), Vec::new());
}

#[test]
fn a_claim_built_four_times_is_built_again() {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::building());
	let params = ClaimParams::try_from(&waiting_claim(1, destination(7)).plan.params).unwrap();

	let answers = prepare_with(&policy, &[deferred(1, 4, params)]);

	assert_eq!(built_slot(&answers, 1), slot(0));
}

fn entry() -> DurableTxEntry {
	DurableTxEntry {
		id: DurableTxId(1),
		domain: DomainId::new("pgas"),
		group: None,
		tx_hash: H256::zero(),
		window: Window { birth: 130, period: 64 },
		status: DurableTxStatus::Pending,
		scanned_to: None,
		plan: None,
	}
}

/// A slot taken meanwhile or a day that ended costs one rebuild.
#[test]
fn a_refused_or_expired_attempt_is_built_again() {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::building());
	let params = PolicyParams::default();

	assert!(policy.can_retry(&entry(), &params, VerdictFailureKind::Rejected));
	assert!(policy.can_retry(&entry(), &params, VerdictFailureKind::Expired));
}

/// The pallet refused the claim itself, so another slot would fare no better.
#[test]
fn a_failed_dispatch_is_final() {
	let policy = PgasClaimPolicy::new(FakeClaimBuilder::building());

	let retried =
		policy.can_retry(&entry(), &PolicyParams::default(), VerdictFailureKind::DispatchFailed);

	assert!(!retried);
}

/// Once an attempt landed or can no longer land, its slot is no longer
/// held back from other claims.
#[test]
fn a_slot_built_longer_ago_than_the_memory_is_forgotten() {
	let mut recent = RecentSlots::default();
	recent.remember(slot(0), 1_000);

	recent.remember(slot(1), 1_000 + RECENT_SLOT_SECS + 1);

	assert_eq!(recent.held_slots(0), BTreeSet::from([slot(1)]));
}

#[test]
fn a_slot_built_within_the_memory_is_still_held_back() {
	let mut recent = RecentSlots::default();
	recent.remember(slot(0), 1_000);
	recent.remember(slot(1), 1_000 + RECENT_SLOT_SECS);

	let held = recent.held_slots(1_000);

	assert_eq!(held, BTreeSet::from([slot(0), slot(1)]));
}

/// The identity a claim was registered with is the one it is built as.
#[test]
fn a_claim_is_built_as_its_registered_identity() {
	let builder = FakeClaimBuilder::building();
	let policy = PgasClaimPolicy::new(builder.clone());
	let other = ClaimIdentity::from_root_public_key([0x2e; 32]);
	let mut params = ClaimParams::try_from(&waiting_claim(1, destination(7)).plan.params).unwrap();
	params.identity = other;

	prepare_with(&policy, &[deferred(1, 0, params)]);

	let identities: Vec<_> = builder.calls().into_iter().map(|call| call.identity).collect();
	assert_eq!(identities, vec![other]);
}
