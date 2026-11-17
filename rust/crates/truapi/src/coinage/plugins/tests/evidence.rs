//! What the `coinage` oracle makes of each read: present, absent or
//! unknown, for coins and for entries, at a [`World`]'s finalized head.
//!
//! Each record is shaped so that the asset under test is the only evidence
//! the oracle has.

use crate::{
	coinage::{
		Asset,
		chain::RingPosition,
		operation::Retry,
		planning::Call,
		registration::{Admission, Rows},
		start::RecordParams,
		testing::world::{Finality, Proof, Standing, World},
	},
	durable::{DurableTxFailureKind, DurableTxId},
};

const NOTHING: Proof = Proof { completed: false, not_completed: false };
const COMPLETED: Proof = Proof { completed: true, not_completed: false };
const NOT_COMPLETED: Proof = Proof { completed: false, not_completed: true };

/// A released record consuming `inputs` into `outputs` under `call`.
fn record(world: &mut World, call: Call, inputs: &[Asset], outputs: &[Asset]) -> DurableTxId {
	let params = RecordParams { call, retry: Retry::Never };
	let rows = Rows { inputs: inputs.to_vec(), outputs: outputs.to_vec(), tokens: Vec::new() };
	let id = world.register_with(Admission::Ordinary, vec![rows], params, true).unwrap()[0];
	world.release(&[id]);
	id
}

/// An unload of `entries` into the external asset: the entries are its
/// only evidence.
fn unload(world: &mut World, entries: &[Asset]) -> DurableTxId {
	record(world, Call::UnloadToExternal([7; 32]), entries, &[])
}

/// Entry 5, at `position` in its collection.
fn entry_at(world: &mut World, position: RingPosition) -> Asset {
	let entry = world.entry(5);
	world.position(entry, position, Finality::Finalized);
	entry
}

/// Entry 5, in ring 7.
fn entry_in_ring(world: &mut World) -> Asset {
	let entry = world.entry(5);
	world.place(entry, 7, Finality::Finalized);
	entry
}

#[test]
fn a_coin_the_chain_holds_is_available() {
	let mut world = World::new();
	let coin = world.coin(1);
	world.mint(&[coin], Finality::Finalized);
	let id = record(&mut world, Call::UnloadToExternal([7; 32]), &[coin], &[]);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

/// An empty account is evidence: an untouched output absent there proves the
/// record has not run, where an unknown read would prove nothing.
#[test]
fn a_coin_the_chain_holds_nothing_for_reads_absent_not_unknown() {
	let mut world = World::new();
	let out = world.coin(2);
	let id = record(&mut world, Call::Load([0; 32]), &[], &[out]);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

#[test]
fn a_failed_coin_read_is_unknown_for_every_coin_of_the_record() {
	let mut world = World::new();
	let (spent, out) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, out);
	let account = world.account(spent);
	world.chain().tree().faults.coins.insert(account);
	assert_eq!(world.proof(id), NOTHING);
}

/// The pallet: absence from the alias map means the alias is available.
#[test]
fn an_entry_in_a_ring_with_no_alias_state_is_present_and_not_unloaded() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

#[test]
fn an_entry_whose_alias_is_unloaded_proves_its_unload() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	world.unload_alias(entry, Finality::Finalized);
	assert_eq!(world.proof(id), COMPLETED);
}

/// A failed alias read is unknown, not "not unloaded" (C2 invariant 2): the
/// unload may have run.
#[test]
fn a_failed_alias_read_leaves_a_present_entry_unknown() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	let (_, alias) = world.entry_keys(entry);
	world.chain().tree().faults.aliases.insert(alias);
	assert_eq!(world.proof(id), NOTHING);
}

/// An onboarding entry was never in a ring, and an unload proves ring
/// membership, so no unload can have consumed it.
#[test]
fn an_onboarding_entry_is_present_and_provably_not_unloaded() {
	let mut world = World::new();
	let entry = entry_at(&mut world, RingPosition::Onboarding);
	let id = unload(&mut world, &[entry]);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

#[test]
fn an_alias_unloaded_in_a_ring_the_entry_is_not_in_is_not_its_alias() {
	let mut world = World::new();
	let entry = world.entry(5);
	world.place(entry, 9, Finality::Finalized);
	let id = unload(&mut world, &[entry]);
	world.unload_alias_in(entry, 7, Finality::Finalized);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

/// Archiving a ring removes the membership while the entry may still be
/// redeemable, so its absence proves nothing either way.
#[test]
fn an_archived_entry_proves_nothing() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	world.archive(entry, Finality::Finalized);
	assert_eq!(world.proof(id), NOTHING);
}

#[test]
fn an_onboarding_entry_that_outlives_the_window_fails_its_unload() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let entry = entry_at(&mut world, RingPosition::Onboarding);
	let out = world.coin(2);
	let id = record(&mut world, Call::UnloadToCoins, &[entry], &[out]);
	let account = world.account(out);
	world.chain().tree().faults.coins.insert(account);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}

/// One unknown alias does not take away what an onboarding entry of the same
/// record proves.
#[test]
fn a_failed_alias_read_does_not_erase_what_an_onboarding_entry_proves() {
	let mut world = World::new();
	let onboarding = world.entry(6);
	world.position(onboarding, RingPosition::Onboarding, Finality::Finalized);
	let in_ring = entry_in_ring(&mut world);
	let id = unload(&mut world, &[onboarding, in_ring]);
	let (_, alias) = world.entry_keys(in_ring);
	world.chain().tree().faults.aliases.insert(alias);
	assert_eq!(world.proof(id), NOT_COMPLETED);
}

/// A suspended entry holds no ring index to read its alias under, and may
/// have been unloaded before it left its ring.
#[test]
fn a_suspended_entry_is_present_but_says_nothing_about_its_alias() {
	let mut world = World::new();
	let entry = entry_at(&mut world, RingPosition::Suspended);
	let id = unload(&mut world, &[entry]);
	assert_eq!(world.proof(id), NOTHING);
}

#[test]
fn a_failed_membership_read_leaves_an_entry_unknown_not_gone() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	world.chain().tree().faults.memberships = true;
	assert_eq!(world.proof(id), NOTHING);
}

#[test]
fn a_failed_position_read_leaves_an_entry_unknown_not_gone() {
	let mut world = World::new();
	let entry = entry_in_ring(&mut world);
	let id = unload(&mut world, &[entry]);
	world.chain().tree().faults.positions = true;
	assert_eq!(world.proof(id), NOTHING);
}

#[test]
fn a_failed_entry_read_leaves_the_unload_pending_without_failing_the_pass() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let entry = entry_in_ring(&mut world);
	let out = world.coin(2);
	let id = record(&mut world, Call::UnloadToCoins, &[entry], &[out]);
	let account = world.account(out);
	world.chain().tree().faults.coins.insert(account);
	world.chain().tree().faults.memberships = true;
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}
