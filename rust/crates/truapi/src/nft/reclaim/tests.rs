use futures::{StreamExt, executor::block_on};
use tempfile::TempDir;

use super::*;
use crate::{
	durable::{DurableTxId, RegistrationError},
	nft::{
		HandoffRequest, ImportRequest, NftLedger, PurseId, PurseTag, StartImportError,
		TransferProgress, item_keys_from_entropy,
		keys::{ImportedItemKey, ItemIndex, item_secrets_from_entropy},
		testing::{
			TEST_ENTROPY, TEST_INSTALLATION, allocate_held_items, fail_transfer,
			stored_transfer_params, test_ledger,
		},
	},
	store::{Db, open_temporary_core_db},
};

const PURSE: PurseId = PurseId(0);

/// A ledger whose purse handed two keys off under [`tag`].
struct HandedOff {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	keys: Vec<ItemKey>,
}

fn handed_off() -> HandedOff {
	let (dir, db) = open_temporary_core_db();
	let ledger = test_ledger(db.clone());
	block_on(ledger.create_purse("cheques".to_owned(), PurseTag::default())).unwrap();
	let keys = allocate_held_items(&ledger, PURSE, 2);
	let request = HandoffRequest { from: PURSE, keys: keys.iter().copied().collect(), tag: tag() };
	block_on(ledger.hand_off(request, |_| Ok::<_, ()>(()))).unwrap();
	HandedOff { _dir: dir, db, ledger, keys }
}

fn tag() -> HandoffTag {
	HandoffTag(b"cheque".to_vec())
}

fn operation() -> OperationKey {
	OperationKey::new(b"product", [7; 32])
}

fn reclaim(
	ledger: &NftLedger,
	tag: HandoffTag,
) -> Result<(), RegistrationError<StartOperationError>> {
	block_on(ledger.start_reclaims(ReclaimRequest { operation: operation(), tag }))
}

/// Imports the handed-off keys with their secrets, as a deposit of the
/// cheque on this device does.
fn deposit_cheque(ledger: &NftLedger) -> Result<(), StartImportError> {
	let slots: Vec<_> = (0..2)
		.map(|index| ItemSlot {
			purse: PURSE,
			installation: TEST_INSTALLATION,
			index: ItemIndex(index),
		})
		.collect();
	let secrets = item_secrets_from_entropy(&TEST_ENTROPY, &slots).unwrap();
	let keys = secrets.into_iter().map(|secret| ImportedItemKey::from_secret(secret).unwrap());
	let operation = OperationKey::new(b"depositor", [8; 32]);
	let request = ImportRequest { operation, into: PURSE, keys: keys.collect() };
	block_on(ledger.start_imports(request))
}

fn transfers_of(ledger: &NftLedger) -> Vec<TransferProgress> {
	block_on(ledger.observe_operation(&operation()).next()).unwrap().unwrap()
}

/// The sender signs a reclaim with its own derived key, and a refund may
/// bring back another instance than the one handed off, so the move takes
/// whatever the key holds.
#[test]
fn a_reclaim_moves_each_handed_off_key_into_a_fresh_key_of_its_purse() {
	let handed_off = handed_off();

	reclaim(&handed_off.ledger, tag()).unwrap();

	let stored = stored_transfer_params(&handed_off.db);
	let slot =
		|index| ItemSlot { purse: PURSE, installation: TEST_INSTALLATION, index: ItemIndex(index) };
	let fresh = item_keys_from_entropy(&TEST_ENTROPY, &[slot(2), slot(3)]).unwrap();
	let expected: Vec<_> = (0..2)
		.map(|index| TransferParams {
			source: handed_off.keys[index],
			signer: TransferSigner::Derived(slot(index as u32)),
			destination: fresh[index],
			submitted_attempt: None,
			give_up_after: stored[index].give_up_after,
		})
		.collect();
	assert_eq!(stored, expected);
}

/// A revoke repeated after a peer's refund retries the keys whose reclaim
/// failed, and leaves the others alone.
#[test]
fn a_repeated_reclaim_adds_moves_only_for_keys_whose_latest_move_failed() {
	let handed_off = handed_off();
	reclaim(&handed_off.ledger, tag()).unwrap();
	fail_transfer(&handed_off.db, DurableTxId(1));

	reclaim(&handed_off.ledger, tag()).unwrap();

	let consumed: Vec<_> = stored_transfer_params(&handed_off.db)
		.into_iter()
		.map(|params| params.source)
		.collect();
	let [first, second] = handed_off.keys[..] else { panic!("two keys") };
	assert_eq!(consumed, vec![first, second, first]);
}

/// Two revokes in a row must not reclaim the same key twice.
#[test]
fn a_repeated_reclaim_while_every_move_is_live_registers_nothing() {
	let handed_off = handed_off();
	reclaim(&handed_off.ledger, tag()).unwrap();

	reclaim(&handed_off.ledger, tag()).unwrap();

	assert_eq!(stored_transfer_params(&handed_off.db).len(), 2);
}

/// The status of a repeated revoke follows the retry, not the failure
/// before it.
#[test]
fn the_operation_reports_the_retry_in_place_of_the_failed_move() {
	let handed_off = handed_off();
	reclaim(&handed_off.ledger, tag()).unwrap();
	fail_transfer(&handed_off.db, DurableTxId(1));

	reclaim(&handed_off.ledger, tag()).unwrap();

	let states: Vec<_> = transfers_of(&handed_off.ledger)
		.into_iter()
		.map(|transfer| transfer.state)
		.collect();
	assert_eq!(states, vec![TransferState::Waiting, TransferState::Waiting]);
}

/// Only one move of a key can land, so a revoke must not race a deposit of
/// the same cheque on this device.
#[test]
fn a_reclaim_of_keys_a_deposit_moves_is_refused() {
	let handed_off = handed_off();
	deposit_cheque(&handed_off.ledger).unwrap();

	let refused = reclaim(&handed_off.ledger, tag());

	assert!(matches!(
		refused,
		Err(RegistrationError::Hook(StartOperationError::ItemUnavailable(_)))
	));
}

/// A reclaim records what it spends, so a later deposit of the cheque on
/// this device sees the keys are moving.
#[test]
fn a_deposit_of_keys_a_reclaim_moves_is_refused() {
	let handed_off = handed_off();
	reclaim(&handed_off.ledger, tag()).unwrap();

	let refused = deposit_cheque(&handed_off.ledger);

	assert!(matches!(
		refused,
		Err(StartImportError::Registration(RegistrationError::Hook(
			StartOperationError::ItemUnavailable(_)
		)))
	));
}

/// A cheque of another product, or one never created, is not found.
#[test]
fn a_tag_nothing_was_handed_off_under_is_refused() {
	let handed_off = handed_off();

	let refused = reclaim(&handed_off.ledger, HandoffTag(b"other".to_vec()));

	assert!(matches!(refused, Err(RegistrationError::Hook(StartOperationError::TagNotFound))));
}
