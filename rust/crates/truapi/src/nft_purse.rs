//! Layer 2 of NFT purses: the RFC's purse calls over [`crate::nft`].
//!
//! Ownership lives in the purse tag. This layer decides which grant a call
//! needs; the capability handler asks for it, because the prompt belongs to the
//! product connection.

mod cheque;
mod dao;
mod operation;
mod receive_key;

use core::num::NonZeroU32;
use std::sync::Arc;

use truapi::latest::{
	NFT_PURSE_MAX_NAME_BYTES, NftPurseId, NftPurseInfoPage, NftPurseItemKey, NftPurseOwner,
	NftPursePurse, RemotePermission,
};

#[cfg(test)]
pub use cheque::keys_in_cheque;
pub use operation::drain_needs_confirmation;
pub use receive_key::CreateReceiveKeyError;

use crate::{
	encrypted_storage::EncryptedStorage,
	nft::{
		AllocateItemsError, CreatePurseError, ItemKey, NftLedger, PageRequest, PurseId,
		PurseRecord, PurseTag,
	},
	platform::{ProductId, ProductIdError},
	store::{Db, DbError},
};

/// Who owns a purse, with a product's id validated and normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurseOwner {
	/// The user's own account: the main purse, and purses recovered without
	/// metadata.
	Root,
	/// The product that created the purse.
	Product(ProductId),
}

impl PurseOwner {
	/// The owner a product named, refusing a product id that is not one.
	pub fn parse(owner: &NftPurseOwner) -> Result<Self, ProductIdError> {
		match owner {
			NftPurseOwner::Root => Ok(Self::Root),
			NftPurseOwner::Product(id) => id.parse().map(Self::Product),
		}
	}

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

