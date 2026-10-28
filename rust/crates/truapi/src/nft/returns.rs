//! Returns: moves that bring the items an import landed back to the keys
//! they came from.

use std::collections::BTreeMap;

use rusqlite::Transaction;

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
	durable::{DurableTxId, PolicyParams, group_exists},
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

/// Works out, inside the registration write, one move for each item the
/// import landed that nothing consumed, back to the key it came from.
/// Refuses while a move of the import can still change, when the return
/// exists, and when no landed item is unconsumed.
pub fn plan_returns(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	let landed = finalized_import_moves(tx, &request.import)?;
	if group_exists(tx, &nft_domain(), request.operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	let minted: Vec<ItemKey> = landed.iter().map(|transfer| transfer.minted).collect();
	let unconsumed: BTreeMap<ItemKey, ItemSlot> =
		dao::unconsumed_keys(tx, &minted)?.into_iter().collect();
	if unconsumed.is_empty() {
		return Err(StartOperationError::NothingIdle);
	}
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	Ok(landed
		.iter()
		.filter_map(|transfer| {
			let slot = unconsumed.get(&transfer.minted)?;
			Some(return_params(transfer, *slot, give_up_after))
		})
		.collect())
}

/// The moves of `import` that finalized. Refuses when no such import exists
/// or while any of its moves can still land or fail.
fn finalized_import_moves(
	tx: &Transaction<'_>,
	import: &OperationKey,
) -> Result<Vec<TransferProgress>, StartOperationError> {
	let group = durable::group(tx, &nft_domain(), import.group())?;
	let moves = progress_of(group)?;
	if moves.is_empty() {
		return Err(StartOperationError::ImportNotFound);
	}
	let finalized = TransferState::Landed { finalized: true };
	let is_final = |transfer: &TransferProgress| {
		matches!(transfer.state, TransferState::Failed | TransferState::Landed { finalized: true })
	};
	if !moves.iter().all(is_final) {
		return Err(StartOperationError::ImportUnsettled);
	}
	Ok(moves.into_iter().filter(|transfer| transfer.state == finalized).collect())
}

/// The params of moving whatever the key `landed` minted, at `slot`, holds
/// back to the key it came from, given up at `give_up_after`.
fn return_params(landed: &TransferProgress, slot: ItemSlot, give_up_after: u64) -> PolicyParams {
	TransferParams {
		source: landed.minted,
		signer: TransferSigner::Derived(slot),
		destination: landed.consumed,
		submitted_attempt: None,
		give_up_after,
	}
	.into()
}

/// Records, in the registration write, that each move of the return with
/// `ids` spends its item, and answers which items it started and which
/// stay.
pub fn record_return(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
	ids: &[DurableTxId],
) -> Result<StartedReturn, StartOperationError> {
	let started = started_return(tx, request)?;
	let inputs: Vec<_> = started.started.iter().copied().zip(ids.iter().copied()).collect();
	dao::insert_inputs(tx, &inputs)?;
	Ok(started)
}

/// The items the return's registered moves take, in registration order,
/// and the finalized items of the import they leave out.
fn started_return(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
) -> Result<StartedReturn, StartOperationError> {
	let group = durable::group(tx, &nft_domain(), request.operation.group())?;
	let started: Vec<ItemKey> =
		progress_of(group)?.into_iter().map(|transfer| transfer.consumed).collect();
	let landed = finalized_import_moves(tx, &request.import)?;
	let unavailable = landed
		.into_iter()
		.map(|transfer| transfer.minted)
		.filter(|minted| !started.contains(minted))
		.collect();
	Ok(StartedReturn { started, unavailable })
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

		assert_eq!(started.unavailable, vec![moved]);
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

	/// A refund is not repeated under the same key.
	#[test]
	fn a_second_return_under_the_same_key_is_refused() {
		let deposited = deposited();
		start_return(&deposited, refund(2)).unwrap();

		let refused = refused_with(start_return(&deposited, refund(2)));

		assert!(matches!(refused, StartOperationError::OperationExists));
	}

	/// Returned items are in use, so nothing else spends them, and are free
	/// again once their moves fail, so a refund can be retried then.
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
