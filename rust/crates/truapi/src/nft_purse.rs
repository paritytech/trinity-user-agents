//! Layer 2 of NFT purses: the RFC's purse calls over [`crate::nft`].
//!
//! Ownership lives in the purse tag. This layer decides which grant a call
//! needs; the capability handler asks for it, because the prompt belongs to the
//! product connection.

mod cheque;
mod dao;
mod deposit;
#[cfg(test)]
mod live;
mod operation;
mod payment;
mod receive_key;
mod refund;

use core::num::NonZeroU32;
use std::sync::Arc;

use truapi::latest::{
	NFT_PURSE_MAX_NAME_BYTES, NftPurseId, NftPurseInfoPage, NftPurseItemKey, NftPurseOwner,
	NftPursePurse, RemotePermission,
};

pub use deposit::DepositRefusal;
pub use operation::drain_needs_confirmation;
#[cfg(test)]
pub use payment::{ENVELOPE_VERSION, EnvelopeContents};
pub use payment::{PaymentListen, PaymentListenError};
pub use receive_key::{CreateReceiveKeyError, StaticReceiveSecret};

use crate::{
	encrypted_storage::EncryptedStorage,
	host_logic::statement_store::StatementStoreApi,
	nft::{
		AllocateItemsError, CreatePurseError, ItemKey, NftLedger, PageRequest, PurseId,
		PurseRecord, PurseTag,
	},
	platform::ProductId,
	store::{Db, DbError},
};

impl NftPurseOwner {
	/// Root is the empty tag. A product is the UTF-8 of its id, which
	/// [`ProductId`] guarantees is never empty.
	fn tag(&self) -> PurseTag {
		match self {
			Self::Root => PurseTag::default(),
			Self::Product(id) => PurseTag(id.as_str().as_bytes().to_vec()),
		}
	}

	/// The owner `tag` names, or `None` when it names no product.
	fn from_tag(tag: &PurseTag) -> Option<Self> {
		if tag.0.is_empty() {
			return Some(Self::Root);
		}
		let id = core::str::from_utf8(&tag.0).ok()?;
		id.parse().ok().map(Self::Product)
	}
}

/// Why a product's purse was not created.
#[derive(Debug, thiserror::Error)]
pub enum CreateProductPurseError {
	/// The name is longer than the RFC allows.
	#[error("purse name is {len} bytes, expected at most {NFT_PURSE_MAX_NAME_BYTES}")]
	NameTooLong {
		/// Length of the refused name, in bytes.
		len: usize,
	},
	/// Layer 1 refused or failed.
	#[error(transparent)]
	Ledger(#[from] CreatePurseError),
}

/// Why a purse's owner could not be read.
#[derive(Debug, thiserror::Error)]
pub enum OwnerLookupError {
	/// The stored tag names no product, so the purse has no known owner.
	#[error("purse {purse:?} has tag {tag:?}, expected empty or a product id")]
	UnreadableTag {
		/// The purse.
		purse: PurseId,
		/// Its stored tag.
		tag: Vec<u8>,
	},
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// Most keys one `create_items` call allocates (RFC).
pub const MAX_ITEMS_PER_CALL: u32 = 1000;

/// Largest page `query_purse` returns (RFC).
pub const MAX_PAGE_SIZE: u32 = 100;

/// A count in `1..=MAX`, as the RFC bounds the counts a call takes.
#[derive(Clone)]
pub struct CountUpTo<const MAX: u32>(NonZeroU32);

impl<const MAX: u32> CountUpTo<MAX> {
	/// Returns `count` when it is between 1 and `MAX`, and `None` otherwise.
	pub fn new(count: u32) -> Option<Self> {
		let count = NonZeroU32::new(count).filter(|count| count.get() <= MAX)?;
		Some(Self(count))
	}
}

/// How many items one `create_items` call may allocate.
pub type ItemCount = CountUpTo<MAX_ITEMS_PER_CALL>;

/// A page size `query_purse` accepts.
pub type PageSize = CountUpTo<MAX_PAGE_SIZE>;

/// What the RFC's purse calls run on.
pub struct NftPursesDeps {
	/// Layer 1: purses, items and their transfers.
	pub ledger: NftLedger,
	/// The core database, for the tables Layer 2 owns.
	pub db: Db,
	/// Keeps the secrets of receive keys.
	pub encrypted: Arc<dyn EncryptedStorage>,
	/// Opens `Static` cheques.
	pub static_secret: Arc<dyn StaticReceiveSecret>,
	/// The statement store cheques arrive through.
	pub statements: Arc<dyn StatementStoreApi>,
}

/// The RFC's purse calls, shared by every product.
#[derive(Clone)]
pub struct NftPurses {
	ledger: NftLedger,
	db: Db,
	encrypted: Arc<dyn EncryptedStorage>,
	static_secret: Arc<dyn StaticReceiveSecret>,
	statements: Arc<dyn StatementStoreApi>,
}

impl NftPurses {
	/// The RFC calls over `deps`.
	pub fn new(deps: NftPursesDeps) -> Self {
		let NftPursesDeps { ledger, db, encrypted, static_secret, statements } = deps;
		Self { ledger, db, encrypted, static_secret, statements }
	}

	/// Creates a purse owned by the product `caller`.
	pub async fn create_purse(
		&self,
		caller: &ProductId,
		name: String,
	) -> Result<PurseId, CreateProductPurseError> {
		if name.len() > NFT_PURSE_MAX_NAME_BYTES {
			return Err(CreateProductPurseError::NameTooLong { len: name.len() });
		}
		let owner = NftPurseOwner::Product(caller.clone());
		let created = self.ledger.create_purse(name, owner.tag()).await?;
		Ok(created)
	}

