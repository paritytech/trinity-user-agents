use std::{num::NonZeroU32, sync::Arc};

use futures::executor::block_on;
use tempfile::TempDir;

use super::*;
use crate::{
	nft::{
		HeldItem, ItemSecret, ItemSecrets, NftLedger, NftLedgerDeps, PageRequest, PurseTag,
		testing::test_ledger_deps,
	},
	store::{CORE_DB_FILE, Db, open_temporary_core_db},
};

const FROM: PurseId = PurseId(0);

/// A ledger with one purse whose `count` keys each hold an NFT.
struct HeldItems {
	dir: TempDir,
	db: Db,
	ledger: NftLedger,
	keys: Vec<ItemKey>,
}

fn held_items_with(count: u32, deps: impl FnOnce(Db) -> NftLedgerDeps) -> HeldItems {
	let (dir, db) = open_temporary_core_db();
	let ledger = NftLedger::new(deps(db.clone()));
	block_on(ledger.create_purse("from".to_owned(), PurseTag::default())).unwrap();
	let keys = block_on(ledger.allocate_items(FROM, NonZeroU32::new(count).unwrap())).unwrap();
	let holding = |(index, key): (usize, &ItemKey)| {
		(*key, Some(HeldItem { collection: 1, item: 1, instance: 100 + index as u64 }))
	};
	block_on(ledger.record_holdings(keys.iter().enumerate().map(holding).collect())).unwrap();
	HeldItems { dir, db, ledger, keys }
}

fn held_items(count: u32) -> HeldItems {
	held_items_with(count, test_ledger_deps)
}

fn request(keys: &[ItemKey], tag: &[u8]) -> HandoffRequest {
	HandoffRequest {
		from: FROM,
		keys: keys.iter().copied().collect(),
		tag: HandoffTag(tag.to_vec()),
	}
}

/// The public key each handed secret rebuilds.
fn public_keys_of(secrets: &[ItemSecret]) -> Result<Vec<ItemKey>, schnorrkel::SignatureError> {
	let public_key = |secret: &ItemSecret| {
		let imported = schnorrkel::SecretKey::from_bytes(secret.as_bytes())?;
		Ok(ItemKey(imported.to_public().to_bytes()))
	};
	secrets.iter().map(public_key).collect()
}

fn hand_off_keys(
	items: &HeldItems,
	keys: &[ItemKey],
	tag: &[u8],
) -> Result<Vec<ItemKey>, HandOffError<schnorrkel::SignatureError>> {
	block_on(items.ledger.hand_off(request(keys, tag), public_keys_of))
}

fn available_keys(items: &HeldItems) -> Vec<ItemKey> {
	let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
	block_on(items.ledger.available_items(FROM, page)).unwrap().items
}

fn handoff_rows(items: &HeldItems) -> i64 {
	let count = |conn: &Connection| {
		let sql = "SELECT count(*) FROM nft_consumption WHERE kind = 'HANDOFF'";
		Ok(conn.query_row(sql, [], |row| row.get(0))?)
	};
	block_on(items.db.read(count)).unwrap()
}

/// The receiver rebuilds each item key from its secret, in the order the
/// cheque lists the keys, so the seal must get the secrets of exactly those
/// keys, sorted by key.
#[test]
fn the_seal_gets_the_secret_of_each_key_in_key_order() {
	let items = held_items(3);
	let mut sorted = items.keys.clone();
	sorted.sort();

	let rebuilt = hand_off_keys(&items, &items.keys, b"cheque").unwrap();

	assert_eq!(rebuilt, sorted);
}

/// Someone else may hold a handed-off secret, so the key must never be spent
/// again: not listed, and refused by the next handoff.
#[test]
fn a_handed_off_key_is_not_available_and_cannot_be_handed_off_again() {
	let items = held_items(2);
	hand_off_keys(&items, &items.keys[..1], b"first").unwrap();

	let again = hand_off_keys(&items, &items.keys[..1], b"second");

	assert_eq!(available_keys(&items), items.keys[1..].to_vec());
	assert!(
		matches!(again, Err(HandOffError::Item(ItemRefusal::Unavailable(key))) if key == items.keys[0])
	);
}

