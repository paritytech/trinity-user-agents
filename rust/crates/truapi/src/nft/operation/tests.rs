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

fn rebalance(operation: OperationKey, items: &[ItemKey]) -> TransferRequest {
	let items = items.iter().copied().collect();
	TransferRequest {
		operation,
		from: FROM,
		items,
		destinations: TransferDestinations::AllocatedIn(TO),
	}
}

fn start(
	purses: &Purses,
	request: TransferRequest,
) -> Result<(), RegistrationError<StartOperationError>> {
	block_on(purses.ledger.start_transfers(request))
}

fn available(purses: &Purses, purse: PurseId) -> Vec<ItemKey> {
	let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
	block_on(purses.ledger.available_items(purse, page)).unwrap().items
}

fn transfers_of(purses: &Purses, operation: &OperationKey) -> Vec<TransferProgress> {
	block_on(purses.ledger.observe_operation(operation).next()).unwrap().unwrap()
}

fn item_rows(purses: &Purses) -> i64 {
	let count = |conn: &rusqlite::Connection| {
		Ok(conn.query_row("SELECT count(*) FROM nft_item", [], |row| row.get(0))?)
	};
	block_on(purses.db.read(count)).unwrap()
}

fn set_transfer_status(purses: &Purses, status: &str) {
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

	let transfers = transfers_of(&purses, &operation(1));
	let consumed: BTreeSet<_> = transfers.iter().map(|progress| progress.consumed).collect();
	let minted: Vec<_> = transfers.iter().map(|progress| progress.minted).collect();
	assert_eq!(consumed, purses.held.iter().copied().collect());
	assert!(transfers.iter().all(|progress| progress.state == TransferState::Waiting));
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

	set_transfer_status(&purses, "FAILURE");

	assert_eq!(available(&purses, FROM), purses.held);
	assert_eq!(transfers_of(&purses, &operation(1))[0].state, TransferState::Failed);
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
	assert_eq!(transfers_of(&purses, &operation(1)).len(), 1);
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
	assert_eq!(transfers_of(&purses, &operation(1)), vec![]);
}

#[test]
fn two_identical_calls_at_once_register_one_operation() {
	let purses = purses_holding(1);

	let (first, second) = block_on(futures::future::join(
		purses.ledger.start_transfers(rebalance(operation(1), &purses.held)),
		purses.ledger.start_transfers(rebalance(operation(1), &purses.held)),
	));

	let refused = [&first, &second].iter().filter(|result| result.is_err()).count();
	assert_eq!(refused, 1);
	assert_eq!(transfers_of(&purses, &operation(1)).len(), 1);
}

#[test]
fn a_transfer_moves_one_item_to_the_foreign_key_without_allocating() {
	let purses = purses_holding(1);
	let request = TransferRequest {
		operation: operation(1),
		from: FROM,
		items: purses.held.iter().copied().collect(),
		destinations: TransferDestinations::Foreign(FOREIGN),
	};

	start(&purses, request).unwrap();

	let transfers = transfers_of(&purses, &operation(1));
	assert_eq!(transfers.iter().map(|progress| progress.minted).collect::<Vec<_>>(), vec![FOREIGN]);
	assert_eq!(item_rows(&purses), 1);
}

#[test]
fn an_operation_never_registered_has_no_moves() {
	let purses = purses_holding(1);

	assert_eq!(transfers_of(&purses, &operation(9)), vec![]);
}

#[test]
fn the_same_idempotency_key_of_another_scope_is_another_operation() {
	assert_ne!(OperationKey::new(b"one", [1; 32]), OperationKey::new(b"two", [1; 32]));
}

/// A transfer stops waiting a fixed time after it is registered, whatever
/// happens to the host meanwhile.
#[test]
fn each_move_is_registered_with_its_wait_limit() {
	let purses = purses_holding(1);
	let before = crate::unix_time::current_unix_secs();

	start(&purses, rebalance(operation(1), &purses.held)).unwrap();

	let params = stored_params(&purses);
	let limit = params.give_up_after - before;
	assert!((TRANSFER_WAIT_LIMIT_SECS..=TRANSFER_WAIT_LIMIT_SECS + 5).contains(&limit), "{limit}");
}

/// The one transfer's params, as the policy reads them back.
fn stored_params(purses: &Purses) -> TransferParams {
	let stored = |conn: &rusqlite::Connection| {
		Ok(conn.query_row("SELECT policy_params FROM durable_tx", [], |row| {
			row.get::<_, Vec<u8>>(0)
		})?)
	};
	let params = block_on(purses.db.read(stored)).unwrap();
	TransferParams::try_from(&crate::durable::PolicyParams(params)).unwrap()
}

/// Only a key allocated for the transfer proves, by holding the instance,
/// that the transfer landed; an outside key may have held it before.
#[test]
fn only_a_rebalance_marks_its_destination_fresh() {
	let rebalanced = purses_holding(1);
	start(&rebalanced, rebalance(operation(1), &rebalanced.held)).unwrap();
	let sent_out = purses_holding(1);
	let items = sent_out.held.iter().copied().collect();
	let request = TransferRequest {
		operation: operation(1),
		from: FROM,
		items,
		destinations: TransferDestinations::Foreign(FOREIGN),
	};
	start(&sent_out, request).unwrap();

	assert!(stored_params(&rebalanced).destination_is_fresh);
	assert!(!stored_params(&sent_out).destination_is_fresh);
}