	/// The owner as the protocol names it.
	pub fn to_wire(&self) -> NftPurseOwner {
		match self {
			Self::Root => NftPurseOwner::Root,
			Self::Product(id) => NftPurseOwner::Product(id.as_str().to_owned()),
		}
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

/// The RFC's purse calls, shared by every product.
#[derive(Clone)]
pub struct NftPurses {
	ledger: NftLedger,
	db: Db,
	encrypted: Arc<dyn EncryptedStorage>,
}

impl NftPurses {
	/// The RFC calls over `deps`.
	pub fn new(deps: NftPursesDeps) -> Self {
		let NftPursesDeps { ledger, db, encrypted } = deps;
		Self { ledger, db, encrypted }
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
		let owner = PurseOwner::Product(caller.clone());
		let created = self.ledger.create_purse(name, owner.tag()).await?;
		Ok(created)
	}

	/// Who owns `purse`, or `None` when no purse has that id.
	pub async fn owner_of(&self, purse: PurseId) -> Result<Option<PurseOwner>, OwnerLookupError> {
		let Some(record) = self.ledger.purse(purse).await? else {
			return Ok(None);
		};
		let unreadable = || OwnerLookupError::UnreadableTag { purse, tag: record.tag.0.clone() };
		PurseOwner::from_tag(&record.tag).ok_or_else(unreadable).map(Some)
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
	pub async fn list_purses(&self, owner: &PurseOwner) -> Result<Vec<NftPursePurse>, DbError> {
		let records = self.ledger.purses_tagged(owner.tag()).await?;
		Ok(records.into_iter().map(listed_purse).collect())
	}
}

/// Purse calls over `db` with the test ledger and encrypted storage in memory.
#[cfg(test)]
pub fn test_purses(db: Db) -> NftPurses {
	let ledger = crate::nft::test_ledger(db.clone());
	let encrypted = Arc::new(crate::encrypted_storage::MemoryEncryptedStorage::default());
	NftPurses::new(NftPursesDeps { ledger, db, encrypted })
}

/// The grant `caller` needs to read `owner`'s purses, or `None` when they are
/// its own.
pub fn read_grant(caller: &ProductId, owner: &PurseOwner) -> Option<RemotePermission> {
	if matches!(owner, PurseOwner::Product(id) if id == caller) {
		return None;
	}
	Some(RemotePermission::NftPurseRead { owner: owner.to_wire() })
}

fn item_key(key: ItemKey) -> NftPurseItemKey {
	NftPurseItemKey(key.0)
}

fn listed_purse(record: PurseRecord) -> NftPursePurse {
	NftPursePurse { id: NftPurseId(record.id.0), name: record.name }
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;
	use tempfile::TempDir;

	use super::*;
	use crate::store::open_temporary_core_db;

	const PRODUCT: &str = "tickets.dot";

	/// The purse calls, with the directory that keeps their database alive.
	fn purses() -> (TempDir, NftPurses) {
		let (dir, db) = open_temporary_core_db();
		(dir, test_purses(db))
	}

	fn product_id(id: &str) -> ProductId {
		ProductId::for_test(id)
	}

	fn product(id: &str) -> PurseOwner {
		PurseOwner::Product(product_id(id))
	}

	fn create(purses: &NftPurses, caller: &str, name: &str) -> PurseId {
		block_on(purses.create_purse(&product_id(caller), name.to_owned())).expect("purse created")
	}

	fn listed(purses: &NftPurses, owner: &PurseOwner) -> Vec<NftPursePurse> {
		block_on(purses.list_purses(owner)).expect("purses list")
	}

	fn purse(id: u32, name: &str) -> NftPursePurse {
		NftPursePurse { id: NftPurseId(id), name: name.to_owned() }
	}

	/// Item keys derive from the purse id and recovery walks ids from 0, so a
	/// gap or a reused id would lose items.
	#[test]
	fn purses_get_sequential_ids_from_zero() {
		let (_dir, purses) = purses();
		let ids: Vec<PurseId> =
			["a", "b", "c"].into_iter().map(|name| create(&purses, PRODUCT, name)).collect();
		assert_eq!(ids, vec![PurseId(0), PurseId(1), PurseId(2)]);
	}

	#[test]
	fn a_fresh_database_gives_root_the_main_purse() {
		let (_dir, purses) = purses();
		assert_eq!(listed(&purses, &PurseOwner::Root), vec![purse(PurseId::MAIN.0, "Main")]);
	}

	#[test]
	fn a_product_lists_only_its_own_purses() {
		let (_dir, purses) = purses();
		create(&purses, PRODUCT, "mine");
		create(&purses, "other.dot", "theirs");
		assert_eq!(listed(&purses, &product(PRODUCT)), vec![purse(0, "mine")]);
	}

	#[test]
	fn a_product_without_purses_lists_none() {
		let (_dir, purses) = purses();
		assert_eq!(listed(&purses, &product(PRODUCT)), vec![]);
	}

	/// The empty product id would share Root's empty tag, so naming it as an
	/// owner must fail rather than reach Root's purses.
	#[test]
	fn an_empty_product_owner_is_refused() {
		let refused = PurseOwner::parse(&NftPurseOwner::Product(String::new()));
		assert_eq!(refused, Err(ProductIdError::Empty));
	}

	/// Lookups match tags byte for byte, so an owner a product spells
	/// differently must still find the purses its owner created.
	#[test]
	fn an_owner_is_found_however_the_product_spells_its_id() {
		let (_dir, purses) = purses();
		create(&purses, PRODUCT, "mine");
		let spelled = PurseOwner::parse(&NftPurseOwner::Product("Tickets.DOT".to_owned()))
			.expect("valid product id");
		assert_eq!(listed(&purses, &spelled), vec![purse(0, "mine")]);
	}

	/// The RFC caps names at 64 bytes, and a refused name must not use up an id.
	#[test]
	fn names_are_capped_at_64_bytes_without_using_an_id() {
		let (_dir, purses) = purses();
		let refused = block_on(purses.create_purse(&product_id(PRODUCT), "x".repeat(65)));
		assert!(matches!(refused, Err(CreateProductPurseError::NameTooLong { len: 65 })));
		assert_eq!(create(&purses, PRODUCT, &"x".repeat(64)), PurseId(0));
	}

	/// The id after `u32::MAX - 1` would collide with the main purse.
	#[test]
	fn creating_past_the_last_id_is_refused() {
		let (_dir, db) = open_temporary_core_db();
		let purses = test_purses(db.clone());
		let last = PurseId::MAIN.0 - 1;
		block_on(db.write(move |tx| {
			tx.execute("INSERT INTO nft_purse (id, name, tag) VALUES (?1, 'last', x'')", [last])?;
			Ok(())
		}))
		.unwrap();
		let refused = block_on(purses.create_purse(&product_id(PRODUCT), "one more".to_owned()));
		assert!(matches!(
			refused,
			Err(CreateProductPurseError::Ledger(CreatePurseError::IdsExhausted))
		));
	}

	/// A grant is never asked for a product's own purses, and never skipped for
	/// anyone else's.
	#[test]
	fn only_another_owners_purses_need_a_read_grant() {
		let caller = product_id(PRODUCT);
		assert_eq!(read_grant(&caller, &product(PRODUCT)), None);
		assert_eq!(
			read_grant(&caller, &PurseOwner::Root),
			Some(RemotePermission::NftPurseRead { owner: NftPurseOwner::Root })
		);
		assert_eq!(
			read_grant(&caller, &product("other.dot")),
			Some(RemotePermission::NftPurseRead {
				owner: NftPurseOwner::Product("other.dot".to_owned())
			})
		);
	}

	/// The owner decides which grant a call needs, so it must read back as
	/// exactly the one the purse was created for.
	#[test]
	fn a_purse_reads_back_the_owner_it_was_created_for() {
		let (_dir, purses) = purses();
		let created = create(&purses, PRODUCT, "Tickets");
		let owners = [created, PurseId::MAIN, PurseId(7)]
			.map(|purse| block_on(purses.owner_of(purse)).expect("owner is read"));
		assert_eq!(owners, [Some(product(PRODUCT)), Some(PurseOwner::Root), None]);
	}

	/// A tag that names no product must not be read as Root, which would
	/// ask for the wrong grant.
	#[test]
	fn a_tag_naming_no_product_is_an_error() {
		let (_dir, db) = open_temporary_core_db();
		let purses = test_purses(db.clone());
		block_on(db.write(|tx| {
			tx.execute("INSERT INTO nft_purse (id, name, tag) VALUES (0, 'odd', x'ff')", [])?;
			Ok(())
		}))
		.expect("odd purse is written");
		let read = block_on(purses.owner_of(PurseId(0)));
		assert!(matches!(
			read,
			Err(OwnerLookupError::UnreadableTag { purse: PurseId(0), tag }) if tag == [0xff]
		));
	}
}
