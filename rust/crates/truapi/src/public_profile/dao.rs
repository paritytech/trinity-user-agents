//! Resolved profile records in the core database.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};

use super::{RECORD_TTL_SECS, record::ProfileRecord};
use crate::store::DbError;

/// Most rows the table holds; past it the rows closest to expiry go first.
const MAX_ROWS: u32 = 1000;

const SELECT_RECORD: &str = "SELECT public_record FROM public_profiles
	WHERE username = :username AND expires_at > :now AND expires_at - :now <= :ttl";

/// Returns an unexpired record from native storage.
///
/// `now` is Unix time in seconds. Future-dated or undecodable rows count as missing;
/// database failures return errors.
pub fn fresh_record(
	conn: &Connection,
	username: &str,
	now: u64,
) -> Result<Option<ProfileRecord>, DbError> {
	let bytes: Option<Vec<u8>> = conn
		.prepare_cached(SELECT_RECORD)?
		.query_row(
			named_params! { ":username": username, ":now": now, ":ttl": RECORD_TTL_SECS },
			|row| row.get(0),
		)
		.optional()?;
	Ok(bytes.and_then(|bytes| ProfileRecord::decode(&mut bytes.as_slice()).ok()))
}

const DELETE_EXPIRED: &str = "DELETE FROM public_profiles WHERE expires_at <= :now";

const UPSERT_RECORD: &str = "INSERT INTO public_profiles (username, public_record, expires_at)
	VALUES (:username, :public_record, :expires_at)
	ON CONFLICT (username) DO UPDATE SET public_record = excluded.public_record, expires_at = excluded.expires_at";

const DELETE_OVER_CAP: &str = "DELETE FROM public_profiles WHERE username IN (
	SELECT username FROM public_profiles ORDER BY expires_at
	LIMIT max(0, (SELECT COUNT(*) FROM public_profiles) - :max_rows))";

/// Caches a resolved record, pruning expired and excess rows.
///
/// `now` and `expires_at` are Unix times in seconds. Beyond 1,000 rows, the earliest
/// expiry is removed first. Database failures return errors to the caller's transaction.
pub fn store_record(
	tx: &Transaction<'_>,
	username: &str,
	record: &ProfileRecord,
	now: u64,
	expires_at: u64,
) -> Result<(), DbError> {
	tx.prepare_cached(DELETE_EXPIRED)?.execute(named_params! { ":now": now })?;
	tx.prepare_cached(UPSERT_RECORD)?.execute(named_params! {
		":username": username,
		":public_record": record.encode(),
		":expires_at": expires_at,
	})?;
	tx.prepare_cached(DELETE_OVER_CAP)?
		.execute(named_params! { ":max_rows": MAX_ROWS })?;
	Ok(())
}

#[cfg(test)]
mod tests;
