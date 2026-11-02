//! Queries over the tables Layer 2 of NFT purses owns.

use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params};
use truapi::latest::NftPurseReceiveKey;

use crate::{nft::PurseId, platform::ProductId, store::DbError};

const INSERT_RECEIVE_KEY: &str =
	"INSERT INTO nft_receive_key (key, purse, creator) VALUES (:key, :purse, :creator)";

/// Records that `creator` made `key`, whose cheques go into `purse`.
pub fn insert_receive_key(
	tx: &Transaction<'_>,
	key: &NftPurseReceiveKey,
	purse: PurseId,
	creator: &ProductId,
) -> Result<(), DbError> {
	let params = named_params! { ":key": key.0, ":purse": purse.0, ":creator": creator.as_str() };
	tx.prepare_cached(INSERT_RECEIVE_KEY)?.execute(params)?;
	Ok(())
}

const RECEIVE_KEY: &str = "SELECT purse, creator FROM nft_receive_key WHERE key = :key";

/// Where the cheques of a receive key go, and who made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiveKeyRecord {
	/// The purse its cheques go into.
	pub purse: PurseId,
	/// The id of the product that made it, as stored.
	pub creator: String,
}

/// The record of `key`, if it was made on this host.
pub fn find_receive_key(
	conn: &Connection,
	key: &NftPurseReceiveKey,
) -> Result<Option<ReceiveKeyRecord>, DbError> {
	let mut statement = conn.prepare_cached(RECEIVE_KEY)?;
	Ok(statement
		.query_row(named_params! { ":key": key.0 }, receive_key_record)
		.optional()?)
}

fn receive_key_record(row: &Row<'_>) -> rusqlite::Result<ReceiveKeyRecord> {
	Ok(ReceiveKeyRecord { purse: PurseId(row.get(0)?), creator: row.get(1)? })
}
