//! Resolved profile records in the core database.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};

use super::{RECORD_TTL_SECS, record::ProfileRecord};
use crate::store::DbError;

/// Most rows the table holds; past it the rows closest to expiry go first.
const MAX_ROWS: u32 = 1000;

const SELECT_RECORD: &str = "SELECT public_record FROM public_profiles
	WHERE username = :username AND expires_at > :now AND expires_at - :now <= :ttl";

/// The record cached for `username` within its lifetime. Future-dated or
/// undecodable rows count as missing.
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

/// Caches `record` for `username` until `expires_at`, after deleting what
/// expired at `now`.
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
mod tests {
	use subxt::utils::WrapperKeepOpaque;

	use super::*;
	use crate::{
		ImageFormat,
		public_profile::record::{ProfileAvatar, ProfileEntry},
		store::{Db, open_temporary_core_db},
	};

	fn record_with_avatar() -> ProfileRecord {
		ProfileRecord {
			entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
				cid: [1; 32].into(),
				format: ImageFormat::Webp,
			}))],
		}
	}

	async fn store(
		db: &Db,
		username: &'static str,
		record: ProfileRecord,
		now: u64,
		expires_at: u64,
	) {
		db.write(move |tx| store_record(tx, username, &record, now, expires_at))
			.await
			.expect("row written");
	}

	async fn fresh(db: &Db, username: &'static str, now: u64) -> Option<ProfileRecord> {
		db.read(move |conn| fresh_record(conn, username, now)).await.expect("row read")
	}

	async fn row_count(db: &Db) -> u32 {
		db.read(|conn| {
			Ok(conn.query_row("SELECT COUNT(*) FROM public_profiles", [], |row| row.get(0))?)
		})
		.await
		.expect("rows counted")
	}

	#[tokio::test]
	async fn a_record_is_served_until_it_expires() {
		let (_dir, db) = open_temporary_core_db();
		store(&db, "alice.01", record_with_avatar(), 100, 200).await;

		assert_eq!(fresh(&db, "alice.01", 199).await, Some(record_with_avatar()));
		assert_eq!(fresh(&db, "alice.01", 200).await, None);
	}

	#[tokio::test]
	async fn a_name_without_a_record_is_cached_as_an_empty_one() {
		let (_dir, db) = open_temporary_core_db();
		store(&db, "bob.02", ProfileRecord::default(), 100, 200).await;

		assert_eq!(fresh(&db, "bob.02", 150).await, Some(ProfileRecord::default()));
	}

	#[tokio::test]
	async fn a_clock_correction_cannot_extend_a_records_lifetime() {
		let (_dir, db) = open_temporary_core_db();
		let written_at = 10_000;
		store(&db, "alice.01", ProfileRecord::default(), written_at, written_at + RECORD_TTL_SECS)
			.await;

		assert_eq!(
			(
				fresh(&db, "alice.01", written_at - 1).await,
				fresh(&db, "alice.01", written_at).await,
			),
			(None, Some(ProfileRecord::default())),
		);
	}

	#[tokio::test]
	async fn a_write_deletes_expired_rows_and_keeps_the_table_under_its_cap() {
		let (_dir, db) = open_temporary_core_db();
		store(&db, "expired.01", ProfileRecord::default(), 0, 50).await;
		db.write(|tx| {
			for index in 0..MAX_ROWS {
				store_record(
					tx,
					&format!("user{index}"),
					&ProfileRecord::default(),
					100,
					1000 + u64::from(index),
				)?;
			}
			Ok(())
		})
		.await
		.expect("rows written");

		store(&db, "newest.01", ProfileRecord::default(), 100, 100 + RECORD_TTL_SECS).await;

		assert_eq!(row_count(&db).await, MAX_ROWS);
		assert_eq!(fresh(&db, "expired.01", 0).await, None, "deleted once it expired");
		assert_eq!(fresh(&db, "user0", 100).await, None, "closest to expiry, deleted over the cap");
		assert_eq!(fresh(&db, "newest.01", 100).await, Some(ProfileRecord::default()));
	}
}
