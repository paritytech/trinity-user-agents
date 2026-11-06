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

fn product(id: &str) -> NftPurseOwner {
	NftPurseOwner::Product(product_id(id))
}

fn create(purses: &NftPurses, caller: &str, name: &str) -> PurseId {
	block_on(purses.create_purse(&product_id(caller), name.to_owned())).expect("purse created")
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
	let (_dir, purses) = purses();
	let ids: Vec<PurseId> =
		["a", "b", "c"].into_iter().map(|name| create(&purses, PRODUCT, name)).collect();
	assert_eq!(ids, vec![PurseId(0), PurseId(1), PurseId(2)]);
}

#[test]
fn a_fresh_database_gives_root_the_main_purse() {
	let (_dir, purses) = purses();
	assert_eq!(listed(&purses, &NftPurseOwner::Root), vec![purse(PurseId::MAIN.0, "Main")]);
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
	use parity_scale_codec::{Decode, Encode};
	let encoded = (0u8, "").encode();
	assert!(NftPurseOwner::decode(&mut encoded.as_slice()).is_err());
}

/// Lookups match tags byte for byte, so an owner a product spells
/// differently must still find the purses its owner created.
#[test]
fn an_owner_is_found_however_the_product_spells_its_id() {
	let (_dir, purses) = purses();
	create(&purses, PRODUCT, "mine");
	let spelled = NftPurseOwner::Product(product_id("Tickets.DOT"));
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
		read_grant(&caller, &NftPurseOwner::Root),
		Some(RemotePermission::NftPurseRead { owner: NftPurseOwner::Root })
	);
	assert_eq!(
		read_grant(&caller, &product("other.dot")),
		Some(RemotePermission::NftPurseRead {
			owner: NftPurseOwner::Product(product_id("other.dot"))
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
	assert_eq!(owners, [Some(product(PRODUCT)), Some(NftPurseOwner::Root), None]);
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
