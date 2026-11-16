//! The native backend: each connection runs on its own thread through
//! `async-sqlite`, with one writer in write-ahead-log mode and a read-only
//! pool beside it.

use std::sync::Arc;

use async_sqlite::{JournalMode, Pool, PoolBuilder};
use rusqlite::{Connection, OpenFlags};

use crate::store::{
	DbConfig, DbError, DbLocation,
	observe::{Invalidation, authorize_for_change_tracking},
	prepare_writer,
};

/// The writer and the connections reads go through.
#[derive(Clone)]
pub struct Backend {
	writer: Pool,
	readers: Pool,
}

impl Backend {
	/// Opens and prepares the writer, then the read-only pool. A
	/// [`DbLocation::Memory`] database reads through the writer.
	pub async fn open(config: &DbConfig, invalidation: Arc<Invalidation>) -> Result<Self, DbError> {
		let writer = match &config.location {
			DbLocation::File(path) => PoolBuilder::new().path(path).journal_mode(JournalMode::Wal),
			DbLocation::Memory => PoolBuilder::new(),
		}
		.num_conns(1)
		.open()
		.await
		.map_err(|error| DbError::Open(error.to_string()))?;

		let migrations = config.migrations;
		writer
			.conn_mut_and_then(move |conn| {
				conn.busy_timeout(BUSY_TIMEOUT)?;
				prepare_writer(conn, migrations, invalidation)
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

		Ok(Self { writer, readers })
	}

	/// Runs `f` on the writer's thread.
	pub async fn with_writer<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		self.writer.conn_mut_and_then(f).await
	}

	/// Runs `f` on the thread of a read-only connection.
	pub async fn with_reader<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		self.readers.conn_and_then(f).await
	}

	/// Checkpoints the write-ahead log and closes every connection.
	pub async fn close(&self) -> Result<(), DbError> {
		self.writer
			.conn(|conn| conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())))
			.await?;
		self.readers.close().await?;
		self.writer.close().await?;
		Ok(())
	}
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

const BUSY_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);
