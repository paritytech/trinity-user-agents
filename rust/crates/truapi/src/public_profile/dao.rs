//! Resolved profile records retained by last use in the core database.

use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
};

use futures::{
	channel::oneshot,
	future::{AbortHandle, Abortable},
	lock::Mutex as AsyncMutex,
};
use parking_lot::Mutex;
use tracing::warn;

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};

use super::{
	CachedRecord, PublicProfileError, PublicProfilesService, RECORD_TTL_SECS, record::ProfileRecord,
};
use crate::{
	Spawner,
	store::{Db, DbError},
	time::{RealTimer, Timer},
	unix_time::current_unix_secs,
};

const ACCESS_FLUSH_DELAY: core::time::Duration = core::time::Duration::from_secs(1);

/// Native cache state owned by the public profile service.
pub struct Cache {
	db: OnceLock<Db>,
	spawner: Spawner,
	timer: Arc<dyn Timer>,
	accesses: Mutex<ProfileAccesses>,
	writes: AsyncMutex<()>,
}

#[derive(Default)]
struct ProfileAccesses {
	pending: HashMap<String, u64>,
	sequence: u64,
	timer: Option<AbortHandle>,
}

impl Cache {
	/// Creates cache state; the database must be installed before use.
	pub fn new(spawner: Spawner) -> Self {
		Self {
			db: OnceLock::new(),
			spawner,
			timer: Arc::new(RealTimer),
			accesses: Mutex::new(ProfileAccesses::default()),
			writes: AsyncMutex::new(()),
		}
	}

	/// Requires native storage before starting a profile lookup.
	pub fn check_storage(&self) -> Result<(), PublicProfileError> {
		self.db()?;
		Ok(())
	}

	fn db(&self) -> Result<&Db, DbError> {
		self.db.get().ok_or(DbError::NotConfigured)
	}
}

/// Most profile records retained, including stale and empty records.
pub const MAX_ROWS: usize = 1000;

const SELECT_RECORD: &str = "SELECT public_record, expires_at FROM public_profiles
	WHERE username = :username";

const TOUCH_RECORD: &str = "UPDATE public_profiles
	SET access_order = (SELECT coalesce(max(access_order), 0) + 1 FROM public_profiles)
	WHERE username = :username";

/// Reads stored metadata without changing its freshness or retention.
///
/// Expired records remain available. Undecodable rows count as missing; database
/// failures return errors.
pub fn stored_record(conn: &Connection, username: &str) -> Result<Option<CachedRecord>, DbError> {
	let stored: Option<(Vec<u8>, u64)> = conn
		.prepare_cached(SELECT_RECORD)?
		.query_row(named_params! { ":username": username }, |row| Ok((row.get(0)?, row.get(1)?)))
		.optional()?;
	let Some((bytes, expires_at)) = stored else { return Ok(None) };
	let Ok(record) = ProfileRecord::decode(&mut bytes.as_slice()) else { return Ok(None) };
	Ok(Some(CachedRecord { record, expires_at }))
}

/// Marks a stored profile as most recently used without renewing freshness.
///
/// Missing records remain absent; database failures return errors.
pub fn touch_record(tx: &Transaction<'_>, username: &str) -> Result<(), DbError> {
	tx.prepare_cached(TOUCH_RECORD)?
		.execute(named_params! { ":username": username })?;
	Ok(())
}

const UPSERT_RECORD: &str =
	"INSERT INTO public_profiles (username, public_record, expires_at, access_order)
	VALUES (:username, :public_record, :expires_at,
		(SELECT coalesce(max(access_order), 0) + 1 FROM public_profiles))
	ON CONFLICT (username) DO UPDATE SET public_record = excluded.public_record,
		expires_at = excluded.expires_at, access_order = excluded.access_order";

const DELETE_OVER_CAP: &str = "DELETE FROM public_profiles WHERE username IN (
	SELECT username FROM public_profiles ORDER BY access_order DESC, username DESC
	LIMIT -1 OFFSET :max_rows)";

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

impl PublicProfilesService {
	/// Sets the native cache database once.
	///
	/// Returns `None` if a database was already installed.
	pub fn install_db(&self, db: Db) -> Option<()> {
		self.cache.db.set(db).ok()
	}

