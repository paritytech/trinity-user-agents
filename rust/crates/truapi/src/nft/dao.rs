//! Queries over the NFT tables, one SQL constant and one function each.

use std::{collections::HashMap, rc::Rc};

use futures::stream::BoxStream;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params, types::Value};

use super::{
	holding::HeldItem,
	keys::{ItemIndex, ItemKey, ItemSlot},
	purse::{PurseId, PurseRecord, PurseTag},
};
use crate::{
	durable::DurableTxId,
	installation::InstallationId,
	store::{Db, DbError},
};

/// Where an item key that holds an NFT in a purse stands for that purse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStanding {
	/// It holds an NFT but is handed off or the input of a live transaction.
	Spent,
	/// It holds `instance` at `slot` and nothing spends it.
	Idle {
		/// The instance it holds.
		instance: u64,
		/// Where it sits in the derivation tree.
		slot: ItemSlot,
	},
}

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

/// Whether the key of `item` is spent: handed off, or the input of a live
/// transaction. An input of a failed transaction no longer counts.
macro_rules! spent {
	() => {
		concat!(
			"EXISTS (SELECT 1 FROM nft_consumption AS spent
			LEFT JOIN durable_tx AS tx ON tx.id = spent.tx
			WHERE spent.key = item.key
			AND (spent.kind = 'HANDOFF' OR tx.status IN ",
			live_statuses!(),
			"))"
		)
	};
}

const AVAILABLE_ITEMS: &str = concat!(
	"SELECT item.key FROM nft_item AS item
	JOIN nft_holding AS holding ON holding.key = item.key
	WHERE item.purse = :purse AND NOT ",
	spent!(),
	" ORDER BY item.idx
	LIMIT :limit OFFSET :offset"
);

/// Up to `limit` keys of `purse` that hold an NFT and are not spent,
/// skipping the first `offset`, ordered by slot.
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

const KEY_STANDINGS: &str = concat!(
	"SELECT item.key AS key, item.purse AS purse, item.installation AS installation,
	item.idx AS idx, holding.instance AS instance, ",
	spent!(),
	" AS spent FROM nft_item AS item
	JOIN nft_holding AS holding ON holding.key = item.key
	WHERE item.purse = :purse AND item.key IN rarray(:keys)"
);

/// Where each of `keys` that holds an NFT in `purse` stands, in one query. A
/// key missing from the answer is not in the purse or holds no NFT.
pub fn key_standings(
	conn: &Connection,
	purse: PurseId,
	keys: &[ItemKey],
) -> Result<HashMap<ItemKey, KeyStanding>, DbError> {
	let mut statement = conn.prepare_cached(KEY_STANDINGS)?;
	let key_values: Rc<Vec<Value>> =
		Rc::new(keys.iter().map(|key| Value::Blob(key.0.to_vec())).collect());
	let params = named_params! { ":purse": purse.0, ":keys": key_values };
	let rows = statement.query_map(params, key_standing_from_row)?;
	Ok(rows.collect::<Result<_, _>>()?)
}

const INSERT_INPUT: &str =
	"INSERT INTO nft_consumption (key, kind, tx) VALUES (:key, 'INPUT', :tx)";

/// Records that each transaction spends its key.
pub fn insert_inputs(
	tx: &Transaction<'_>,
	inputs: &[(ItemKey, DurableTxId)],
) -> Result<(), DbError> {
	let mut statement = tx.prepare_cached(INSERT_INPUT)?;
	for (key, spender) in inputs {
		statement.execute(named_params! { ":key": key.0, ":tx": spender.0 })?;
	}
	Ok(())
}

const PURSE_OF: &str = "SELECT purse FROM nft_item WHERE key = :key";

/// The purse `key` belongs to, if it is one of ours.
pub fn purse_of(conn: &Connection, key: &ItemKey) -> Result<Option<PurseId>, DbError> {
	let mut statement = conn.prepare_cached(PURSE_OF)?;
	Ok(statement
		.query_row(named_params! { ":key": key.0 }, |row| row.get("purse"))
		.optional()?
		.map(PurseId))
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

fn key_standing_from_row(row: &Row<'_>) -> rusqlite::Result<(ItemKey, KeyStanding)> {
	let key = ItemKey(row.get("key")?);
	if row.get("spent")? {
		return Ok((key, KeyStanding::Spent));
	}
	let instance: i64 = row.get("instance")?;
	let slot = ItemSlot {
		purse: PurseId(row.get("purse")?),
		installation: InstallationId(row.get("installation")?),
		index: ItemIndex(row.get("idx")?),
	};
	Ok((key, KeyStanding::Idle { instance: instance as u64, slot }))
}

fn purse_record(row: &Row<'_>) -> rusqlite::Result<PurseRecord> {
	Ok(PurseRecord {
		id: PurseId(row.get("id")?),
		name: row.get("name")?,
		tag: PurseTag(row.get("tag")?),
	})
}
