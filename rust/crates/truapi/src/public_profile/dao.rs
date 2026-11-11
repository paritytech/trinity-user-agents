//! Resolved profile records retained by last use in the core database.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{OptionalExtension, Transaction, named_params};

use super::{RECORD_TTL_SECS, record::ProfileRecord};
use crate::store::DbError;

/// Most profile records retained, including stale and empty records.
const MAX_ROWS: u32 = 1000;

/// A stored record with a freshness deadline independent of its retention.
pub struct CachedRecord {
	/// Full record, including entries the current core does not understand.
	pub record: ProfileRecord,
	expires_at: u64,
}

impl CachedRecord {
	/// Whether the record is fresh at Unix time `now`, in seconds.
	///
	/// A backward clock correction does not extend the record's freshness.
	pub fn is_fresh(&self, now: u64) -> bool {
		self.expires_at > now && self.expires_at - now <= RECORD_TTL_SECS
	}
}

const SELECT_RECORD: &str = "SELECT public_record, expires_at FROM public_profiles
	WHERE username = :username";

const TOUCH_RECORD: &str = "UPDATE public_profiles
	SET access_order = (SELECT coalesce(max(access_order), 0) + 1 FROM public_profiles)
	WHERE username = :username";

/// Reads stored metadata and marks it as most recently used in the same transaction.
///
/// Expired records remain available. Undecodable rows count as missing; database
/// failures return errors without committing the access-order update.
pub fn stored_record(
	tx: &Transaction<'_>,
	username: &str,
) -> Result<Option<CachedRecord>, DbError> {
	let stored: Option<(Vec<u8>, u64)> = tx
		.prepare_cached(SELECT_RECORD)?
		.query_row(named_params! { ":username": username }, |row| Ok((row.get(0)?, row.get(1)?)))
		.optional()?;
	let Some((bytes, expires_at)) = stored else { return Ok(None) };
	let Ok(record) = ProfileRecord::decode(&mut bytes.as_slice()) else { return Ok(None) };
	tx.prepare_cached(TOUCH_RECORD)?
		.execute(named_params! { ":username": username })?;
	Ok(Some(CachedRecord { record, expires_at }))
}

const UPSERT_RECORD: &str =
	"INSERT INTO public_profiles (username, public_record, expires_at, access_order)
	VALUES (:username, :public_record, :expires_at,
		(SELECT coalesce(max(access_order), 0) + 1 FROM public_profiles))
	ON CONFLICT (username) DO UPDATE SET public_record = excluded.public_record,
		expires_at = excluded.expires_at, access_order = excluded.access_order";

const DELETE_OVER_CAP: &str = "DELETE FROM public_profiles WHERE username IN (
	SELECT username FROM public_profiles ORDER BY access_order, username
	LIMIT max(0, (SELECT COUNT(*) FROM public_profiles) - :max_rows))";

/// Stores a resolved record and evicts the least recently used records over capacity.
///
/// `expires_at` is Unix time in seconds. Stale records are retained within the
/// 1,000-row limit. Database failures return errors to the caller's transaction.
pub fn store_record(
	tx: &Transaction<'_>,
	username: &str,
	record: &ProfileRecord,
	expires_at: u64,
) -> Result<(), DbError> {
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
