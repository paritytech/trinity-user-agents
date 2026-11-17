//! What a [`World`]'s records do to the assets they hold, mint and
//! disclose, and how chains of them resolve.

use futures::executor::block_on;

use crate::{
	coinage::{
		ledger::Mark,
		operation::Retry,
		planning::Call,
		registration::{Admission, RegisterError, Rows, coinage_domain},
		start::RecordParams,
		testing::world::{Finality, Standing, World},
	},
	durable::DurableTxFailureKind,
};

/// A disclosed output may be spent by the payee, so its absence fails
/// nothing; the search then proves the minter succeeded.
#[test]
fn a_minter_succeeds_though_its_disclosed_output_was_spent() {
	let mut world = World::new();
	let [a, b] = [1, 2].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let id = world.executed(a, b, Finality::Finalized);
	world.disclose(b, true).unwrap();
	world.consume(b, Finality::InBest);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!((world.standing(id), world.mark(b)), (Standing::FinalizedSuccess, Some(Mark::Committed)));
}

/// Without the mark, the same spent output fails a minter that succeeded:
/// the gap design section 5 and C2 accept, recorded here.
#[test]
fn a_minter_without_the_disclosure_mark_fails_on_its_missing_output() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let [a, b] = [1, 2].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let id = world.executed(a, b, Finality::Finalized);
	world.consume(b, Finality::InBest);
	world.reach_mortality_of(id, Finality::Finalized);
	world.run_pass();
	assert_eq!(world.standing(id), Standing::Failed(DurableTxFailureKind::Expired));
}

/// Each pass reads one snapshot, so a final consumer promotes its minter
/// one hop per pass.
#[test]
fn a_chain_of_three_succeeds_hop_by_hop_once_the_last_finalizes() {
	let mut world = World::new();
	world.chain().tree().faults.bodies = true;
	let [a, b, c, e] = [1, 2, 3, 6].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let first = world.unwatched(a, b);
	let second = world.unwatched(b, c);
	let third = world.unwatched(c, e);
	world.mint(&[e], Finality::InBest);
	world.finalize();
	for _ in 0..3 {
		world.run_pass();
	}
	assert_eq!([first, second, third].map(|id| world.standing(id)), [Standing::FinalizedSuccess; 3]);
}

/// The search decides each record on its own window, so a chain whose first
/// record never ran resolves in one pass after the last window, not one
/// window per hop.
#[test]
fn a_chain_whose_first_record_fails_resolves_every_successor_within_one_window() {
	let mut world = World::new();
	let [a, b, c, e] = [1, 2, 3, 6].map(|n| world.coin(n));
	world.mint(&[a], Finality::Finalized);
	let first = world.unwatched(a, b);
	let second = world.unwatched(b, c);
	let third = world.unwatched(c, e);
	world.reach_mortality_of(third, Finality::Finalized);
	world.run_pass();
	assert_eq!(
		[first, second, third].map(|id| world.standing(id)),
		[Standing::Failed(DurableTxFailureKind::Expired); 3]
	);
}

/// A peer's transfer may not be included yet when it is claimed, and the
/// claim holds the coin like any input.
#[test]
fn a_peer_coin_not_yet_on_chain_is_claimed_once() {
	let mut world = World::new();
	let peer = world.peer_coin(50);
	let [b, c] = [2, 3].map(|n| world.coin(n));
	let claim = |world: &mut World, output| {
		let params = RecordParams { call: Call::Transfer, retry: Retry::Never };
		let rows = Rows { inputs: vec![peer], outputs: vec![output], tokens: Vec::new() };
		world.register_with(Admission::Import, vec![rows], params, true).map(|ids| ids.len())
	};
	assert_eq!([claim(&mut world, b), claim(&mut world, c)], [Ok(1), Err(RegisterError::InputTaken(peer))]);
}

/// A batch is one operation: every record is listed in its group, and each
/// first attempt is watched.
#[test]
fn a_batch_registers_every_record_in_its_group_and_watches_each() {
	let mut world = World::new();
	let [a, b, c, d] = [1, 2, 3, 4].map(|n| world.coin(n));
	world.mint(&[a, c], Finality::Finalized);
	let rows = |input, output| Rows { inputs: vec![input], outputs: vec![output], tokens: Vec::new() };
	let params = RecordParams { call: Call::Transfer, retry: Retry::Never };
	let ids = world.register_with(Admission::Ordinary, vec![rows(a, b), rows(c, d)], params, true).unwrap();
	let operation = world.view().registrations().record(ids[0]).unwrap().operation.clone();
	let group = block_on(world.engine().group(&coinage_domain(), &operation.group())).unwrap();
	let listed: Vec<_> = group.iter().map(|state| state.id).collect();
	assert_eq!((listed, ids.iter().all(|id| world.engine().is_watched(*id))), (ids, true));
}
