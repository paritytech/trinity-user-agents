//! Submission watches of coinage records on a [`World`].

use core::time::Duration;

use crate::{
	chain::{DispatchOutcome, WatchEvent},
	coinage::testing::world::{Finality, Standing, World},
	test_support::wait_until,
};

/// A world with coins 1 and 3 on chain.
fn funded() -> World {
	let mut world = World::new();
	let coins = [world.coin(1), world.coin(3)];
	world.mint(&coins, Finality::Finalized);
	world
}

/// Without the block's number the watch cannot tell which block the success
/// is at, so it records none.
#[test]
fn an_inclusion_in_a_block_whose_number_cannot_be_read_records_nothing() {
	let mut world = funded();
	let (spent, out) = (world.coin(1), world.coin(2));
	let id = world.register(&[spent], &[out]).unwrap();
	let block = world.include(id, DispatchOutcome::Succeeded, Finality::InBest);
	world.chain().tree().faults.numbers = true;
	let watch = world.chain().watch(world.attempt(id).tx_hash).unwrap();
	watch.unbounded_send(WatchEvent::InBestBlock(block.hash)).unwrap();
	drop(watch);
	world.release(&[id]);
	assert_eq!(world.status(id), crate::durable::DurableTxStatus::Pending);
}

#[test]
fn a_failed_watch_does_not_take_down_the_watch_of_another_record() {
	let mut world = funded();
	let [a, b, c, d] = [1, 2, 3, 4].map(|n| world.coin(n));
	let failing = world.register(&[a], &[b]).unwrap();
	let watched = world.register(&[c], &[d]).unwrap();
	let failing_watch = world.chain().watch(world.attempt(failing).tx_hash).unwrap();
	failing_watch.unbounded_send(WatchEvent::Error("the subscription failed".into())).unwrap();
	let block = world.include(watched, DispatchOutcome::Succeeded, Finality::Finalized);
	let watch = world.chain().watch(world.attempt(watched).tx_hash).unwrap();
	watch.unbounded_send(WatchEvent::InFinalizedBlock(block.hash)).unwrap();
	world.wait_for(watched, crate::durable::DurableTxStatus::FinalizedSuccess { at: Some(block) });
	wait_until(|| !world.engine().is_watched(failing), "the failed watch hands its record back");
	assert_eq!(world.standing(failing), Standing::Pending);
}

/// A watch that fell silent has let its record go: an event arriving after
/// that changes nothing, and nothing is submitted again.
#[test]
fn a_late_event_after_a_silent_watch_released_changes_nothing() {
	let mut world = funded();
	let (spent, out) = (world.coin(1), world.coin(2));
	let id = world.register(&[spent], &[out]).unwrap();
	let late = world.chain().watch(world.attempt(id).tx_hash).unwrap();
	wait_until(|| !world.timer().remaining().is_empty(), "the watch waits for an event");
	world.timer().advance(Duration::from_secs(30));
	wait_until(|| !world.engine().is_watched(id), "the silent watch releases the record");
	let block = world.include(id, DispatchOutcome::Succeeded, Finality::Finalized);
	let _ = late.unbounded_send(WatchEvent::InFinalizedBlock(block.hash));
	world.settle();
	assert_eq!((world.standing(id), world.chain().tree().submitted.len()), (Standing::Pending, 1));
}
