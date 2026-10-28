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
		latest_per_source, nft_domain, progress_of,
	},
	scarcity::{TRANSFER_WAIT_LIMIT_SECS, TransferParams, TransferSigner},
};
use crate::{durable, durable::PolicyParams, unix_time::current_unix_secs};

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
/// and refuses when nothing was handed off under the tag.
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
	let busy = keys_moving_or_moved(tx, &request.operation)?;
	let sources: Vec<_> = handed_off.into_iter().filter(|(key, _)| !busy.contains(key)).collect();
	if sources.is_empty() {
		return Ok(Vec::new());
	}
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

/// The keys whose latest move in `operation` waits or landed.
fn keys_moving_or_moved(
	tx: &Transaction<'_>,
	operation: &OperationKey,
) -> Result<BTreeSet<ItemKey>, StartOperationError> {
	let group = durable::group(tx, &nft_domain(), operation.group())?;
	Ok(latest_per_source(progress_of(group)?)
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
mod tests {
	use futures::{StreamExt, executor::block_on};
	use tempfile::TempDir;

	use super::*;
	use crate::{
		durable::{DurableTxId, RegistrationError},
		nft::{
			HandoffRequest, NftLedger, PurseId, PurseTag, TransferProgress, item_keys_from_entropy,
			keys::ItemIndex,
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
		let request =
			HandoffRequest { from: PURSE, keys: keys.iter().copied().collect(), tag: tag() };
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
		let slot = |index| ItemSlot {
			purse: PURSE,
			installation: TEST_INSTALLATION,
			index: ItemIndex(index),
		};
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

	/// A cheque of another product, or one never created, is not found.
	#[test]
	fn a_tag_nothing_was_handed_off_under_is_refused() {
		let handed_off = handed_off();

		let refused = reclaim(&handed_off.ledger, HandoffTag(b"other".to_vec()));

		assert!(matches!(refused, Err(RegistrationError::Hook(StartOperationError::TagNotFound))));
	}
}
