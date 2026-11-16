//! Queries over the tables Layer 2 of NFT purses owns.

use parity_scale_codec::{Decode, DecodeAll, Encode};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params, types::Type};
use truapi::latest::{NftPurseChequeRecord, NftPurseId, NftPurseReceiveKey};

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
#[derive(Clone)]
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

const INSERT_CHEQUE: &str = "INSERT INTO nft_cheque
	(creator, id, purse, destination, cheque, created_at)
	VALUES (:creator, :id, :purse, :destination, :cheque, :created_at)";

/// Records that `creator` made the cheque of `record`.
pub fn insert_cheque(
	tx: &Transaction<'_>,
	creator: &ProductId,
	record: &NftPurseChequeRecord,
) -> Result<(), DbError> {
	let params = named_params! {
		":creator": creator.as_str(),
		":id": record.cheque.id.0,
		":purse": record.from.0,
		":destination": record.destination.encode(),
		":cheque": record.cheque.encode(),
		":created_at": record.created_at,
	};
	tx.prepare_cached(INSERT_CHEQUE)?.execute(params)?;
	Ok(())
}

const CHEQUES_OF: &str = "SELECT id, purse, destination, cheque, created_at FROM nft_cheque
	WHERE creator = :creator ORDER BY rowid";

/// Every cheque `creator` made, oldest first.
pub fn cheques_of(
	conn: &Connection,
	creator: &ProductId,
) -> Result<Vec<NftPurseChequeRecord>, DbError> {
	let mut statement = conn.prepare_cached(CHEQUES_OF)?;
	let rows =
		statement.query_map(named_params! { ":creator": creator.as_str() }, cheque_record)?;
	Ok(rows.collect::<Result<_, _>>()?)
}

fn cheque_record(row: &Row<'_>) -> rusqlite::Result<NftPurseChequeRecord> {
	let id: [u8; 32] = row.get(0)?;
	Ok(NftPurseChequeRecord {
		from: NftPurseId(row.get(1)?),
		destination: decode_cheque_column(row, 2, &id)?,
		cheque: decode_cheque_column(row, 3, &id)?,
		created_at: row.get(4)?,
	})
}

/// Column `index` of a row of the cheque `id`, decoded from SCALE.
fn decode_cheque_column<T: Decode>(
	row: &Row<'_>,
	index: usize,
	id: &[u8; 32],
) -> rusqlite::Result<T> {
	let bytes: Vec<u8> = row.get(index)?;
	T::decode_all(&mut bytes.as_slice()).map_err(|error| {
		let id = hex::encode(id);
		let expected = core::any::type_name::<T>();
		let reason =
			format!("column {index} of stored cheque {id} is not a SCALE {expected}: {error}");
		rusqlite::Error::FromSqlConversionFailure(index, Type::Blob, reason.into())
	})
}
