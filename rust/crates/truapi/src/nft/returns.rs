//! Returns: moves that bring the items an import landed back to the keys
//! they came from.

use std::collections::BTreeMap;

use rusqlite::{Connection, Transaction};

use super::{
	dao,
	keys::{ItemKey, ItemSlot},
	operation::{
		OperationKey, StartOperationError, TransferProgress, TransferState, nft_domain, progress_of,
	},
	scarcity::{TRANSFER_WAIT_LIMIT_SECS, TransferParams, TransferSigner},
};
use crate::{
	durable,
	durable::{DurableTxId, DurableTxState, PolicyParams, group_exists},
	store::DbError,
	unix_time::current_unix_secs,
};

/// A return of the items the import `import` landed, registered as
/// `operation`.
#[derive(Clone)]
pub struct ReturnRequest {
	/// The operation the return forms.
	pub operation: OperationKey,
	/// The import whose items go back.
	pub import: OperationKey,
}

/// The items of a return, by their keys in our purse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartedReturn {
	/// The items a move was registered for.
	pub started: Vec<ItemKey>,
	/// The items the import landed that are consumed already, so stay.
	pub unavailable: Vec<ItemKey>,
}

/// An item an import landed that nothing consumed since.
pub struct ReturnableItem {
	/// Its key in our purse.
	pub key: ItemKey,
	/// Where that key sits.
	pub slot: ItemSlot,
	/// The key it came from, which it goes back to.
	pub origin: ItemKey,
}

/// Works out, inside the registration write, one move for each of the
/// [`returnable_items`] of `request`, back to the key it came from.
pub fn plan_returns(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	Ok(returnable_items(tx, request)?
		.iter()
		.map(|item| return_params(item, give_up_after))
		.collect())
}

/// The items the import of `request` landed that nothing consumed, in the
/// import's order. Refuses, in this order, when no import exists under that
/// key, while any of its moves can still land or fail, when the return
/// exists, and when no landed item is unconsumed.
pub fn returnable_items(
	conn: &Connection,
	request: &ReturnRequest,
) -> Result<Vec<ReturnableItem>, StartOperationError> {
	let import = durable::group(conn, &nft_domain(), request.import.group())?;
	validate_settled_import(&import)?;
	validate_return_is_new(conn, &request.operation)?;
	let moves = progress_of(import)?;
	let items = unconsumed_landed_items(conn, &finalized_moves(moves))?;
	if items.is_empty() {
		return Err(StartOperationError::NothingIdle);
	}
	Ok(items)
}

/// Checks that `import` is an import, every move of it signed with an
/// imported secret, and that each of its moves finalized or failed. Any
/// other operation under the key, such as a revoke or a rebalance, is no
/// import.
fn validate_settled_import(import: &[DurableTxState]) -> Result<(), StartOperationError> {
	if import.is_empty() || !import.iter().all(is_import_move) {
		return Err(StartOperationError::ImportNotFound);
	}
	if !import.iter().all(|state| state.status.is_final()) {
		return Err(StartOperationError::ImportUnsettled);
	}
	Ok(())
}

/// Whether `state` is a move signed with an imported secret.
fn is_import_move(state: &DurableTxState) -> bool {
	let params = state.plan.as_ref().and_then(|plan| TransferParams::try_from(&plan.params).ok());
	params.is_some_and(|params| params.signer == TransferSigner::Imported)
}

