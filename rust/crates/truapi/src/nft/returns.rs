//! Returns: moves that bring the items an import landed back to the keys
//! they came from.

use std::collections::{BTreeMap, BTreeSet};

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
	durable::{DurableTxState, PolicyParams, group_exists},
	store::DbError,
	unix_time::current_unix_secs,
};

/// A return of the items the import `import` landed, registered as
/// `operation`.
pub struct ReturnRequest {
	/// The operation the return forms.
	pub operation: OperationKey,
	/// The import whose items go back.
	pub import: OperationKey,
}

/// The answer to a started return. Each item is named by its key in our purse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartedReturn {
	/// The items that a move was registered for.
	pub started: Vec<ItemKey>,
	/// The items that the import landed but something consumed since. They
	/// stay where they are.
	pub unavailable: Vec<ItemKey>,
}

/// An item that an import landed and that nothing consumed since.
pub struct ReturnableItem {
	/// Its key in our purse.
	pub key: ItemKey,
	/// The slot of that key.
	pub slot: ItemSlot,
	/// The key it came from. The return moves it back there.
	pub origin: ItemKey,
}

/// What a return of an import would move, read without checking that the
/// return may start.
pub struct ReturnItems {
	/// The moves of the import.
	pub import: Vec<DurableTxState>,
	/// The items to move back, in the order the import moved them.
	pub available: Vec<ReturnableItem>,
	/// The items that the import landed but something consumed since.
	pub unavailable: Vec<ItemKey>,
}

impl ReturnItems {
	/// Checks that a return of these items may start as `operation`. Refuses,
	/// in this order, when the import does not exist or is no import, while
	/// any of its moves can still land or fail, when `operation` exists
	/// already, and when no item is available.
	fn validate_start(
		&self,
		conn: &Connection,
		operation: &OperationKey,
	) -> Result<(), StartOperationError> {
		validate_settled_import(&self.import)?;
		validate_return_is_new(conn, operation)?;
		if self.available.is_empty() {
			return Err(StartOperationError::NothingIdle);
		}
		Ok(())
	}

	/// The answer to a return that registered a move for every available item.
	pub fn into_started(self) -> StartedReturn {
		let started = self.available.iter().map(|item| item.key).collect();
		StartedReturn { started, unavailable: self.unavailable }
	}
}

/// Works out, inside the registration write, one move for each available
/// item of `request`, back to the key it came from. Refuses as
/// [`ReturnItems::validate_start`] does.
pub fn plan_returns(
	tx: &Transaction<'_>,
	request: &ReturnRequest,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	let items = validated_return_items(tx, request)?;
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	Ok(items.available.iter().map(|item| return_params(item, give_up_after)).collect())
}

/// Reads the items of `request` and checks that the return may start, as
/// [`ReturnItems::validate_start`] does.
pub fn validated_return_items(
	conn: &Connection,
	request: &ReturnRequest,
) -> Result<ReturnItems, StartOperationError> {
	let items = read_return_items(conn, &request.import)?;
	items.validate_start(conn, &request.operation)?;
	Ok(items)
}

/// Reads what a return of `import` would move. The import's items that
/// landed in a finalized block are available if nothing consumed them, and
/// unavailable otherwise.
pub fn read_return_items(conn: &Connection, import: &OperationKey) -> Result<ReturnItems, DbError> {
	let import = durable::group(conn, &nft_domain(), import.group())?;
	let landed = filter_landed_finalized(progress_of(import.clone())?);
	let available = unconsumed_landed_items(conn, &landed)?;
	let unavailable = consumed_landed_keys(&landed, &available);
	Ok(ReturnItems { import, available, unavailable })
}

/// Checks that every move of `import` is signed with an imported secret,
/// and that each of them finalized or failed. A revoke or a rebalance under
/// the same key is no import.
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
fn filter_landed_finalized(moves: Vec<TransferProgress>) -> Vec<TransferProgress> {
	let finalized = TransferState::Landed { finalized: true };
	moves.into_iter().filter(|transfer| transfer.state == finalized).collect()
}

/// The keys that the moves `landed` minted and that nothing consumed since.
/// Each comes with its slot and the key it came from, in the order of
/// `landed`.
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

/// The keys that the moves `landed` minted and that are not in `available`.
fn consumed_landed_keys(landed: &[TransferProgress], available: &[ReturnableItem]) -> Vec<ItemKey> {
	let available: BTreeSet<ItemKey> = available.iter().map(|item| item.key).collect();
	landed
		.iter()
		.map(|transfer| transfer.minted)
		.filter(|minted| !available.contains(minted))
		.collect()
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

#[cfg(test)]
mod tests;
