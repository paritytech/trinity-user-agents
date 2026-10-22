use core::num::NonZeroU32;

use futures::{StreamExt, executor::block_on};
use tempfile::TempDir;

use super::*;
use crate::{
	durable::RegistrationError,
	nft::{HeldItem, NftLedger, PageRequest, PurseTag, testing::test_ledger},
	store::{Db, open_temporary_core_db},
};

const FROM: PurseId = PurseId(0);
const TO: PurseId = PurseId(1);
const FOREIGN: ItemKey = ItemKey([0xee; 32]);

/// A ledger with two purses, and `count` keys of [`FROM`] that hold
/// instances 100, 101, ...
struct Purses {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	held: Vec<ItemKey>,
}

fn purses_holding(count: u32) -> Purses {
	let (dir, db) = open_temporary_core_db();
	let ledger = test_ledger(db.clone());
	for name in ["from", "to"] {
		block_on(ledger.create_purse(name.to_owned(), PurseTag::default())).unwrap();
	}
	let count = NonZeroU32::new(count).unwrap();
	let held = block_on(ledger.allocate_items(FROM, count)).unwrap();
	let holding = |(index, key): (usize, &ItemKey)| {
		(*key, Some(HeldItem { collection: 1, item: 1, instance: 100 + index as u64 }))
	};
	block_on(ledger.record_holdings(held.iter().enumerate().map(holding).collect())).unwrap();
	Purses { _dir: dir, db, ledger, held }
}

fn operation(byte: u8) -> OperationKey {
	OperationKey::new(b"product", [byte; 32])
}

fn rebalance(operation: OperationKey, items: &[ItemKey]) -> MoveRequest {
	let items = items.iter().copied().collect();
	MoveRequest { operation, from: FROM, items, destinations: MoveDestinations::AllocatedIn(TO) }
}

fn start(
	purses: &Purses,
	request: MoveRequest,
) -> Result<(), RegistrationError<StartOperationError>> {
	block_on(purses.ledger.start_moves(request))
}

fn available(purses: &Purses, purse: PurseId) -> Vec<ItemKey> {
	let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
	block_on(purses.ledger.available_items(purse, page)).unwrap().items
}

fn moves_of(purses: &Purses, operation: &OperationKey) -> Vec<MoveProgress> {
	block_on(purses.ledger.observe_operation(operation).next()).unwrap().unwrap()
}

fn item_rows(purses: &Purses) -> i64 {
	let count = |conn: &rusqlite::Connection| {
		Ok(conn.query_row("SELECT count(*) FROM nft_item", [], |row| row.get(0))?)
	};
	block_on(purses.db.read(count)).unwrap()
}

fn set_move_status(purses: &Purses, status: &str) {
	let status = status.to_owned();
	let update = move |tx: &rusqlite::Transaction<'_>| {
		tx.execute("UPDATE durable_tx SET status = ?1, failure_kind = 'REJECTED'", [status])?;
		Ok(())
	};
	block_on(purses.db.write(update)).unwrap();
}

#[test]
fn a_rebalance_registers_one_waiting_move_per_item_into_fresh_keys_of_the_destination() {
	let purses = purses_holding(2);

	start(&purses, rebalance(operation(1), &purses.held)).unwrap();

	let moves = moves_of(&purses, &operation(1));
	let consumed: BTreeSet<_> = moves.iter().map(|progress| progress.consumed).collect();
	let minted: Vec<_> = moves.iter().map(|progress| progress.minted).collect();
	assert_eq!(consumed, purses.held.iter().copied().collect());
	assert!(moves.iter().all(|progress| progress.state == MoveState::Waiting));
	assert_eq!(item_rows(&purses), 4);
	assert!(minted.iter().all(|key| !purses.held.contains(key)));
}

#[test]
fn an_item_spent_by_a_live_move_is_not_available() {
	let purses = purses_holding(2);

	start(&purses, rebalance(operation(1), &purses.held[..1])).unwrap();

	assert_eq!(available(&purses, FROM), purses.held[1..].to_vec());
}