/// Checks that nothing is registered under `operation` yet.
fn validate_return_is_new(
	conn: &Connection,
	operation: &OperationKey,
) -> Result<(), StartOperationError> {
	if group_exists(conn, &nft_domain(), operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	Ok(())
}

/// The moves of `moves` that landed in a finalized block.
fn finalized_moves(moves: Vec<TransferProgress>) -> Vec<TransferProgress> {
	let finalized = TransferState::Landed { finalized: true };
	moves.into_iter().filter(|transfer| transfer.state == finalized).collect()
}

/// The keys `landed` minted that nothing consumed since, each with its slot
/// and the key it came from, in the order of `landed`.
fn unconsumed_landed_items(
	conn: &Connection,
	landed: &[TransferProgress],
) -> Result<Vec<ReturnableItem>, DbError> {
	let minted: Vec<ItemKey> = landed.iter().map(|transfer| transfer.minted).collect();
	let unconsumed: BTreeMap<ItemKey, ItemSlot> =
		dao::unconsumed_keys(conn, &minted)?.into_iter().collect();
	Ok(landed
		.iter()
		.filter_map(|transfer| {
			let slot = *unconsumed.get(&transfer.minted)?;
			Some(ReturnableItem { key: transfer.minted, slot, origin: transfer.consumed })
		})
		.collect())
}

/// The params of moving whatever the key of `item` holds back to the key it
/// came from, given up at `give_up_after`.
fn return_params(item: &ReturnableItem, give_up_after: u64) -> PolicyParams {
	TransferParams {
		source: item.key,
		signer: TransferSigner::Derived(item.slot),
		destination: item.origin,
		submitted_attempt: None,
		give_up_after,
	}
	.into()
}

/// The items the return's registered moves take, in registration order,
/// and the finalized items of the import they leave out.
pub fn started_return(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
) -> Result<StartedReturn, DbError> {
	let own = durable::group(tx, &nft_domain(), request.operation.group())?;
	let started: Vec<ItemKey> =
		progress_of(own)?.into_iter().map(|transfer| transfer.consumed).collect();
	let import = durable::group(tx, &nft_domain(), request.import.group())?;
	let import_moves = progress_of(import)?;
	let unavailable = finalized_moves(import_moves)
		.into_iter()
		.map(|transfer| transfer.minted)
		.filter(|minted| !started.contains(minted))
		.collect();
	Ok(StartedReturn { started, unavailable })
}

/// Records that the move with each id of `ids` spends the item at the same
/// position of `started`.
pub fn record_return_inputs(
	tx: &Transaction<'_>,
	started: &[ItemKey],
	ids: &[DurableTxId],
) -> Result<(), DbError> {
	let inputs: Vec<_> = started.iter().copied().zip(ids.iter().copied()).collect();
	dao::insert_inputs(tx, &inputs)
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;
	use tempfile::TempDir;

	use super::*;
	use crate::{
		durable::RegistrationError,
		nft::{
			HandoffRequest, HandoffTag, HeldItem, ImportRequest, NftLedger, PurseId, PurseTag,
			TransferDestinations, TransferRequest,
			keys::ItemIndex,
			testing::{
				TEST_INSTALLATION, fail_transfer, finalize_transfer, imported_test_key,
				stored_transfer_params, test_ledger,
			},
		},
		store::{Db, open_temporary_core_db},
	};

	const PURSE: PurseId = PurseId(0);

	/// A ledger whose purse imported two cheque keys, with every move still
	/// waiting.
	struct Imported {
		_dir: TempDir,
		db: Db,
		ledger: NftLedger,
		cheque_keys: Vec<ItemKey>,
		minted: Vec<ItemKey>,
	}

	fn imported() -> Imported {
		let (dir, db) = open_temporary_core_db();
		let ledger = test_ledger(db.clone());
		block_on(ledger.create_purse("tickets".to_owned(), PurseTag::default())).unwrap();
		let keys: Vec<_> = (0..2).map(imported_test_key).collect();
		let cheque_keys = keys.iter().map(|key| key.key()).collect();
		let request = ImportRequest { operation: import(), into: PURSE, keys };
		block_on(ledger.start_imports(request)).unwrap();
		let minted = stored_transfer_params(&db).iter().map(|params| params.destination).collect();
		Imported { _dir: dir, db, ledger, cheque_keys, minted }
	}

	/// [`imported`] with both moves finalized.
	fn deposited() -> Imported {
		let imported = imported();
		finalize_transfer(&imported.db, DurableTxId(1));
		finalize_transfer(&imported.db, DurableTxId(2));
		imported
	}

	fn import() -> OperationKey {
		OperationKey::new(b"product", [1; 32])
	}

	fn refund(byte: u8) -> OperationKey {
		OperationKey::new(b"product", [byte; 32])
	}

	fn start_return(
		imported: &Imported,
		operation: OperationKey,
	) -> Result<StartedReturn, RegistrationError<StartOperationError>> {
		let request = ReturnRequest { operation, import: import() };
		block_on(imported.ledger.start_returns(request))
	}

	fn hold(imported: &Imported, key: ItemKey) {
		let held = HeldItem { collection: 1, item: 1, instance: 100 };
		block_on(imported.ledger.record_holdings(vec![(key, Some(held))])).unwrap();
	}

	fn hand_off(imported: &Imported, key: ItemKey) {
		hold(imported, key);
		let tag = HandoffTag(b"next cheque".to_vec());
		let request = HandoffRequest { from: PURSE, keys: [key].into(), tag };
		block_on(imported.ledger.hand_off(request, |_| Ok::<_, ()>(()))).unwrap();
	}

	fn refused_with(
		result: Result<StartedReturn, RegistrationError<StartOperationError>>,
	) -> StartOperationError {
		match result {
			Err(RegistrationError::Hook(refusal)) => refusal,
			other => panic!("expected a refusal, got {other:?}"),
		}
	}

	/// The minted keys are ours and derived, so the return signs with them,
	/// and the cheque keys get their items back.
	#[test]
	fn a_return_moves_each_landed_item_back_to_its_cheque_key() {
		let deposited = deposited();

		let started = start_return(&deposited, refund(2)).unwrap();

		assert_eq!(
			started,
			StartedReturn { started: deposited.minted.clone(), unavailable: vec![] }
		);
		let returns = &stored_transfer_params(&deposited.db)[2..];
		let expected: Vec<_> = (0..2)
			.map(|index| TransferParams {
				source: deposited.minted[index],
				signer: TransferSigner::Derived(ItemSlot {
					purse: PURSE,
					installation: TEST_INSTALLATION,
					index: ItemIndex(index as u32),
				}),
				destination: deposited.cheque_keys[index],
				submitted_attempt: None,
				give_up_after: returns[index].give_up_after,
			})
			.collect();
		assert_eq!(returns, expected);
	}

	/// The depositor may have spent an item before refunding; the refund
	/// returns the rest and says which stay.
	#[test]
	fn a_handed_off_item_is_left_out_and_reported_unavailable() {
		let deposited = deposited();
		hand_off(&deposited, deposited.minted[1]);

		let started = start_return(&deposited, refund(2)).unwrap();

		let [first, second] = deposited.minted[..] else { panic!("two items") };
		assert_eq!(started, StartedReturn { started: vec![first], unavailable: vec![second] });
	}

	/// A key emptied by a move that finalized has no holding left, so only
	/// the move's record shows it is gone.
	#[test]
	fn an_item_moved_out_by_a_finalized_transfer_is_reported_unavailable() {
		let deposited = deposited();
		let moved = deposited.minted[1];
		hold(&deposited, moved);
		let transfer = TransferRequest {
			operation: refund(9),
			from: PURSE,
			items: [moved].into(),
			destinations: TransferDestinations::Foreign(ItemKey([0xee; 32])),
		};
		block_on(deposited.ledger.start_transfers(transfer)).unwrap();
		finalize_transfer(&deposited.db, DurableTxId(3));

		let started = start_return(&deposited, refund(2)).unwrap();

		let expected =
			StartedReturn { started: vec![deposited.minted[0]], unavailable: vec![moved] };
		assert_eq!(started, expected);
	}

	/// An item whose deposit move failed never arrived, so there is nothing
	/// to return for it.
	#[test]
	fn an_item_whose_import_failed_is_neither_returned_nor_reported() {
		let imported = imported();
		finalize_transfer(&imported.db, DurableTxId(1));
		fail_transfer(&imported.db, DurableTxId(2));

		let started = start_return(&imported, refund(2)).unwrap();

		assert_eq!(
			started,
			StartedReturn { started: vec![imported.minted[0]], unavailable: vec![] }
		);
	}

	/// A refund must not race the deposit's own moves.
	#[test]
	fn a_return_of_an_unsettled_import_is_refused_and_registers_nothing() {
		let imported = imported();
		finalize_transfer(&imported.db, DurableTxId(1));

		let refused = refused_with(start_return(&imported, refund(2)));

		assert!(matches!(refused, StartOperationError::ImportUnsettled));
		assert_eq!(stored_transfer_params(&imported.db).len(), 2);
	}

	#[test]
	fn a_return_of_an_unknown_import_is_refused() {
		let deposited = deposited();
		let request = ReturnRequest { operation: refund(2), import: refund(8) };

		let refused = refused_with(block_on(deposited.ledger.start_returns(request)));

		assert!(matches!(refused, StartOperationError::ImportNotFound));
	}

	/// A product's revoke, rebalance or transfer shares the key space of its
	/// deposits, and returning one would undo it, as a refund of a revoke
	/// would put items back at keys the recipient holds.
	#[test]
	fn a_return_of_an_operation_that_is_no_import_is_refused() {
		let deposited = deposited();
		let moved = deposited.minted[1];
		hold(&deposited, moved);
		let transfer = TransferRequest {
			operation: refund(9),
			from: PURSE,
			items: [moved].into(),
			destinations: TransferDestinations::Foreign(ItemKey([0xee; 32])),
		};
		block_on(deposited.ledger.start_transfers(transfer)).unwrap();
		finalize_transfer(&deposited.db, DurableTxId(3));
		let request = ReturnRequest { operation: refund(2), import: refund(9) };

		let refused = refused_with(block_on(deposited.ledger.start_returns(request)));

		assert!(matches!(refused, StartOperationError::ImportNotFound));
	}

	/// A refund is not repeated under the same key.
	#[test]
	fn a_second_return_under_the_same_key_is_refused() {
		let deposited = deposited();
		start_return(&deposited, refund(2)).unwrap();

		let refused = refused_with(start_return(&deposited, refund(2)));

		assert!(matches!(refused, StartOperationError::OperationExists));
	}

	/// Returned items are in use, so nothing else spends them, and are free
	/// again once their moves fail, so the depositor can spend them again.
	#[test]
	fn returned_items_stay_in_use_until_their_moves_fail() {
		let deposited = deposited();
		start_return(&deposited, refund(2)).unwrap();

		let while_live = refused_with(start_return(&deposited, refund(3)));
		fail_transfer(&deposited.db, DurableTxId(3));
		fail_transfer(&deposited.db, DurableTxId(4));
		let after_failure = start_return(&deposited, refund(3)).unwrap();

		assert!(matches!(while_live, StartOperationError::NothingIdle));
		assert_eq!(after_failure.started, deposited.minted);
	}
}
