//! Reclaims: moves that bring the NFT of each key handed off under one tag
//! back into a fresh key of the purse it was handed off from.

use std::collections::BTreeSet;

use rusqlite::Transaction;

use super::{
	dao,
	handoff::HandoffTag,
	keys::{ItemKey, ItemSlot},
	operation::{
		DestinationKeys, OperationKey, StartOperationError, TransferState, allocate_destinations,
		latest_per_source, nft_domain, progress_of, record_inputs, validate_not_moving,
	},
	scarcity::{TRANSFER_WAIT_LIMIT_SECS, TransferParams, TransferSigner},
};
use crate::{
	durable,
	durable::{DurableTxId, PolicyParams},
	unix_time::current_unix_secs,
};

/// Reclaims of the keys handed off under `tag`, registered as `operation`.
/// Registering it again adds moves only for keys whose last move failed.
pub struct ReclaimRequest {
	/// The operation every round of reclaims belongs to.
	pub operation: OperationKey,
	/// The handoff whose keys are reclaimed.
	pub tag: HandoffTag,
}

/// Works out, inside the registration write, one move for each key handed
/// off under `request.tag` that has no move in the operation yet or whose
/// latest move failed. Answers no move when every key is moving or moved,
/// and refuses when nothing was handed off under the tag, or when another
/// operation, such as a deposit of the cheque, moves one of those keys.
pub fn plan_reclaims(
	tx: &Transaction<'_>,
	request: &ReclaimRequest,
	keys: DestinationKeys<'_>,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	let handed_off = dao::handoff_keys(tx, &request.tag)?;
	let Some((_, first)) = handed_off.first() else {
		return Err(StartOperationError::TagNotFound);
	};
	let purse = first.purse;
	let sources = keys_to_reclaim(tx, &request.operation, handed_off)?;
	if sources.is_empty() {
		return Ok(Vec::new());
	}
	let source_keys = sources.iter().map(|(key, _)| *key).collect();
	validate_not_moving(tx, &source_keys)?;
	let destinations = allocate_destinations(tx, purse, sources.len(), keys)?;
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	Ok(sources
		.into_iter()
		.zip(destinations)
		.map(|((source, slot), destination)| {
			reclaim_params(source, slot, destination, give_up_after)
		})
		.collect())
}

/// Records that each move of `operation` with an id of `ids`, just
/// registered, spends its key.
pub fn record_reclaim_inputs(
	tx: &Transaction<'_>,
	operation: &OperationKey,
	ids: &[DurableTxId],
) -> Result<(), StartOperationError> {
	let group = durable::group(tx, &nft_domain(), operation.group())?;
	let registered = group.into_iter().filter(|state| ids.contains(&state.id)).collect();
	let sources = progress_of(registered)?.into_iter().map(|transfer| transfer.consumed);
	record_inputs(tx, sources, ids)
}

/// The keys of `handed_off` that `operation` has not moved yet, or whose
/// latest move in it failed.
fn keys_to_reclaim(
	tx: &Transaction<'_>,
	operation: &OperationKey,
	handed_off: Vec<(ItemKey, ItemSlot)>,
) -> Result<Vec<(ItemKey, ItemSlot)>, StartOperationError> {
	let busy = keys_moving_or_moved(tx, operation)?;
	Ok(handed_off.into_iter().filter(|(key, _)| !busy.contains(key)).collect())
}

/// The keys whose latest move in `operation` waits or landed.
fn keys_moving_or_moved(
	tx: &Transaction<'_>,
	operation: &OperationKey,
) -> Result<BTreeSet<ItemKey>, StartOperationError> {
	let group = durable::group(tx, &nft_domain(), operation.group())?;
	let transfers = progress_of(group)?;
	Ok(latest_per_source(transfers)
		.into_iter()
		.filter(|transfer| transfer.state != TransferState::Failed)
		.map(|transfer| transfer.consumed)
		.collect())
}

/// The params of moving whatever `source`, at `slot`, holds to
/// `destination`, signed with the derived key and given up at `give_up_after`.
fn reclaim_params(
	source: ItemKey,
	slot: ItemSlot,
	destination: ItemKey,
	give_up_after: u64,
) -> PolicyParams {
	TransferParams {
		source,
		signer: TransferSigner::Derived(slot),
		destination,
		submitted_attempt: None,
		give_up_after,
	}
	.into()
}

#[cfg(test)]
mod tests;
