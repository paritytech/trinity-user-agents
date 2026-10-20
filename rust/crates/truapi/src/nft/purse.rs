//! Purse records and the ledger that stores them.

use super::dao;
use crate::store::{Db, DbError};

/// A purse's id. Ids are allocated in order from 0 and never reused, because
/// each one becomes part of its items' key derivation and recovery walks them
/// in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PurseId(pub u32);

impl PurseId {
	/// The purse every user has. No product owns it.
	pub const MAIN: Self = Self(u32::MAX);
}

/// Bytes the caller attaches to a purse when it creates it, written in the
/// same transaction. A tag lets the layer above group purses and attach its
/// own metadata to them; ownership is one such use, with the owner in the
/// tag. Layer 1 stores and matches tags but never reads their contents. The
/// empty tag marks a purse no caller has claimed: the main purse, and purses
/// recovered without metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PurseTag(pub Vec<u8>);

/// A stored purse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurseRecord {
	/// Its id.
	pub id: PurseId,
	/// The name it was created with.
	pub name: String,
	/// The tag it was created with.
	pub tag: PurseTag,
}

/// Why a purse could not be created.
#[derive(Debug, thiserror::Error)]
pub enum CreatePurseError {
	/// Every id below [`PurseId::MAIN`] is taken.
	#[error("no purse id left: ids 0..{} are all taken", u32::MAX)]
	IdsExhausted,
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// Layer 1's store of purses.
#[derive(Clone)]
pub struct NftLedger {
	db: Db,
}

impl NftLedger {
	/// A ledger over the core database.
	pub fn new(db: Db) -> Self {
		Self { db }
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
}
