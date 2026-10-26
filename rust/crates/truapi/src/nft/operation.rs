//! NFT operations: transfers of items registered together under a caller's
//! idempotency key, and where each transfer stands.

use core::num::NonZeroU32;
use std::collections::BTreeSet;

use parity_scale_codec::Encode;
use rusqlite::Transaction;
use sp_crypto_hashing::blake2_256;
use subxt::utils::H256;

use super::{
	dao::{self, KeyStanding},
	item::{AllocateItemsError, Allocation, allocate_items_in},
	keys::{ItemKey, ItemKeyDeriver},
	purse::PurseId,
	scarcity::{TRANSFER_WAIT_LIMIT_SECS, TransferParams},
};
use crate::{
	durable::{
		DomainId, DurableTxId, DurableTxState, DurableTxStatus, GroupId, PolicyParams, group_exists,
	},
	installation::InstallationId,
	store::DbError,
	unix_time::current_unix_secs,
};

/// The durable domain NFT transfers are registered under.
pub fn nft_domain() -> DomainId {
	DomainId::new("nft")
}

/// Names one operation: an idempotency key within the scope of the caller
/// that chose it. Layer 1 hashes the scope and never reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationKey(GroupId);

impl OperationKey {
	/// The operation `idempotency_key` names within `scope`.
	pub fn new(scope: &[u8], idempotency_key: [u8; 32]) -> Self {
		let digest = blake2_256(&(scope, idempotency_key).encode());
		Self(GroupId::new(format!("nft-op:{}", hex::encode(digest))))
	}

	/// The durable group the operation's transfers belong to.
	pub fn group(&self) -> &GroupId {
		&self.0
	}
}

/// Why an operation was not registered. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum StartOperationError {
	/// The operation names no item.
	#[error("an operation needs at least one item, got none")]
	NoItems,
	/// An operation is already registered under the same key.
	#[error("an operation with this key is already registered")]
	OperationExists,
	/// The item is not in the purse, or holds no NFT.
	#[error("item {} is not held in the purse", hex::encode(.0.0))]
	ItemNotFound(ItemKey),
	/// The item is handed off or spent by a live transaction.
	#[error("item {} is in use", hex::encode(.0.0))]
	ItemUnavailable(ItemKey),
	/// The destination keys could not be allocated.
	#[error(transparent)]
	Allocate(#[from] AllocateItemsError),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// Where the transfers of an operation take their items.
pub enum TransferDestinations {
	/// Fresh keys allocated in this purse, one per item.
	AllocatedIn(PurseId),
	/// This key, outside every purse. Only one item can go there.
	Foreign(ItemKey),
}

/// Transfers of `items`, each from its key in `from`, registered as one operation.
pub struct TransferRequest {
	/// The operation they form.
	pub operation: OperationKey,
	/// The purse every item is spent from.
	pub from: PurseId,
	/// The items, in the order their transfers are registered.
	pub items: BTreeSet<ItemKey>,
	/// Where they go.
	pub destinations: TransferDestinations,
}

/// What allocating destination keys needs: the installation they are
/// derived under and the deriver.
pub struct DestinationKeys<'a> {
	/// The installation new keys are allocated under.
	pub installation: InstallationId,
	/// Derives new keys.
	pub deriver: &'a dyn ItemKeyDeriver,
}

/// Works out, inside the registration write, the params of each transfer of
/// `request`, or refuses: the operation exists, an item is not idle in
/// `from`, or destination keys cannot be allocated.
pub fn plan_transfers(
	tx: &Transaction<'_>,
	request: &TransferRequest,
	keys: DestinationKeys<'_>,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	if group_exists(tx, &nft_domain(), request.operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	let instances = idle_instances(tx, request.from, &request.items)?;
	let (destinations, destination_is_fresh) = match request.destinations {
		TransferDestinations::AllocatedIn(purse) => {
			(allocate_destinations(tx, purse, &instances, keys)?, true)
		},
		TransferDestinations::Foreign(destination) => (vec![destination], false),
	};
	let give_up_after = current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS;
	Ok(instances
		.into_iter()
		.zip(destinations)
		.map(|(instance, destination)| {
			TransferParams { instance, destination, destination_is_fresh, give_up_after }.into()
		})
		.collect())
}

