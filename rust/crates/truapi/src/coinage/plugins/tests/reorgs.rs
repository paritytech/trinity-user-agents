//! Reorganisations of a [`World`]'s best chain under records the oracle
//! and the search decide.

use crate::{
	chain::DispatchOutcome,
	coinage::{
		operation::Retry,
		planning::Call,
		registration::{Admission, Rows},
		start::RecordParams,
		testing::world::{Finality, Standing, World},
	},
	durable::{DurableTxFailureKind, DurableTxStatus},
};

/// A world whose search never decides anything.
fn without_search() -> World {
	let world = World::new();
	world.chain().tree().faults.bodies = true;
	world
}

/// A coin seen only on a fork never existed: once the fork is gone, the
/// minter's input is available again and its window decides it.
#[test]
fn an_output_reorged_out_no_longer_exists_and_its_minter_fails_on_its_window() {
	let mut world = without_search();
	let [a, b] = [1, 2].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let minter = world.unwatched(a, b);
	world.mint(&[b], Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(minter), Standing::PendingSuccess);
	world.reorg(1);
	world.advance(1, Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(minter), Standing::Pending);
	world.reach_mortality_of(minter, Finality::Finalized);
	world.run_pass();
	let expired = Some(Standing::Failed(DurableTxFailureKind::Expired));
	assert_eq!((Some(world.standing(minter)), world.minter(b)), (expired, expired));
}

/// The same attempt applied in two blocks with opposite outcomes: only the
/// canonical block's outcome counts.
#[test]
fn an_outcome_is_read_from_the_block_the_extrinsic_was_found_in() {
	let mut world = World::new();
	let [a, b] = [1, 2].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let id = world.unwatched(a, b);
	let tx = world.attempt(id).tx_hash;
	let orphaned = world.include(id, DispatchOutcome::Failed("Coinage.Locked".into()), Finality::InBest);
	world.reorg(1);
	world.include(id, DispatchOutcome::Succeeded, Finality::InBest);
	let accounts = [world.account(a), world.account(b)];
	world.chain().tree().faults.coins.extend(accounts);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	let orphaned_outcome = world.chain().tree().outcome_in(orphaned.hash, tx);
	assert_eq!(
		(world.standing(id), orphaned_outcome),
		(Standing::FinalizedSuccess, Some(DispatchOutcome::Failed("Coinage.Locked".into())))
	);
}

/// The reorg retracts the minter's own transaction with the coin it minted:
/// the minter's input is back, and the consumer's input never existed.
#[test]
fn a_retracted_mint_fails_both_the_minter_and_its_consumer() {
	let mut world = without_search();
	let [a, b, c] = [1, 2, 3].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let minter = world.executed(a, b, Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(minter), Standing::PendingSuccess);
	let consumer = world.unwatched(b, c);
	world.reorg(1);
	world.advance(1, Finality::InBest);
	world.run_pass();
	assert_eq!([world.standing(minter), world.standing(consumer)], [Standing::Pending; 2]);
	world.reach_mortality_of(consumer, Finality::Finalized);
	world.run_pass();
	assert_eq!(
		[world.standing(minter), world.standing(consumer)],
		[Standing::Failed(DurableTxFailureKind::Expired); 2]
	);
}

/// An import a fork invalidated is built again into the coin already
/// recorded, so the payment that spends that coin still completes.
///
/// A peer's coin is never available, so unlike on Android no oracle rule
/// fails the import past its window: the search does, and the search stays
/// on here (design C2).
#[test]
fn an_import_a_fork_invalidated_is_built_again_into_the_same_coin() {
	let mut world = World::new();
	let peer = world.peer_coin(50);
	let [b, c] = [2, 3].map(|n| world.coin(n));
	world.mint(&[peer], Finality::Finalized);
	world.observe();
	let params = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly };
	let rows = Rows { inputs: vec![peer], outputs: vec![b], tokens: Vec::new() };
	let import = world.register_with(Admission::Import, vec![rows], params, true).unwrap()[0];
	let spender = world.register_retriable(&[b], &[c]).unwrap();
	world.release(&[import, spender]);
	world.consume(peer, Finality::InBest);
	world.mint(&[b], Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(import), Standing::PendingSuccess);
	world.reorg(2);
	world.advance(1, Finality::InBest);
	world.run_pass();
	assert_eq!(world.standing(import), Standing::Pending);
	world.reach_mortality_of(import, Finality::Finalized);
	world.observe();
	world.run_pass();
	world.pass_rebuild_cooldown();
	world.wait_for_submissions(3);
	assert!(world.asked().concat().contains(&import), "the import was built again");
	assert_eq!((world.standing(import), world.rows(import).outputs), (Standing::Pending, vec![b]));
	world.release(&[import]);
	world.consume(peer, Finality::Finalized);
	world.mint(&[b], Finality::Finalized);
	world.observe();
	world.run_pass();
	assert_eq!(world.standing(import), Standing::FinalizedSuccess);
	world.wait_for(spender, DurableTxStatus::Pending);
	world.release(&[spender]);
	world.consume(b, Finality::Finalized);
	world.mint(&[c], Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(spender), Standing::FinalizedSuccess);
}
