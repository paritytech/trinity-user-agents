//! The `coinage` oracle's rules, read at a [`World`]'s heads and driven
//! through the durability layer's passes.

use crate::{
	coinage::{
		Asset,
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

/// A world whose search never decides anything, so only the oracle's rules
/// can.
fn without_search() -> World {
	let world = World::new();
	world.chain().tree().faults.bodies = true;
	world
}

/// Coin 2 minted from coin 1 by a record that succeeded finally, with that
/// record's window closed at the finalized head when `closed`.
fn settled_coin(world: &mut World, closed: bool) -> Asset {
	let (seed, coin) = (world.coin(1), world.coin(2));
	world.mint(&[seed], Finality::Finalized);
	let minter = world.decided(seed, coin, Finality::Finalized);
	assert_eq!(world.standing(minter), Standing::FinalizedSuccess);
	if closed {
		world.reach_mortality_of(minter, Finality::Finalized);
	}
	coin
}

/// A record spending `inputs` into nothing the chain shows, as an offboard
/// does, its watch released.
fn offboard(world: &mut World, admission: Admission, inputs: &[Asset]) -> DurableTxId {
	let params = RecordParams { call: Call::UnloadToExternal([7; 32]), retry: Retry::Never };
	let rows = Rows { inputs: inputs.to_vec(), outputs: Vec::new(), tokens: Vec::new() };
	let id = world.register_with(admission, vec![rows], params, true).unwrap()[0];
	world.release(&[id]);
	id
}

/// Only a successful unload marks an alias unloaded.
#[test]
fn an_unloaded_entry_input_proves_completion() {
	let mut world = World::new();
	let (entry, out) = (world.entry(5), world.coin(2));
	world.place(entry, 7, Finality::Finalized);
	let id = world.register(&[entry], &[out]).unwrap();
	world.release(&[id]);
	world.unload_alias(entry, Finality::Finalized);
	let out_account = world.account(out);
	world.chain().tree().faults.coins.insert(out_account);
	assert_eq!(world.proof(id), COMPLETED);
}

/// Only this record creates its output and nothing could have spent it, so
/// its absence past the window means the record never ran.
#[test]
fn an_untouched_coin_output_absent_after_the_window_fails_its_record() {
	let mut world = without_search();
	let out = world.coin(2);
	let id = world.register(&[], &[out]).unwrap();
	world.release(&[id]);
	assert_eq!(world.proof(id), NOT_COMPLETED);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}

#[test]
fn every_settled_input_gone_at_the_finalized_head_proves_completion() {
	let mut world = World::new();
	let coin = settled_coin(&mut world, true);
	let id = offboard(&mut world, Admission::Ordinary, &[coin]);
	world.consume(coin, Finality::Finalized);
	assert_eq!(world.proofs(id), [COMPLETED, COMPLETED]);
}

/// A split registered but never included: its input is still there at both
/// heads, which proves it has not run, never that it has.
#[test]
fn an_input_present_at_both_heads_proves_the_record_has_not_run() {
	let mut world = without_search();
	let coin = settled_coin(&mut world, true);
	let out = world.coin(3);
	let id = world.unwatched(coin, out);
	assert_eq!(world.proofs(id), [NOT_COMPLETED, NOT_COMPLETED]);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

/// Someone else holds a disclosed coin's key, so its absence says nothing
/// about this record.
#[test]
fn a_disclosed_input_is_never_settled() {
	let mut world = World::new();
	let coin = settled_coin(&mut world, true);
	world.disclose(coin, true).unwrap();
	let id = offboard(&mut world, Admission::Reclaim, &[coin]);
	world.consume(coin, Finality::Finalized);
	assert_eq!(world.proofs(id), [NOTHING, NOTHING]);
}

/// Design C2: an input is settled only once its minter's window is closed at
/// the finalized head.
#[test]
fn an_input_whose_minters_window_is_open_is_not_settled() {
	let mut world = World::new();
	let coin = settled_coin(&mut world, false);
	let id = offboard(&mut world, Admission::Ordinary, &[coin]);
	world.consume(coin, Finality::Finalized);
	assert_eq!(world.proofs(id), [NOTHING, NOTHING]);
}

#[test]
fn a_failed_read_of_a_settled_input_proves_nothing() {
	let mut world = World::new();
	let coin = settled_coin(&mut world, true);
	let id = offboard(&mut world, Admission::Ordinary, &[coin]);
	let account = world.account(coin);
	world.chain().tree().faults.coins.insert(account);
	assert_eq!(world.proofs(id), [NOTHING, NOTHING]);
}

/// The same scenario through a pass: an input whose minter's window is still
/// open proves nothing by its absence, so the record stays pending.
#[test]
fn a_settled_inputs_absence_decides_nothing_while_its_minters_window_is_open() {
	let mut world = without_search();
	let coin = settled_coin(&mut world, false);
	let out = world.coin(3);
	let id = world.unwatched(coin, out);
	world.consume(coin, Finality::InBest);
	world.finalize();
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

#[test]
fn a_coin_the_wallet_did_not_mint_never_proves_a_spend() {
	let mut world = without_search();
	let (spent, out) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, out);
	world.consume(spent, Finality::InBest);
	world.run_pass();
	assert_eq!((world.standing(id), world.minter(out)), (Standing::Pending, Some(Standing::Pending)));
}

#[test]
fn a_failed_read_never_lets_a_settled_input_count_as_gone() {
	let mut world = without_search();
	let coin = settled_coin(&mut world, true);
	let out = world.coin(3);
	let id = world.unwatched(coin, out);
	let account = world.account(coin);
	world.chain().tree().faults.coins.insert(account);
	world.advance(1, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

/// The minter's output is gone because this layer's own later record spent
/// it: without the live-consumer guard the minter would fail while it
/// succeeded, and its input would be free to spend twice.
#[test]
fn a_live_consumer_keeps_an_absent_output_from_failing_its_minter() {
	let mut world = without_search();
	let [a, b, c] = [1, 2, 3].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let minter = world.executed(a, b, Finality::Finalized);
	world.unwatched(b, c);
	world.consume(b, Finality::Finalized);
	world.reach_mortality_of(minter, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(minter), Standing::Pending);
}

#[test]
fn the_same_absent_output_with_no_consumer_fails_its_minter() {
	let mut world = without_search();
	let [a, b] = [1, 2].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let minter = world.executed(a, b, Finality::Finalized);
	world.consume(b, Finality::Finalized);
	world.reach_mortality_of(minter, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(minter), Standing::Failed(DurableTxFailureKind::Expired));
}

/// A record with no output to look for is decided by its settled inputs
/// being gone: nobody else holds their keys.
#[test]
fn a_spend_into_nothing_trackable_succeeds_from_its_settled_input_being_gone() {
	let mut world = without_search();
	let coin = settled_coin(&mut world, true);
	let id = offboard(&mut world, Admission::Ordinary, &[coin]);
	world.consume(coin, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::FinalizedSuccess);
}

#[test]
fn the_same_spend_seen_only_on_the_best_chain_is_a_success_not_yet_final() {
	let mut world = without_search();
	let coin = settled_coin(&mut world, true);
	let id = offboard(&mut world, Admission::Ordinary, &[coin]);
	world.consume(coin, Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::PendingSuccess);
}

/// Past the window, best-head evidence is no reason to wait: the search
/// decides, and the payment's coin is free again.
#[test]
fn a_payment_that_never_landed_fails_when_only_the_best_head_reads() {
	let mut world = World::new();
	let (spent, out) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, out);
	world.reach_mortality_of(id, Finality::Finalized);
	world.advance(1, Finality::InBest);
	let finalized = world.chain().tree().finalized().hash;
	world.chain().tree().faults.coins_at.insert(finalized);
	world.run_pass();
	assert_eq!((world.standing(id), world.holders(spent)), (Standing::Failed(DurableTxFailureKind::Expired), 0));
}

/// The record a coin's absence would fail is held up only by evidence that
/// cannot reverse: before the window closes the same reads leave it pending.
#[test]
fn the_expiry_rules_wait_for_the_window_to_close() {
	let mut world = without_search();
	let (spent, out) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, out);
	world.advance(4, Finality::Finalized);
	world.run_pass();
	let before = world.standing(id);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!([before, world.standing(id)], [Standing::Pending, Standing::Failed(DurableTxFailureKind::Expired)]);
}

