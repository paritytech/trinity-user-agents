//! The `wasm32` backend: one connection in the instance that holds the store,
//! with every closure run in place.
//!
//! A WebAssembly instance has no thread to hand a closure to, so a call
//! occupies the instance until its closure returns, and reads go through the
//! writer. Nothing awaits between the start of a transaction and its commit,
//! so no other task runs inside it. A panic in a closure aborts the instance.

use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use crate::store::{DbConfig, DbError, DbLocation, observe::Invalidation, prepare_writer};

/// The connection, until the database is closed.
#[derive(Clone)]
pub struct Backend {
	connection: Arc<Mutex<Option<Connection>>>,
}

impl Backend {
	/// Opens and prepares an in-memory database. A [`DbLocation::File`] is
	/// refused: nothing persists on this target, and an in-memory stand-in
	/// would lose what a caller believes is on disk.
	pub async fn open(config: &DbConfig, invalidation: Arc<Invalidation>) -> Result<Self, DbError> {
		if !matches!(config.location, DbLocation::Memory) {
			let reason = "only in-memory databases are available on wasm32";
			return Err(DbError::Open(reason.to_owned()));
		}
		let mut connection =
			Connection::open_in_memory().map_err(|error| DbError::Open(error.to_string()))?;
		prepare_writer(&mut connection, config.migrations, invalidation)?;
		Ok(Self { connection: Arc::new(Mutex::new(Some(connection))) })
	}

	/// Runs `f` in place on the connection.
	pub async fn with_writer<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		f(self.connection.lock().as_mut().ok_or(DbError::Closed)?)
	}

	/// Runs `f` in place on the connection, which is the writer.
	pub async fn with_reader<T, F>(&self, f: F) -> Result<T, DbError>
	where
		F: FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
		T: Send + 'static,
	{
		f(self.connection.lock().as_ref().ok_or(DbError::Closed)?)
	}

	/// Closes the connection. Closing a closed database does nothing.
	pub async fn close(&self) -> Result<(), DbError> {
		let Some(connection) = self.connection.lock().take() else {
			return Ok(());
		};
		connection.close().map_err(|(_, error)| DbError::Sqlite(error))
	}
}