/// One fresh key in `purse` for each of `instances`.
fn allocate_destinations(
	tx: &Transaction<'_>,
	purse: PurseId,
	instances: &[u64],
	keys: DestinationKeys<'_>,
) -> Result<Vec<ItemKey>, StartOperationError> {
	let count = u32::try_from(instances.len()).ok().and_then(NonZeroU32::new);
	let count = count.ok_or(StartOperationError::NoItems)?;
	let allocation = Allocation { purse, installation: keys.installation, count };
	Ok(allocate_items_in(tx, allocation, keys.deriver)??)
}

/// The instance each of `items` holds, in order, when every one is idle in
/// `purse`.
fn idle_instances(
	tx: &Transaction<'_>,
	purse: PurseId,
	items: &BTreeSet<ItemKey>,
) -> Result<Vec<u64>, StartOperationError> {
	let idle_instance = |item: &ItemKey| match dao::key_standing(tx, purse, item)? {
		KeyStanding::Idle { instance } => Ok(instance),
		KeyStanding::Spent => Err(StartOperationError::ItemUnavailable(*item)),
		KeyStanding::NotHeld => Err(StartOperationError::ItemNotFound(*item)),
	};
	items.iter().map(idle_instance).collect()
}

/// Records that the transfer with each id of `ids` spends the item at the same
/// position of `items`.
pub fn record_inputs(
	tx: &Transaction<'_>,
	items: &BTreeSet<ItemKey>,
	ids: &[DurableTxId],
) -> Result<(), StartOperationError> {
	let inputs: Vec<_> = items.iter().copied().zip(ids.iter().copied()).collect();
	Ok(dao::insert_inputs(tx, &inputs)?)
}

/// Where one transfer of an operation stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferProgress {
	/// Key the NFT leaves.
	pub consumed: ItemKey,
	/// Key the NFT goes to.
	pub minted: ItemKey,
	/// Hash of its latest attempt, once one was built.
	pub tx_hash: Option<H256>,
	/// Whether it waits, landed or failed.
	pub state: TransferState,
}

/// Whether a transfer waits, landed or failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
	/// Not built or not landed yet.
	Waiting,
	/// Landed at a block, finalized or not.
	Landed {
		/// Whether that block is finalized.
		finalized: bool,
	},
	/// It can no longer land.
	Failed,
}

impl From<DurableTxStatus> for TransferState {
	fn from(status: DurableTxStatus) -> Self {
		match status {
			DurableTxStatus::PendingSubmission | DurableTxStatus::Pending => Self::Waiting,
			DurableTxStatus::PendingSuccess { .. } => Self::Landed { finalized: false },
			DurableTxStatus::FinalizedSuccess { .. } => Self::Landed { finalized: true },
			DurableTxStatus::Failure(_) => Self::Failed,
		}
	}
}

/// Where each of `transfers` stands. A transfer whose spent key or params cannot be
/// read is an error: the operation's rows are written together.
pub fn progress_of(
	conn: &rusqlite::Connection,
	transfers: Vec<DurableTxState>,
) -> Result<Vec<TransferProgress>, DbError> {
	transfers.into_iter().map(|state| transfer_progress(conn, state)).collect()
}

fn transfer_progress(
	conn: &rusqlite::Connection,
	state: DurableTxState,
) -> Result<TransferProgress, DbError> {
	let unreadable = |what: String| {
		DbError::Connection(format!("transfer {:?} has no readable {what}", state.id))
	};
	let (consumed, _) =
		dao::input_of(conn, state.id)?.ok_or_else(|| unreadable("spent key".into()))?;
	let plan = state.plan.as_ref().ok_or_else(|| unreadable("plan".into()))?;
	let params = TransferParams::try_from(&plan.params)
		.map_err(|error| unreadable(format!("params: {error}")))?;
	let state_of_transfer = TransferState::from(state.status);
	Ok(TransferProgress {
		consumed,
		minted: params.destination,
		tx_hash: state.tx_hash,
		state: state_of_transfer,
	})
}

#[cfg(test)]
mod tests {
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