	/// Reads cached metadata and records its use without waiting for the writer.
	///
	/// Missing or undecodable records return `None`; storage failures return errors.
	pub async fn stored_record(
		self: &Arc<Self>,
		username: &str,
	) -> Result<Option<CachedRecord>, PublicProfileError> {
		let db = self.cache.db()?;
		let username = username.to_owned();
		let cached_username = username.clone();
		let cached = db.read(move |conn| stored_record(conn, &cached_username)).await?;
		if cached.is_some() {
			self.record_access(username);
		}
		Ok(cached)
	}

	/// Persists resolved metadata with pending accesses before capacity eviction.
	///
	/// Storage failures return errors. Dispatched writes finish after cancellation.
	pub async fn store_record(
		self: &Arc<Self>,
		username: &str,
		record: ProfileRecord,
	) -> Result<(), PublicProfileError> {
		let username = username.to_owned();
		let expires_at = current_unix_secs() + RECORD_TTL_SECS;
		let (reply, result) = oneshot::channel();
		let profiles = self.clone();
		self.cache.spawner.spawn(async move {
			let _writes = profiles.cache.writes.lock().await;
			if !reply.is_canceled() {
				let result = profiles.persist(Some((username, record, expires_at))).await;
				let _ = reply.send(result);
			}
		});
		result.await.map_err(|_| DbError::Closed)??;
		Ok(())
	}

	fn record_access(self: &Arc<Self>, username: String) {
		{
			let mut accesses = self.cache.accesses.lock();
			accesses.sequence += 1;
			let sequence = accesses.sequence;
			accesses.pending.insert(username, sequence);
			if accesses.pending.len() > MAX_ROWS {
				let oldest = accesses
					.pending
					.iter()
					.min_by_key(|(_, sequence)| *sequence)
					.unwrap()
					.0
					.clone();
				accesses.pending.remove(&oldest);
			}
		}
		self.schedule_flush();
	}

	fn schedule_flush(self: &Arc<Self>) {
		let (sleep, abort, registration) = {
			let mut accesses = self.cache.accesses.lock();
			if accesses.pending.is_empty() || accesses.timer.is_some() {
				return;
			}
			let (abort, registration) = AbortHandle::new_pair();
			accesses.timer = Some(abort.clone());
			(self.cache.timer.sleep(ACCESS_FLUSH_DELAY), abort, registration)
		};
		let profiles = Arc::downgrade(self);
		self.cache.spawner.spawn(async move {
			if Abortable::new(sleep, registration).await.is_err() {
				return;
			}
			let Some(profiles) = profiles.upgrade() else { return };
			let _writes = profiles.cache.writes.lock().await;
			if !abort.is_aborted() &&
				let Err(error) = profiles.persist(None).await
			{
				warn!(%error, "profile access update failed");
			}
		});
	}

	async fn persist(
		self: &Arc<Self>,
		record: Option<(String, ProfileRecord, u64)>,
	) -> Result<(), DbError> {
		let profiles = self.clone();
		let result = self
			.cache
			.db()?
			.write(move |tx| {
				let mut batch: Vec<_> = {
					let mut accesses = profiles.cache.accesses.lock();
					if let Some(timer) = accesses.timer.take() {
						timer.abort();
					}
					accesses
						.pending
						.iter()
						.map(|(username, sequence)| (username.clone(), *sequence))
						.collect()
				};
				batch.sort_unstable_by_key(|(_, sequence)| *sequence);
				for (username, _) in &batch {
					touch_record(tx, username)?;
				}
				if let Some((username, record, expires_at)) = record {
					store_record(tx, &username, &record, expires_at)?;
				}
				Ok(batch)
			})
			.await;
		{
			let mut accesses = self.cache.accesses.lock();
			if let Ok(batch) = &result {
				for (username, sequence) in batch {
					if accesses.pending.get(username) == Some(sequence) {
						accesses.pending.remove(username);
					}
				}
			}
			if (result.is_err() || accesses.pending.is_empty()) &&
				let Some(timer) = accesses.timer.take()
			{
				timer.abort();
			}
		}
		self.schedule_flush();
		result.map(|_| ())
	}
}

impl Drop for Cache {
	fn drop(&mut self) {
		if let Some(timer) = self.accesses.get_mut().timer.take() {
			timer.abort();
		}
	}
}

#[cfg(test)]
mod tests;
