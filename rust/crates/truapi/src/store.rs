//! Core-owned SQLite store: one writer connection, a small read-only pool,
//! and async access that works on any executor.
//!
//! Each connection runs on its own thread (via `async-sqlite`), so SQLite work
//! never blocks the runtime's executor. [`Db::observe`] turns a query into a
//! stream that follows every commit.

mod migration;
mod observe;

use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use async_sqlite::{JournalMode, Pool, PoolBuilder};
use rusqlite::{OpenFlags, TransactionBehavior};
use rusqlite_migration::Migrations;

pub use migration::core_migrations;
pub use observe::ObservedStatement;
use observe::{Invalidation, authorize_for_change_tracking, track_changes};

/// Where a database lives.
#[derive(Clone)]
pub enum DbLocation {
	/// A file. Its directory must exist.
	File(PathBuf),
	/// A private in-memory database, for tests.
	Memory,
}

/// Options for [`Db::open`].
#[derive(Clone)]
pub struct DbConfig {
	/// Where the database lives.
	pub location: DbLocation,
	/// Builds the schema migrations, applied in order on the writer before
	/// `open` returns.
	pub migrations: fn() -> Migrations<'static>,
	/// Number of read-only connections. A [`DbLocation::Memory`] database
	/// reads through the writer instead.
	pub readers: usize,
}

/// File name of the core database inside the host-configured directory.
pub const CORE_DB_FILE: &str = "core.sqlite3";

/// Read-only connections opened for the core database.
const CORE_DB_READERS: usize = 2;

/// The core database configuration for a host-provided directory.
pub fn core_db_config(directory: &Path) -> DbConfig {
	DbConfig {
		location: DbLocation::File(directory.join(CORE_DB_FILE)),
		migrations: core_migrations,
		readers: CORE_DB_READERS,
	}
}

/// A core database in a fresh temporary directory, so reads go through the
/// reader pool as they do in production. Dropping the directory deletes it.
#[cfg(test)]
pub fn open_temporary_core_db() -> (tempfile::TempDir, Db) {
	let dir = tempfile::tempdir().expect("temporary directory is created");
	let db = futures::executor::block_on(Db::open(core_db_config(dir.path())))
		.expect("core database opens");
	(dir, db)
}

/// A core database in memory, private to the caller.
#[cfg(test)]
pub fn open_memory_core_db() -> Db {
	let config = DbConfig { location: DbLocation::Memory, migrations: core_migrations, readers: 1 };
	futures::executor::block_on(Db::open(config)).expect("core database opens")
}

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
	/// The database could not be opened or configured.
	#[error("database open failed: {0}")]
	Open(String),
	/// A migration failed. The schema stays at its previous version.
	#[error("database migration failed: {0}")]
	Migration(String),
	/// SQLite reported an error.
	#[error(transparent)]
	Sqlite(#[from] rusqlite::Error),
	/// A connection worker failed outside SQLite.
	#[error("database connection failed: {0}")]
	Connection(String),
	/// The database was closed.
	#[error("database closed")]
	Closed,
	/// The host configured no database location.
	#[error("no database configured")]
	NotConfigured,
	/// An observed query reads no table, so no commit could ever change it.
	#[error("observed query reads no table: {0}")]
	Unobservable(&'static str),
}

impl From<async_sqlite::Error> for DbError {
	fn from(error: async_sqlite::Error) -> Self {
		match error {
			async_sqlite::Error::Closed => Self::Closed,
			async_sqlite::Error::Rusqlite(error) => Self::Sqlite(error),
			other => Self::Connection(other.to_string()),
		}
	}
}

/// What a database reports about itself.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DbStatus {
	/// Version of the SQLite library serving the database.
	pub sqlite_version: String,
	/// Number of migrations applied (`PRAGMA user_version`).
	pub schema_version: u32,
	/// Absolute file path, or `None` for an in-memory database.
	pub path: Option<String>,
}

/// A handle to one SQLite database. Cloning shares the same connections.
#[derive(Clone)]
pub struct Db {
	writer: Pool,
	readers: Pool,
	invalidation: Arc<Invalidation>,
}

