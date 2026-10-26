//! The ledger, Layer 1's entry point over purses and their items.

use core::num::NonZeroU32;
use std::sync::Arc;

use futures::{StreamExt, stream::BoxStream};

use super::{
	dao,
	handoff::{HandOffError, HandoffRequest, handoff_slots, record_handoff},
	holding::{HeldItem, record_holdings},
	item::{AllocateItemsError, Allocation, ItemPage, PageRequest, allocate_items_in},
	keys::{ItemKey, ItemKeyDeriver, ItemSecret, ItemSecrets},
	operation::{
		DestinationKeys, OperationKey, StartOperationError, TransferProgress, TransferRequest,
		nft_domain, plan_transfers, progress_of, record_inputs,
	},
	purse::{CreatePurseError, PurseId, PurseRecord, PurseTag},
	scarcity::TRANSFER_POLICY,
};
use crate::{
	durable::{DurableRequest, DurableTxEngine, PolicyId, RegistrationError},
	installation::{InstallationId, InstallationIdProvider},
	store::{Db, DbError},
};

/// What the ledger runs on.
pub struct NftLedgerDeps {
	/// The core database.
	pub db: Db,
	/// The installation items are allocated under.
	pub installation: Arc<dyn InstallationIdProvider>,
	/// Derives the keys of new items.
	pub keys: Arc<dyn ItemKeyDeriver>,
	/// Registers and follows the transfers of items.
	pub durable: Arc<DurableTxEngine>,
	/// Exports the secrets of items handed off.
	pub secrets: Arc<dyn ItemSecrets>,
}

/// Layer 1's store of purses and their items.
#[derive(Clone)]
pub struct NftLedger {
	db: Db,
	installation: Arc<dyn InstallationIdProvider>,
	keys: Arc<dyn ItemKeyDeriver>,
	durable: Arc<DurableTxEngine>,
	secrets: Arc<dyn ItemSecrets>,
}

impl NftLedger {
	/// A ledger over `deps`.
	pub fn new(deps: NftLedgerDeps) -> Self {
		let NftLedgerDeps { db, installation, keys, durable, secrets } = deps;
		Self { db, installation, keys, durable, secrets }
	}

	/// Records a purse under the next free id, one past the largest below
	/// [`PurseId::MAIN`], in one write.
	pub async fn create_purse(
		&self,
		name: String,
		tag: PurseTag,
	) -> Result<PurseId, CreatePurseError> {
		let created = self.db.write(move |tx| dao::insert_purse(tx, &name, &tag)).await?;
		created.ok_or(CreatePurseError::IdsExhausted)
	}

	/// Every purse carrying `tag`, by id.
	pub async fn purses_tagged(&self, tag: PurseTag) -> Result<Vec<PurseRecord>, DbError> {
		self.db.read(move |conn| dao::purses_tagged(conn, &tag)).await
	}

	/// The purse with `id`, if any.
	pub async fn purse(&self, id: PurseId) -> Result<Option<PurseRecord>, DbError> {
		self.db.read(move |conn| dao::purse(conn, id)).await
	}

	/// Derives and records `count` keys at the next indices of `purse` under
	/// the current installation, in one write, so concurrent calls never
	/// share an index. The installation is asked before the write starts.
	pub async fn allocate_items(
		&self,
		purse: PurseId,
		count: NonZeroU32,
	) -> Result<Vec<ItemKey>, AllocateItemsError> {
		let installation = self
			.installation
			.current_installation()
			.await
			.map_err(AllocateItemsError::InstallationUnavailable)?;
		let allocation = Allocation { purse, installation, count };
		let keys = self.keys.clone();
		self.db
			.write(move |tx| allocate_items_in(tx, allocation, keys.as_ref()))
			.await?
	}

	/// One page of `purse`'s Available items, ordered by slot. An item is
	/// Available when its key holds an NFT at the best block; no operation
	/// can hold one yet.
	pub async fn available_items(
		&self,
		purse: PurseId,
		page: PageRequest,
	) -> Result<ItemPage, DbError> {
		let size = u64::from(page.size.get());
		let offset = u64::from(page.index) * size;
		let read_page =
			move |conn: &rusqlite::Connection| dao::available_items(conn, purse, size + 1, offset);
		let overread = self.db.read(read_page).await?;
		let page_size = page.size.get() as usize;
		Ok(ItemPage::from_overread(overread, page_size))
	}