/// A tag names one cheque, so a second handoff under it is refused and
/// writes nothing, even over other keys.
#[test]
fn a_tag_in_use_is_refused_and_writes_nothing() {
	let items = held_items(2);
	hand_off_keys(&items, &items.keys[..1], b"cheque").unwrap();

	let again = hand_off_keys(&items, &items.keys[1..], b"cheque");

	assert!(matches!(again, Err(HandOffError::TagInUse)));
	assert_eq!(handoff_rows(&items), 1);
}

/// Secrets that were never sealed into a cheque reached nobody, so a failed
/// seal leaves the items spendable.
#[test]
fn a_refused_seal_writes_nothing() {
	let items = held_items(1);
	let refuse = |_: &[ItemSecret]| Err::<(), _>("no recipient");

	let refused = block_on(items.ledger.hand_off(request(&items.keys, b"cheque"), refuse));

	assert!(matches!(refused, Err(HandOffError::Seal("no recipient"))));
	assert_eq!(available_keys(&items), items.keys);
	assert_eq!(handoff_rows(&items), 0);
}

/// A key the purse does not hold has no secret worth handing off, and the
/// check runs before any secret is exported.
#[test]
fn a_key_not_held_in_the_purse_is_refused_before_sealing() {
	let items = held_items(1);
	let not_held = ItemKey([0x55; 32]);
	let never_called = |_: &[ItemSecret]| -> Result<(), ()> { panic!("sealed a refused handoff") };

	let refused = block_on(
		items
			.ledger
			.hand_off(request(&[items.keys[0], not_held], b"cheque"), never_called),
	);

	assert!(
		matches!(refused, Err(HandOffError::Item(ItemRefusal::NotFound(key))) if key == not_held)
	);
	assert_eq!(handoff_rows(&items), 0);
}

/// An exporter with no session.
struct SignedOut;

impl ItemSecrets for SignedOut {
	fn item_secrets(&self, _slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, AuthorityError> {
		Err(AuthorityError::Disconnected)
	}
}

/// Without a session there are no secrets, and nothing is recorded as handed
/// off.
#[test]
fn a_signed_out_host_hands_nothing_off() {
	let signed_out = |db| NftLedgerDeps { secrets: Arc::new(SignedOut), ..test_ledger_deps(db) };
	let items = held_items_with(1, signed_out);

	let refused = hand_off_keys(&items, &items.keys, b"cheque");

	assert!(matches!(refused, Err(HandOffError::Secrets(AuthorityError::Disconnected))));
	assert_eq!(handoff_rows(&items), 0);
}

/// A rebalance can register between the first read and the write. The write
/// checks again, so the item is never both moved and handed off.
#[test]
fn a_key_spent_while_sealing_is_refused_by_the_write() {
	let items = held_items(1);
	let path = items.dir.path().join(CORE_DB_FILE);
	let key = items.keys[0];
	let spend_while_sealing = move |secrets: &[ItemSecret]| {
		let conn = Connection::open(&path)?;
		let sql =
			"INSERT INTO nft_consumption (key, kind, handoff_tag) VALUES (?1, 'HANDOFF', x'02')";
		conn.execute(sql, [key.0])?;
		Ok::<_, rusqlite::Error>(secrets.len())
	};

	let refused =
		block_on(items.ledger.hand_off(request(&items.keys, b"cheque"), spend_while_sealing));

	assert!(
		matches!(refused, Err(HandOffError::Item(ItemRefusal::Unavailable(spent))) if spent == key)
	);
	assert_eq!(handoff_rows(&items), 1);
}

/// An empty handoff would seal nothing and record nothing; it is refused.
#[test]
fn a_handoff_of_no_keys_is_refused() {
	let items = held_items(1);

	let refused = hand_off_keys(&items, &[], b"cheque");

	assert!(matches!(refused, Err(HandOffError::NoItems)));
}