impl Db {
	/// Opens or creates the database, applies the connection settings and runs
	/// pending migrations on the writer.
	pub async fn open(config: DbConfig) -> Result<Self, DbError> {
		let writer = match &config.location {
			DbLocation::File(path) => PoolBuilder::new().path(path).journal_mode(JournalMode::Wal),
			DbLocation::Memory => PoolBuilder::new(),
		}
		.num_conns(1)
		.open()
		.await
		.map_err(|error| DbError::Open(error.to_string()))?;

		let migrations = config.migrations;
		let invalidation = Arc::new(Invalidation::default());
		let hook = invalidation.clone();
		writer
			.conn_mut_and_then(move |conn| {
				conn.pragma_update(None, "synchronous", "FULL")?;
				conn.pragma_update(None, "foreign_keys", true)?;
				conn.pragma_update(None, "temp_store", "MEMORY")?;
				conn.busy_timeout(BUSY_TIMEOUT)?;
				rusqlite::vtab::array::load_module(conn)?;
				match migrations().to_latest(conn) {
					Ok(()) |
					Err(rusqlite_migration::Error::MigrationDefinition(
						rusqlite_migration::MigrationDefinitionError::NoMigrationsDefined,
					)) => Ok(()),
					Err(error) => Err(DbError::Migration(error.to_string())),
				}?;
				track_changes(conn, hook).map_err(DbError::from)
			})
			.await?;

		let readers = match &config.location {
			DbLocation::File(path) => {
				let readers = PoolBuilder::new()
					.path(path)
					.flags(OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
					.num_conns(config.readers)
					.open()
					.await
					.map_err(|error| DbError::Open(error.to_string()))?;
				readers
					.conn_for_each(|conn| {
						conn.busy_timeout(BUSY_TIMEOUT)?;
						rusqlite::vtab::array::load_module(conn)?;
						conn.authorizer(Some(authorize_for_change_tracking))
					})
					.await
					.into_iter()
					.collect::<Result<Vec<()>, _>>()?;
				readers
			},
			DbLocation::Memory => writer.clone(),
		};

		Ok(Self { writer, readers, invalidation })
	}

	/// Runs `f` in one `BEGIN IMMEDIATE` transaction on the writer. Commits
	/// when `f` returns `Ok` and rolls back when it returns `Err`. A commit
	/// wakes the observers of every table it changed.
	pub async fn write<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		self.write_interactive(f).await
	}

	/// Runs `f` in one `BEGIN IMMEDIATE` transaction on the writer, like
	/// [`Self::write`], with the caller's error type. Commits when `f` returns
	/// `Ok`; when it returns `Err`, rolls back and returns that error as it
	/// is, so a hook can refuse a write after inspecting it. A database
	/// failure reaches the caller through `E::from`.
	pub async fn write_interactive<T, E, F>(&self, f: F) -> Result<T, E>
	where
		F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, E> + Send + 'static,
		T: Send + 'static,
		E: From<DbError> + Send + 'static,
	{
		let invalidation = self.invalidation.clone();
		let written = self
			.writer
			.conn_mut_and_then(move |conn| {
				let outcome = in_transaction(conn, f);
				// Publish only once readers can see the rows: `commit_hook`
				// runs before the commit is visible.
				match outcome {
					Ok(_) => invalidation.publish(),
					Err(_) => invalidation.discard(),
				}
				// The pool's own error must convert from its crate's errors,
				// which `E` does not, so `E` travels inside its `Ok`.
				Ok::<_, DbError>(outcome)
			})
			.await;
		written.map_err(E::from)?
	}

	/// Runs `f` on a read-only connection that sees the last committed state.
	pub async fn read<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&rusqlite::Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		self.readers.conn_and_then(f).await
	}

	/// Reports the SQLite version, schema version and file path.
	pub async fn status(&self) -> Result<DbStatus, DbError> {
		self.read(|conn| {
			Ok(DbStatus {
				sqlite_version: rusqlite::version().to_owned(),
				schema_version: conn.pragma_query_value(None, "user_version", |row| row.get(0))?,
				path: conn.path().filter(|path| !path.is_empty()).map(str::to_owned),
			})
		})
		.await
	}

	/// Checkpoints the write-ahead log and closes every connection.
	pub async fn close(&self) -> Result<(), DbError> {
		self.writer
			.conn(|conn| conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())))
			.await?;
		self.readers.close().await?;
		self.writer.close().await?;
		self.invalidation.close();
		Ok(())
	}
}

/// Runs `f` in a transaction that commits only when `f` returns `Ok` and
/// otherwise rolls back when it is dropped. A failure to begin or commit
/// reaches the caller through `E::from`.
fn in_transaction<T, E: From<DbError>>(
	conn: &mut rusqlite::Connection,
	f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, E>,
) -> Result<T, E> {
	let tx = conn
		.transaction_with_behavior(TransactionBehavior::Immediate)
		.map_err(DbError::from)?;
	let value = f(&tx)?;
	tx.commit().map_err(DbError::from)?;
	Ok(value)
}

const BUSY_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);

#[cfg(test)]
mod tests;