	fn rebalance_request(operation: OperationKey, items: &[ItemKey]) -> TransferRequest {
		let items = items.iter().copied().collect();
		TransferRequest {
			operation,
			from: FROM,
			items,
			destinations: TransferDestinations::AllocatedIn(TO),
		}
	}

	fn start_operation(
		purses: &Purses,
		request: TransferRequest,
	) -> Result<(), RegistrationError<StartOperationError>> {
		block_on(purses.ledger.start_transfers(request))
	}

	fn available_keys(purses: &Purses, purse: PurseId) -> Vec<ItemKey> {
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

		start_operation(&purses, rebalance_request(operation(1), &purses.held)).unwrap();

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

		start_operation(&purses, rebalance_request(operation(1), &purses.held[..1])).unwrap();

		assert_eq!(available_keys(&purses, FROM), purses.held[1..].to_vec());
	}

	#[test]
	fn an_item_is_available_again_once_its_move_failed() {
		let purses = purses_holding(1);
		start_operation(&purses, rebalance_request(operation(1), &purses.held)).unwrap();

		set_transfer_status(&purses, "FAILURE");

		assert_eq!(available_keys(&purses, FROM), purses.held);
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

		let refused = start_operation(&purses, rebalance_request(operation(1), &purses.held));

		assert_eq!(available_keys(&purses, FROM), vec![]);
		assert!(
			matches!(refused, Err(RegistrationError::Hook(StartOperationError::ItemUnavailable(item))) if item == key)
		);
	}

	#[test]
	fn a_second_registration_under_the_same_key_is_refused_and_writes_nothing() {
		let purses = purses_holding(2);
		start_operation(&purses, rebalance_request(operation(1), &purses.held[..1])).unwrap();

		let again = start_operation(&purses, rebalance_request(operation(1), &purses.held[1..]));

		assert!(matches!(
			again,
			Err(RegistrationError::Hook(StartOperationError::OperationExists))
		));
		assert_eq!(transfers_of(&purses, &operation(1)).len(), 1);
		assert_eq!(item_rows(&purses), 3);
	}

	#[test]
	fn a_new_operation_over_an_item_in_use_is_refused() {
		let purses = purses_holding(1);
		start_operation(&purses, rebalance_request(operation(1), &purses.held)).unwrap();

		let overlapping = start_operation(&purses, rebalance_request(operation(2), &purses.held));

		let in_use = StartOperationError::ItemUnavailable(purses.held[0]);
		assert!(
			matches!(overlapping, Err(RegistrationError::Hook(refusal)) if refusal.to_string() == in_use.to_string())
		);
	}

	#[test]
	fn a_refused_operation_allocates_no_destination_key() {
		let purses = purses_holding(1);
		let not_held = ItemKey([0x55; 32]);

		let refused =
			start_operation(&purses, rebalance_request(operation(1), &[purses.held[0], not_held]));

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
			purses.ledger.start_transfers(rebalance_request(operation(1), &purses.held)),
			purses.ledger.start_transfers(rebalance_request(operation(1), &purses.held)),
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

		start_operation(&purses, request).unwrap();

		let transfers = transfers_of(&purses, &operation(1));
		assert_eq!(
			transfers.iter().map(|progress| progress.minted).collect::<Vec<_>>(),
			vec![FOREIGN]
		);
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

		start_operation(&purses, rebalance_request(operation(1), &purses.held)).unwrap();

		let params = stored_params(&purses);
		let limit = params.give_up_after - before;
		assert!(
			(TRANSFER_WAIT_LIMIT_SECS..=TRANSFER_WAIT_LIMIT_SECS + 5).contains(&limit),
			"{limit}"
		);
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
		start_operation(&rebalanced, rebalance_request(operation(1), &rebalanced.held)).unwrap();
		let sent_out = purses_holding(1);
		let items = sent_out.held.iter().copied().collect();
		let request = TransferRequest {
			operation: operation(1),
			from: FROM,
			items,
			destinations: TransferDestinations::Foreign(FOREIGN),
		};
		start_operation(&sent_out, request).unwrap();

		assert!(stored_params(&rebalanced).destination_is_fresh);
		assert!(!stored_params(&sent_out).destination_is_fresh);
	}
}
