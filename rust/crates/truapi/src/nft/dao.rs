//! Queries over the NFT tables, one SQL constant and one function each.

use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params};

use super::{
	holding::HeldItem,
	keys::{ItemKey, ItemSlot},
	purse::{PurseId, PurseRecord, PurseTag},
};
use futures::stream::BoxStream;

use crate::{
	installation::InstallationId,
	store::{Db, DbError},
};

const NEXT_PURSE_ID: &str = "SELECT COALESCE(MAX(id) + 1, 0) FROM nft_purse WHERE id < :main";

const INSERT_PURSE: &str = "INSERT INTO nft_purse (id, name, tag) VALUES (:id, :name, :tag)";

/// Inserts a purse under the next free id, or returns `None` when that id
/// would be [`PurseId::MAIN`]. Runs in the writer's transaction, so two
/// creates never take the same id.
pub fn insert_purse(
	tx: &Transaction<'_>,
	name: &str,
	tag: &PurseTag,
) -> Result<Option<PurseId>, DbError> {
	let next: u32 = tx
		.prepare_cached(NEXT_PURSE_ID)?
		.query_row(named_params! { ":main": PurseId::MAIN.0 }, |row| row.get(0))?;
	if next == PurseId::MAIN.0 {
		return Ok(None);
	}
	tx.prepare_cached(INSERT_PURSE)?
		.execute(named_params! { ":id": next, ":name": name, ":tag": tag.0 })?;
	Ok(Some(PurseId(next)))
}

const PURSES_TAGGED: &str = "SELECT id, name, tag FROM nft_purse WHERE tag = :tag ORDER BY id";

/// Every purse carrying `tag`, by id.
pub fn purses_tagged(conn: &Connection, tag: &PurseTag) -> Result<Vec<PurseRecord>, DbError> {
	let mut statement = conn.prepare_cached(PURSES_TAGGED)?;
	let rows = statement.query_map(named_params! { ":tag": tag.0 }, purse_record)?;
	Ok(rows.collect::<Result<_, _>>()?)
}

const PURSE: &str = "SELECT id, name, tag FROM nft_purse WHERE id = :id";

/// The purse with `id`, if any.
pub fn purse(conn: &Connection, id: PurseId) -> Result<Option<PurseRecord>, DbError> {
	let mut statement = conn.prepare_cached(PURSE)?;
	Ok(statement.query_row(named_params! { ":id": id.0 }, purse_record).optional()?)
}

const NEXT_ITEM_INDEX: &str = "SELECT COALESCE(MAX(idx) + 1, 0) FROM nft_item
	WHERE purse = :purse AND installation = :installation";

/// The first index of `purse` under `installation` no item has taken.
pub fn next_item_index(
	conn: &Connection,
	purse: PurseId,
	installation: &InstallationId,
) -> Result<u32, DbError> {
	let mut statement = conn.prepare_cached(NEXT_ITEM_INDEX)?;
	let params = named_params! { ":purse": purse.0, ":installation": installation.0 };
	Ok(statement.query_row(params, |row| row.get(0))?)
}

const INSERT_ITEM: &str = "INSERT INTO nft_item (key, purse, installation, idx)
	VALUES (:key, :purse, :installation, :idx)";

/// Records each key at its slot.
pub fn insert_items(tx: &Transaction<'_>, items: &[(ItemSlot, ItemKey)]) -> Result<(), DbError> {
	let mut statement = tx.prepare_cached(INSERT_ITEM)?;
	for (slot, key) in items {
		statement.execute(named_params! {
			":key": key.0,
			":purse": slot.purse.0,
			":installation": slot.installation.0,
			":idx": slot.index.0,
		})?;
	}
	Ok(())
}

const AVAILABLE_ITEMS: &str = "SELECT item.key FROM nft_item AS item
	JOIN nft_holding AS holding ON holding.key = item.key
	WHERE item.purse = :purse
	ORDER BY item.installation, item.idx
	LIMIT :limit OFFSET :offset";

/// Up to `limit` keys of `purse` that hold an NFT, skipping the first
/// `offset`, ordered by slot.
pub fn available_items(
	conn: &Connection,
	purse: PurseId,
	limit: u64,
	offset: u64,
) -> Result<Vec<ItemKey>, DbError> {
	let mut statement = conn.prepare_cached(AVAILABLE_ITEMS)?;
	let params = named_params! { ":purse": purse.0, ":limit": limit, ":offset": offset };
	let rows = statement.query_map(params, |row| Ok(ItemKey(row.get("key")?)))?;
	Ok(rows.collect::<Result<_, _>>()?)
}

const ITEM_KEYS: &str = "SELECT key FROM nft_item";

/// Every item key, now and after each change.
pub fn observe_item_keys(db: &Db) -> BoxStream<'static, Result<Vec<ItemKey>, DbError>> {
	db.observe(ITEM_KEYS, |statement| statement.query_map([], |row| Ok(ItemKey(row.get("key")?))))
}

const UPSERT_HOLDING: &str = "INSERT INTO nft_holding
	(key, collection, item, instance) VALUES (:key, :collection, :item, :instance)
	ON CONFLICT (key) DO UPDATE SET collection = excluded.collection, item = excluded.item,
		instance = excluded.instance";

/// Records that `key` holds `held`. The instance is stored as the
/// bit pattern of an `i64`, since SQLite integers are signed.
pub fn upsert_holding(tx: &Transaction<'_>, key: &ItemKey, held: &HeldItem) -> Result<(), DbError> {
	tx.prepare_cached(UPSERT_HOLDING)?.execute(named_params! {
		":key": key.0,
		":collection": held.collection,
		":item": held.item,
		":instance": held.instance as i64,
	})?;
	Ok(())
}

const DELETE_HOLDING: &str = "DELETE FROM nft_holding WHERE key = :key";

/// Records that `key` holds nothing.
pub fn delete_holding(tx: &Transaction<'_>, key: &ItemKey) -> Result<(), DbError> {
	tx.prepare_cached(DELETE_HOLDING)?.execute(named_params! { ":key": key.0 })?;
	Ok(())
}

fn purse_record(row: &Row<'_>) -> rusqlite::Result<PurseRecord> {
	Ok(PurseRecord {
		id: PurseId(row.get("id")?),
		name: row.get("name")?,
		tag: PurseTag(row.get("tag")?),
	})
}