#[test]
fn an_item_is_available_again_once_its_move_failed() {
	let purses = purses_holding(1);
	start(&purses, rebalance(operation(1), &purses.held)).unwrap();

	set_move_status(&purses, "FAILURE");

	assert_eq!(available(&purses, FROM), purses.held);
	assert_eq!(moves_of(&purses, &operation(1))[0].state, MoveState::Failed);
}

#[test]
fn a_handed_off_item_is_not_available_whatever_its_moves_say() {
	let purses = purses_holding(1);
	let key = purses.held[0];
	let hand_off = move |tx: &rusqlite::Transaction<'_>| {
		tx.execute("INSERT INTO nft_consumption (key, kind) VALUES (?1, 'HANDOFF')", [key.0])?;
		Ok(())
	};
	block_on(purses.db.write(hand_off)).unwrap();

	let refused = start(&purses, rebalance(operation(1), &purses.held));

	assert_eq!(available(&purses, FROM), vec![]);
	assert!(
		matches!(refused, Err(RegistrationError::Hook(StartOperationError::ItemUnavailable(item))) if item == key)
	);
}

#[test]
fn a_second_registration_under_the_same_key_is_refused_and_writes_nothing() {
	let purses = purses_holding(2);
	start(&purses, rebalance(operation(1), &purses.held[..1])).unwrap();

	let again = start(&purses, rebalance(operation(1), &purses.held[1..]));

	assert!(matches!(again, Err(RegistrationError::Hook(StartOperationError::OperationExists))));
	assert_eq!(moves_of(&purses, &operation(1)).len(), 1);
	assert_eq!(item_rows(&purses), 3);
}

#[test]
fn a_new_operation_over_an_item_in_use_is_refused() {
	let purses = purses_holding(1);
	start(&purses, rebalance(operation(1), &purses.held)).unwrap();

	let overlapping = start(&purses, rebalance(operation(2), &purses.held));

	let in_use = StartOperationError::ItemUnavailable(purses.held[0]);
	assert!(
		matches!(overlapping, Err(RegistrationError::Hook(refusal)) if refusal.to_string() == in_use.to_string())
	);
}

#[test]
fn a_refused_operation_allocates_no_destination_key() {
	let purses = purses_holding(1);
	let not_held = ItemKey([0x55; 32]);

	let refused = start(&purses, rebalance(operation(1), &[purses.held[0], not_held]));

	assert!(
		matches!(refused, Err(RegistrationError::Hook(StartOperationError::ItemNotFound(item))) if item == not_held)
	);
	assert_eq!(item_rows(&purses), 1);
	assert_eq!(moves_of(&purses, &operation(1)), vec![]);
}

#[test]
fn two_identical_calls_at_once_register_one_operation() {
	let purses = purses_holding(1);

	let (first, second) = block_on(futures::future::join(
		purses.ledger.start_moves(rebalance(operation(1), &purses.held)),
		purses.ledger.start_moves(rebalance(operation(1), &purses.held)),
	));

	let refused = [&first, &second].iter().filter(|result| result.is_err()).count();
	assert_eq!(refused, 1);
	assert_eq!(moves_of(&purses, &operation(1)).len(), 1);
}

#[test]
fn a_transfer_moves_one_item_to_the_foreign_key_without_allocating() {
	let purses = purses_holding(1);
	let request = MoveRequest {
		operation: operation(1),
		from: FROM,
		items: purses.held.iter().copied().collect(),
		destinations: MoveDestinations::Foreign(FOREIGN),
	};

	start(&purses, request).unwrap();

	let moves = moves_of(&purses, &operation(1));
	assert_eq!(moves.iter().map(|progress| progress.minted).collect::<Vec<_>>(), vec![FOREIGN]);
	assert_eq!(item_rows(&purses), 1);
}

#[test]
fn an_operation_never_registered_has_no_moves() {
	let purses = purses_holding(1);

	assert_eq!(moves_of(&purses, &operation(9)), vec![]);
}

#[test]
fn the_same_idempotency_key_of_another_scope_is_another_operation() {
	assert_ne!(OperationKey::new(b"one", [1; 32]), OperationKey::new(b"two", [1; 32]));
}
