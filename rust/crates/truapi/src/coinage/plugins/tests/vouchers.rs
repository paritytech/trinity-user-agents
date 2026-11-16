//! Records that spend or create recycler entries, decided through a
//! [`World`]'s passes.

use crate::{
	coinage::{
		Asset,
		classification::{Class, EntryLocal, classify},
		testing::{
			chain_params,
			world::{Finality, Standing, World},
		},
	},
	durable::{DurableTxFailureKind, DurableTxId},
};

/// A world whose search never decides anything, with entry 5 in ring 7 and
/// an unload of it into coin 2 released, whose coin cannot be read, so the
/// entry is the record's only evidence.
fn blinded_unload() -> (World, Asset, DurableTxId) {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (entry, out) = (world.entry(5), world.coin(2));
	world.place(entry, 7, Finality::Finalized);
	let id = world.register(&[entry], &[out]).unwrap();
	world.release(&[id]);
	let account = world.account(out);
	world.chain().tree().faults.coins.insert(account);
	(world, entry, id)
}

#[test]
fn an_entry_still_in_its_ring_after_the_window_fails_its_unload() {
	let (world, _, id) = blinded_unload();
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}

/// For a coin, absence is consumption; for an entry it is ring cleanup.
#[test]
fn an_entry_whose_ring_is_archived_mid_unload_decides_nothing() {
	let (world, entry, id) = blinded_unload();
	world.archive(entry, Finality::Finalized);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

/// An unreadable alias is not a "not unloaded" one: if the unload ran, its
/// output exists, and failing the record would drop that value.
#[test]
fn an_unreadable_alias_decides_nothing_and_the_entry_stays_held() {
	let (world, entry, id) = blinded_unload();
	world.reach_mortality_of(id, Finality::Finalized);
	let (_, alias) = world.entry_keys(entry);
	world.chain().tree().faults.aliases.insert(alias);
	world.run_pass();
	assert_eq!((world.standing(id), world.consumer(entry)), (Standing::Pending, Some(Standing::Pending)));
}

#[test]
fn the_same_unload_finalizes_once_its_alias_reads_unloaded() {
	let (world, entry, id) = blinded_unload();
	world.unload_alias(entry, Finality::Finalized);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::FinalizedSuccess);
}

#[test]
fn an_entry_minted_as_an_output_is_arriving_while_its_minter_is_pending() {
	let mut world = World::new();
	let (coin, entry) = (world.coin(1), world.entry(11));
	world.mint(&[coin], Finality::Finalized);
	let id = world.register(&[coin], &[entry]).unwrap();
	let class = classify(&entry, &world.view(), world.now(), &chain_params()).class;
	assert_eq!(
		(world.standing(id), world.minter(entry), world.mark(entry)),
		(Standing::Pending, Some(Standing::Pending), None)
	);
	assert!(matches!(class, Class::Entry { local: EntryLocal::Minting, .. }), "{class:?}");
}

/// Only an untouched coin output's absence proves anything: an entry
/// output's says nothing.
#[test]
fn an_entry_output_that_says_nothing_does_not_fail_its_minter() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (coin, entry) = (world.coin(1), world.entry(11));
	world.mint(&[coin], Finality::Finalized);
	world.place(entry, 7, Finality::Finalized);
	world.archive(entry, Finality::Finalized);
	let id = world.register(&[coin], &[entry]).unwrap();
	world.release(&[id]);
	world.consume(coin, Finality::InBest);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Pending);
}

#[test]
fn a_coin_output_that_reads_absent_fails_its_minter() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let (coin, out) = (world.coin(1), world.coin(2));
	world.mint(&[coin], Finality::Finalized);
	let id = world.unwatched(coin, out);
	world.consume(coin, Finality::InBest);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}
