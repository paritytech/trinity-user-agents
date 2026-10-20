//! Layer 2 of NFT purses: the RFC's purse calls over [`crate::nft`].
//!
//! Ownership lives in the purse tag. This layer decides which grant a call
//! needs; the capability handler asks for it, because the prompt belongs to the
//! product connection.

use truapi::latest::{
	NFT_PURSE_MAX_NAME_BYTES, NftPurseId, NftPurseOwner, NftPursePurse, RemotePermission,
};

use crate::{
	nft::{CreatePurseError, NftLedger, PurseId, PurseRecord, PurseTag},
	store::DbError,
};

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

/// The RFC's purse calls, shared by every product.
#[derive(Clone)]
pub struct NftPurses {
	ledger: NftLedger,
}

impl NftPurses {
	/// The RFC calls over `ledger`.
	pub fn new(ledger: NftLedger) -> Self {
		Self { ledger }
	}

	/// Creates a purse owned by the product `caller`.
	pub async fn create_purse(
		&self,
		caller: &str,
		name: String,
	) -> Result<PurseId, CreateProductPurseError> {
		if name.len() > NFT_PURSE_MAX_NAME_BYTES {
			return Err(CreateProductPurseError::NameTooLong { len: name.len() });
		}
		let owner = NftPurseOwner::Product(caller.to_owned());
		let created = self.ledger.create_purse(name, owner_tag(&owner)).await?;
		Ok(created)
	}

	/// The purses `owner` holds, by id. The caller checks [`read_grant`] first.
	pub async fn list_purses(&self, owner: &NftPurseOwner) -> Result<Vec<NftPursePurse>, DbError> {
		let records = self.ledger.purses_tagged(owner_tag(owner)).await?;
		Ok(records.into_iter().map(listed_purse).collect())
	}
}

/// The grant `caller` needs to read `owner`'s purses, or `None` when they are
/// its own.
pub fn read_grant(caller: &str, owner: &NftPurseOwner) -> Option<RemotePermission> {
	if matches!(owner, NftPurseOwner::Product(id) if id == caller) {
		return None;
	}
	Some(RemotePermission::NftPurseRead { owner: owner.clone() })
}

/// Root is the empty tag. A product is the UTF-8 of its id, which is never
/// empty.
fn owner_tag(owner: &NftPurseOwner) -> PurseTag {
	match owner {
		NftPurseOwner::Root => PurseTag::default(),
		NftPurseOwner::Product(id) => PurseTag(id.as_bytes().to_vec()),
	}
}

fn listed_purse(record: PurseRecord) -> NftPursePurse {
	NftPursePurse { id: NftPurseId(record.id.0), name: record.name }
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;

	use super::*;
	use crate::store::{Db, DbConfig, DbLocation, core_migrations};

	const PRODUCT: &str = "tickets.dot";

	fn database() -> Db {
		let config =
			DbConfig { location: DbLocation::Memory, migrations: core_migrations, readers: 1 };
		block_on(Db::open(config)).expect("in-memory core database opens")
	}

	fn purses() -> NftPurses {
		NftPurses::new(NftLedger::new(database()))
	}

	fn product(id: &str) -> NftPurseOwner {
		NftPurseOwner::Product(id.to_owned())
	}

	fn listed(purses: &NftPurses, owner: &NftPurseOwner) -> Vec<NftPursePurse> {
		block_on(purses.list_purses(owner)).expect("purses list")
	}

	fn purse(id: u32, name: &str) -> NftPursePurse {
		NftPursePurse { id: NftPurseId(id), name: name.to_owned() }
	}

	/// Item keys derive from the purse id and recovery walks ids from 0, so a
	/// gap or a reused id would lose items.
	#[test]
	fn purses_get_sequential_ids_from_zero() {
		let purses = purses();
		let ids: Vec<PurseId> = ["a", "b", "c"]
			.into_iter()
			.map(|name| block_on(purses.create_purse(PRODUCT, name.to_owned())).unwrap())
			.collect();
		assert_eq!(ids, vec![PurseId(0), PurseId(1), PurseId(2)]);
	}

	#[test]
	fn a_fresh_database_gives_root_the_main_purse() {
		let purses = purses();
		assert_eq!(listed(&purses, &NftPurseOwner::Root), vec![purse(PurseId::MAIN.0, "Main")]);
	}

	#[test]
	fn a_product_lists_only_its_own_purses() {
		let purses = purses();
		block_on(purses.create_purse(PRODUCT, "mine".to_owned())).unwrap();
		block_on(purses.create_purse("other.dot", "theirs".to_owned())).unwrap();
		assert_eq!(listed(&purses, &product(PRODUCT)), vec![purse(0, "mine")]);
	}

	#[test]
	fn a_product_without_purses_lists_none() {
		assert_eq!(listed(&purses(), &product(PRODUCT)), vec![]);
	}

	/// The RFC caps names at 64 bytes, and a refused name must not use up an id.
	#[test]
	fn names_are_capped_at_64_bytes_without_using_an_id() {
		let purses = purses();
		let refused = block_on(purses.create_purse(PRODUCT, "x".repeat(65)));
		assert!(matches!(refused, Err(CreateProductPurseError::NameTooLong { len: 65 })));
		let created = block_on(purses.create_purse(PRODUCT, "x".repeat(64))).unwrap();
		assert_eq!(created, PurseId(0));
	}

	/// The id after `u32::MAX - 1` would collide with the main purse.
	#[test]
	fn creating_past_the_last_id_is_refused() {
		let db = database();
		let purses = NftPurses::new(NftLedger::new(db.clone()));
		let last = PurseId::MAIN.0 - 1;
		block_on(db.write(move |tx| {
			tx.execute("INSERT INTO nft_purse (id, name, tag) VALUES (?1, 'last', x'')", [last])?;
			Ok(())
		}))
		.unwrap();
		let refused = block_on(purses.create_purse(PRODUCT, "one more".to_owned()));
		assert!(matches!(
			refused,
			Err(CreateProductPurseError::Ledger(CreatePurseError::IdsExhausted))
		));
	}

	/// A grant is never asked for a product's own purses, and never skipped for
	/// anyone else's.
	#[test]
	fn only_another_owners_purses_need_a_read_grant() {
		assert_eq!(read_grant(PRODUCT, &product(PRODUCT)), None);
		assert_eq!(
			read_grant(PRODUCT, &NftPurseOwner::Root),
			Some(RemotePermission::NftPurseRead { owner: NftPurseOwner::Root })
		);
		assert_eq!(
			read_grant(PRODUCT, &product("other.dot")),
			Some(RemotePermission::NftPurseRead { owner: product("other.dot") })
		);
	}
}
