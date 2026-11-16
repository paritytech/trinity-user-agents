//! The whole engine with the coinage plug-ins over a [`World`]: what a
//! registration holds, what a crash keeps, and what a pass reads.

use crate::{
	coinage::testing::world::{Finality, Standing, World},
	durable::DurableTxFailureKind,
};

/// The input is held from the registration's commit, and the watch of its
/// first attempt keeps every pass away from it.
#[test]
fn a_registration_holds_its_input_and_its_watch_owns_the_record() {
	let mut world = World::new();
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.register(&[spent], &[minted]).unwrap();
	assert_eq!((world.consumer(spent), world.engine().is_watched(id)), (Some(Standing::Pending), true));
}

/// Ownership lives in memory only, so a new process decides the record
/// from the chain, while the hold, a row, survives.
#[test]
fn a_crash_keeps_the_ledger_and_drops_the_watch() {
	let mut world = World::new();
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.register(&[spent], &[minted]).unwrap();
	world.crash();
	assert_eq!(
		(world.standing(id), world.consumer(spent), world.engine().is_watched(id)),
		(Standing::Pending, Some(Standing::Pending), false)
	);
}

#[test]
fn an_output_present_at_the_finalized_head_finalizes_the_record() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, minted);
	assert!(world.recovery_requested(), "a watch that ends without a verdict asks for recovery");
	world.mint(&[minted], Finality::InBest);
	world.finalize();
	world.run_pass();
	assert_eq!(world.standing(id), Standing::FinalizedSuccess);
}

/// A failed read is unknown, never absent, so it decides nothing and the
/// input stays held.
#[test]
fn a_record_whose_reads_all_fail_keeps_its_status_and_its_hold() {
	let mut world = World::new();
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, minted);
	let accounts = [world.account(spent), world.account(minted)];
	world.chain().tree().faults.coins.extend(accounts);
	world.advance(1, Finality::Finalized);
	world.run_pass();
	assert_eq!((world.standing(id), world.consumer(spent)), (Standing::Pending, Some(Standing::Pending)));
}

/// The pass lock is in memory, so a crash in the middle of a pass cannot
/// leave a lock that stops every later pass.
#[test]
fn a_crash_after_a_failed_pass_does_not_stop_the_next_pass() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, minted);
	world.chain().tree().faults.heads = true;
	world.run_pass();
	world.crash();
	world.chain().tree().faults.heads = false;
	world.mint(&[minted], Finality::InBest);
	world.finalize();
	world.run_pass();
	assert_eq!(world.standing(id), Standing::FinalizedSuccess);
}

#[test]
fn a_pass_that_cannot_read_the_chain_leaves_every_record_to_the_next_one() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let [a, b, c, d] = [1, 2, 3, 4].map(|n| world.coin(n));
	world.mint(&[a, d], Finality::Finalized);
	let first = world.unwatched(a, b);
	let second = world.unwatched(d, c);
	world.mint(&[b], Finality::InBest);
	world.mint(&[c], Finality::InBest);
	world.finalize();
	world.chain().tree().faults.heads = true;
	world.run_pass();
	assert_eq!([world.standing(first), world.standing(second)], [Standing::Pending; 2]);
	world.chain().tree().faults.heads = false;
	world.run_pass();
	assert_eq!([world.standing(first), world.standing(second)], [Standing::FinalizedSuccess; 2]);
}

/// A failed split is rebuilt by the policy from the same rows, so its
/// outputs keep their addresses and no second record mints them.
#[test]
fn a_failed_split_is_rebuilt_with_the_same_outputs() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let a = world.coin_of(1, 2);
	let [b, c] = [2, 3].map(|n| world.coin_of(n, 1));
	world.mint(&[a], Finality::Finalized);
	world.observe();
	let id = world.register_retriable(&[a], &[b, c]).unwrap();
	let rows = world.rows(id);
	world.release(&[id]);
	world.reach_mortality_of(id, Finality::Finalized);
	world.observe();
	world.run_pass();
	world.pass_rebuild_cooldown();
	world.wait_for_submissions(2);
	assert_eq!(
		(world.standing(id), world.asked(), world.rows(id), world.view().registrations().records().count()),
		(Standing::Pending, vec![vec![id]], rows, 1)
	);
	assert_eq!([world.minter(b), world.minter(c)], [Some(Standing::Pending); 2]);
}

/// Finality decides; a best chain far past the window decides nothing.
#[test]
fn best_chain_height_alone_gives_no_verdict_while_finality_stalls() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, minted);
	world.reach_mortality_of(id, Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

#[test]
fn a_pass_with_nothing_to_decide_reads_no_chain() {
	let mut world = World::new();
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.decided(spent, minted, Finality::Finalized);
	assert_eq!(world.standing(id), Standing::FinalizedSuccess);
	let reads = world.chain().tree().heads_reads;
	world.run_pass();
	assert_eq!(world.chain().tree().heads_reads, reads, "a settled ledger costs no chain read");
}

/// The oracle's expiry, written as a failure the policy does not retry.
#[test]
fn an_input_still_on_chain_after_the_window_fails_its_record() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (spent, minted) = (world.coin(1), world.coin(2));
	world.mint(&[spent], Finality::Finalized);
	let id = world.unwatched(spent, minted);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}