	/// Every item key, now and after each allocation.
	pub fn observe_item_keys(&self) -> BoxStream<'static, Result<Vec<ItemKey>, DbError>> {
		dao::observe_item_keys(&self.db)
	}

	/// Registers the transfers of `request` as one operation, in one write that
	/// checks every item is idle in its purse, allocates destination keys
	/// and records what each transfer spends. Returns once they are registered.
	pub async fn start_transfers(
		&self,
		request: TransferRequest,
	) -> Result<(), RegistrationError<StartOperationError>> {
		if request.items.is_empty() {
			return Err(RegistrationError::Hook(StartOperationError::NoItems));
		}
		let installation = self.current_installation().await?;
		let items = request.items.clone();
		let planned = self.planned_transfers(request, installation);
		let record =
			move |tx: &rusqlite::Transaction<'_>, ids: &[_]| record_inputs(tx, &items, ids);
		self.durable.execute_with(planned, record).await.map(|_| ())
	}

	/// The installation new keys are allocated under now.
	async fn current_installation(
		&self,
	) -> Result<InstallationId, RegistrationError<StartOperationError>> {
		self.installation.current_installation().await.map_err(|error| {
			RegistrationError::Hook(AllocateItemsError::InstallationUnavailable(error).into())
		})
	}

	/// `request` as a durable request whose transfers are planned in its write.
	fn planned_transfers(
		&self,
		request: TransferRequest,
		installation: InstallationId,
	) -> DurableRequest<StartOperationError> {
		let group = Some(request.operation.group().clone());
		let deriver = self.keys.clone();
		let plan = move |tx: &rusqlite::Transaction<'_>| {
			let keys = DestinationKeys { installation, deriver: deriver.as_ref() };
			plan_transfers(tx, &request, keys)
		};
		DurableRequest::scheduled_in(nft_domain(), group, PolicyId::new(TRANSFER_POLICY), plan)
	}

	/// Every transfer of `operation`, in registration order, now and after every
	/// change. Empty while no such operation exists.
	pub fn observe_operation(
		&self,
		operation: &OperationKey,
	) -> BoxStream<'static, Result<Vec<TransferProgress>, DbError>> {
		self.durable
			.observe_group(&nft_domain(), operation.group())
			.map(|transfers| transfers.and_then(progress_of))
			.boxed()
	}

	/// Hands the keys of `request` off: exports their secrets, lets `seal` turn
	/// them into the caller's result, then records the keys as handed off under
	/// the tag in one write that checks them again. The keys are spent from
	/// then on. Returns what `seal` returned; on any refusal nothing is written.
	pub async fn hand_off<T, E, Seal>(
		&self,
		request: HandoffRequest,
		seal: Seal,
	) -> Result<T, HandOffError<E>>
	where
		Seal: FnOnce(&[ItemSecret]) -> Result<T, E>,
		E: Send + 'static,
	{
		if request.keys.is_empty() {
			return Err(HandOffError::NoItems);
		}
		let secrets = self.secrets_of(&request).await?;
		let sealed = seal(&secrets).map_err(HandOffError::Seal)?;
		drop(secrets);
		self.db
			.write_unless_refused(move |tx| Ok(record_handoff(tx, &request)))
			.await??;
		Ok(sealed)
	}

	/// The secret of each key of `request`, once the read checks pass.
	async fn secrets_of<E: Send + 'static>(
		&self,
		request: &HandoffRequest,
	) -> Result<Vec<ItemSecret>, HandOffError<E>> {
		let read_request = HandoffRequest {
			from: request.from,
			keys: request.keys.clone(),
			tag: request.tag.clone(),
		};
		let slots = self.db.read(move |conn| Ok(handoff_slots(conn, &read_request))).await??;
		self.secrets.item_secrets(&slots).map_err(HandOffError::Secrets)
	}

	/// The purse `item` belongs to, if it is one of ours.
	pub async fn purse_of(&self, item: ItemKey) -> Result<Option<PurseId>, DbError> {
		self.db.read(move |conn| dao::purse_of(conn, &item)).await
	}

	/// Stores what each key holds, in one write. Called by the tracker.
	pub async fn record_holdings(
		&self,
		holdings: Vec<(ItemKey, Option<HeldItem>)>,
	) -> Result<(), DbError> {
		self.db.write(move |tx| record_holdings(tx, &holdings)).await
	}
}

#[cfg(test)]
mod tests;
