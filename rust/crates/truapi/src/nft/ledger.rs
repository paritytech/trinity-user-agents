//! The ledger, Layer 1's entry point over purses and their items.

use std::{num::NonZeroU32, sync::Arc};

use futures::stream::BoxStream;

use super::{
	dao,
	holding::{HeldItem, record_in},
	item::{AllocateItemsError, Allocation, ItemPage, ItemsPage, allocate_in},
	keys::{ItemKey, ItemKeyDeriver},
	purse::{CreatePurseError, PurseId, PurseRecord, PurseTag},
};
use crate::{
	chain::HashAndNumber,
	installation::InstallationIdProvider,
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
}

/// Layer 1's store of purses and their items.
#[derive(Clone)]
pub struct NftLedger {
	db: Db,
	installation: Arc<dyn InstallationIdProvider>,
	keys: Arc<dyn ItemKeyDeriver>,
}

impl NftLedger {
	/// A ledger over `deps`.
	pub fn new(deps: NftLedgerDeps) -> Self {
		let NftLedgerDeps { db, installation, keys } = deps;
		Self { db, installation, keys }
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
			.map_err(AllocateItemsError::Installation)?;
		let allocation = Allocation { purse, installation, count };
		let keys = self.keys.clone();
		self.db.write(move |tx| allocate_in(tx, allocation, keys.as_ref())).await?
	}

	/// One page of `purse`'s Available items, ordered by slot. An item is
	/// Available when its key holds an NFT at the best block; no operation
	/// can hold one yet.
	pub async fn available_items(
		&self,
		purse: PurseId,
		page: ItemPage,
	) -> Result<ItemsPage, DbError> {
		let size = u64::from(page.size.get());
		let offset = u64::from(page.index) * size;
		let read =
			move |conn: &rusqlite::Connection| dao::available_items(conn, purse, size + 1, offset);
		let overread = self.db.read(read).await?;
		Ok(ItemsPage::from_overread(overread, page.size.get() as usize))
	}

	/// Every item key, now and after each allocation.
	pub fn observe_item_keys(&self) -> BoxStream<'static, Result<Vec<ItemKey>, DbError>> {
		dao::observe_item_keys(&self.db)
	}

	/// Stores what each key holds at `at`, in one write. Called by the
	/// tracker.
	pub async fn record_holdings(
		&self,
		at: HashAndNumber,
		holdings: Vec<(ItemKey, Option<HeldItem>)>,
	) -> Result<(), DbError> {
		self.db.write(move |tx| record_in(tx, at, &holdings)).await
	}
}

#[cfg(test)]
mod tests;