	/// Who owns `purse`, or `None` when no purse has that id.
	pub async fn owner_of(
		&self,
		purse: PurseId,
	) -> Result<Option<NftPurseOwner>, OwnerLookupError> {
		let Some(record) = self.ledger.purse(purse).await? else {
			return Ok(None);
		};
		let unreadable = || OwnerLookupError::UnreadableTag { purse, tag: record.tag.0.clone() };
		NftPurseOwner::from_tag(&record.tag).ok_or_else(unreadable).map(Some)
	}

	/// Allocates `count` keys in `purse`. The caller checks [`read_grant`] on
	/// [`Self::owner_of`] first.
	pub async fn create_items(
		&self,
		purse: PurseId,
		count: ItemCount,
	) -> Result<Vec<NftPurseItemKey>, AllocateItemsError> {
		let keys = self.ledger.allocate_items(purse, count.0).await?;
		Ok(keys.into_iter().map(item_key).collect())
	}

	/// Page `page` of `purse`'s spendable items. The caller checks
	/// [`read_grant`] on [`Self::owner_of`] first.
	pub async fn query_purse(
		&self,
		purse: PurseId,
		page: u32,
		size: PageSize,
	) -> Result<NftPurseInfoPage, DbError> {
		let page = PageRequest { index: page, size: size.0 };
		let available = self.ledger.available_items(purse, page).await?;
		let items = available.items.into_iter().map(item_key).collect();
		Ok(NftPurseInfoPage { has_next: available.has_next, items })
	}

	/// The purses `owner` holds, by id. The caller checks [`read_grant`] first.
	pub async fn list_purses(&self, owner: &NftPurseOwner) -> Result<Vec<NftPursePurse>, DbError> {
		let records = self.ledger.purses_tagged(owner.tag()).await?;
		Ok(records.into_iter().map(listed_purse).collect())
	}
}

/// Purse calls over `db` with the test ledger, encrypted storage in memory,
/// [`TEST_STATIC_SECRET`] and a statement store that has nothing to watch.
#[cfg(test)]
pub fn test_purses(db: Db) -> NftPurses {
	test_purses_watching(db, Arc::default())
}

/// [`test_purses`] whose statement store is `statements`.
#[cfg(test)]
pub fn test_purses_watching(db: Db, statements: Arc<ScriptedStatements>) -> NftPurses {
	test_purses_over(db, statements)
}

/// [`test_purses`] over any statement store, such as a live one.
#[cfg(test)]
pub fn test_purses_over(db: Db, statements: Arc<dyn StatementStoreApi>) -> NftPurses {
	let ledger = crate::nft::test_ledger(db.clone());
	let encrypted = Arc::new(crate::encrypted_storage::MemoryEncryptedStorage::default());
	let static_secret = Arc::new(FixedStaticSecret(TEST_STATIC_SECRET));
	NftPurses::new(NftPursesDeps { ledger, db, encrypted, static_secret, statements })
}

/// A statement store whose every watch yields `batches` and then ends, and
/// that remembers the filters it was asked to watch.
#[cfg(test)]
#[derive(Default)]
pub struct ScriptedStatements {
	/// What each watch yields.
	pub batches: std::sync::Mutex<Vec<Vec<Vec<u8>>>>,
	/// The filters watched, in order.
	pub watched: std::sync::Mutex<Vec<crate::host_logic::statement_store::TopicFilter>>,
}

#[cfg(test)]
#[async_trait::async_trait]
impl StatementStoreApi for ScriptedStatements {
	async fn start_statement_watch(
		&self,
		filter: crate::host_logic::statement_store::TopicFilter,
	) -> Result<
		crate::host_logic::statement_store::StatementBatches,
		crate::host_logic::statement_store::StatementWatchError,
	> {
		self.watched.lock().unwrap().push(filter);
		let batches = self.batches.lock().unwrap().clone().into_iter().map(Ok);
		let watch = futures::stream::iter(batches);
		Ok(futures::StreamExt::boxed(watch))
	}
}

/// The static receive secret of [`test_purses`].
#[cfg(test)]
pub const TEST_STATIC_SECRET: [u8; 32] = [0x5e; 32];

/// Always the same static receive secret.
#[cfg(test)]
pub struct FixedStaticSecret(pub [u8; 32]);

#[cfg(test)]
impl StaticReceiveSecret for FixedStaticSecret {
	fn static_receive_secret(
		&self,
	) -> Result<crate::host_logic::x25519_seal::X25519PrivateKey, crate::runtime::AuthorityError> {
		Ok(crate::host_logic::x25519_seal::X25519PrivateKey::from(self.0))
	}
}

/// The grant `caller` needs to read `owner`'s purses, or `None` when they are
/// its own.
pub fn read_grant(caller: &ProductId, owner: &NftPurseOwner) -> Option<RemotePermission> {
	if matches!(owner, NftPurseOwner::Product(id) if id == caller) {
		return None;
	}
	Some(RemotePermission::NftPurseRead { owner: owner.clone() })
}

fn item_key(key: ItemKey) -> NftPurseItemKey {
	NftPurseItemKey(key.0)
}

fn listed_purse(record: PurseRecord) -> NftPursePurse {
	NftPursePurse { id: NftPurseId(record.id.0), name: record.name }
}

#[cfg(test)]
mod tests;
