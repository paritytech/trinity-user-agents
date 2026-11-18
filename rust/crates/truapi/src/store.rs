//! Core-owned SQLite store: one writer, read access that sees the last
//! commit, and async calls that work on any executor.
//!
//! [`Db`] is the same on every target. Where its closures run is the
//! backend's: on native targets each connection has its own thread (via
//! `async-sqlite`), so SQLite work never blocks the runtime's executor, and
//! reads go through a small read-only pool. On `wasm32` there are no threads,
//! so one connection in the calling instance runs each closure in place.
//! [`Db::observe`] turns a query into a stream that follows every commit.

mod backend;
mod migration;
mod observe;

use core::convert::Infallible;
use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use rusqlite::TransactionBehavior;
use rusqlite_migration::Migrations;

use backend::Backend;
pub use migration::core_migrations;
pub use observe::ObservedStatement;
use observe::{Invalidation, track_changes};

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
}

/// File name of the core database inside the host-configured directory.
pub const CORE_DB_FILE: &str = "core.sqlite3";

/// The core database configuration for a host-provided directory.
pub fn core_db_config(directory: &Path) -> DbConfig {
	DbConfig { location: DbLocation::File(directory.join(CORE_DB_FILE)), migrations: core_migrations }
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
	let config = DbConfig { location: DbLocation::Memory, migrations: core_migrations };
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

/// What a database reports about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
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
	backend: Backend,
	invalidation: Arc<Invalidation>,
}

impl Db {
	/// Opens or creates the database, applies the connection settings and runs
	/// pending migrations on the writer.
	pub async fn open(config: DbConfig) -> Result<Self, DbError> {
		let invalidation = Arc::new(Invalidation::default());
		let backend = Backend::open(&config, invalidation.clone()).await?;
		Ok(Self { backend, invalidation })
	}

	/// Runs `f` in one `BEGIN IMMEDIATE` transaction on the writer. Commits
	/// when `f` returns `Ok` and rolls back when it returns `Err`. A commit
	/// wakes the observers of every table it changed.
	pub async fn write<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		let never_refused = move |tx: &rusqlite::Transaction<'_>| Ok(Ok::<_, Infallible>(f(tx)?));
		Ok(self
			.write_unless_refused(never_refused)
			.await?
			.unwrap_or_else(|never| match never {}))
	}

	/// Runs `f` in one `BEGIN IMMEDIATE` transaction on the writer, like
	/// [`Self::write`], and commits only when `f` returns `Ok(Ok(_))`. An
	/// inner `Err` is a refusal: the transaction rolls back and the refusal is
	/// returned as it is.
	pub async fn write_unless_refused<T, E, F>(&self, f: F) -> Result<Result<T, E>, DbError>
	where
		F: FnOnce(&rusqlite::Transaction<'_>) -> Result<Result<T, E>, DbError> + Send + 'static,
		T: Send + 'static,
		E: Send + 'static,
	{
		let invalidation = self.invalidation.clone();
		self.backend
			.with_writer(move |conn| {
				let outcome = in_transaction_unless_refused(conn, f);
				// Publish only once readers can see the rows: `commit_hook`
				// runs before the commit is visible.
				match outcome {
					Ok(Ok(_)) => invalidation.publish(),
					Ok(Err(_)) | Err(_) => invalidation.discard(),
				}
				outcome
			})
			.await
	}

	/// Runs `f` on a connection that sees the last committed state. It is
	/// read-only wherever the backend keeps a separate one for reads.
	pub async fn read<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&rusqlite::Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		self.backend.with_reader(f).await
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

	/// Closes every connection, after checkpointing the write-ahead log where
	/// the backend keeps one.
	pub async fn close(&self) -> Result<(), DbError> {
		self.backend.close().await?;
		self.invalidation.close();
		Ok(())
	}
}

/// Applies the settings every writer carries, runs the pending `migrations`
/// and makes the writer report its committed changes to `invalidation`.
fn prepare_writer(
	conn: &mut rusqlite::Connection,
	migrations: fn() -> Migrations<'static>,
	invalidation: Arc<Invalidation>,
) -> Result<(), DbError> {
	conn.pragma_update(None, "synchronous", "FULL")?;
	conn.pragma_update(None, "foreign_keys", true)?;
	conn.pragma_update(None, "temp_store", "MEMORY")?;
	rusqlite::vtab::array::load_module(conn)?;
	match migrations().to_latest(conn) {
		Ok(()) |
		Err(rusqlite_migration::Error::MigrationDefinition(
			rusqlite_migration::MigrationDefinitionError::NoMigrationsDefined,
		)) => Ok(()),
		Err(error) => Err(DbError::Migration(error.to_string())),
	}?;
	track_changes(conn, invalidation).map_err(DbError::from)
}

/// Runs `f` in a transaction that commits only when `f` returns `Ok(Ok(_))`
/// and otherwise rolls back when it is dropped.
fn in_transaction_unless_refused<T, E>(
	conn: &mut rusqlite::Connection,
	f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<Result<T, E>, DbError>,
) -> Result<Result<T, E>, DbError> {
	let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
	let value = match f(&tx)? {
		Ok(value) => value,
		Err(refusal) => return Ok(Err(refusal)),
	};
	tx.commit()?;
	Ok(Ok(value))
}

#[cfg(test)]
mod tests;
