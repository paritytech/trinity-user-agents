//! Queries over the `nft_purse` table, one SQL constant and one function each.

use rusqlite::{Connection, Row, Transaction, named_params};

use super::purse::{PurseId, PurseRecord, PurseTag};
use crate::store::DbError;

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

fn purse_record(row: &Row<'_>) -> rusqlite::Result<PurseRecord> {
	Ok(PurseRecord {
		id: PurseId(row.get("id")?),
		name: row.get("name")?,
		tag: PurseTag(row.get("tag")?),
	})
}
