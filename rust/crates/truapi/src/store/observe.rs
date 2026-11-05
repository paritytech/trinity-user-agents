//! Observed queries: a query's result as a stream that re-emits after every
//! commit that changes a table the query reads.
//!
//! The writer's `update_hook` collects the tables a write touches. They are
//! published only after the commit returns, so an observer never re-reads
//! before its reader can see the new rows, and a rolled-back write publishes
//! nothing. Each observer re-runs its query on a reader and emits the result
//! unless the raw rows equal the last emission's.

use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use futures::{
	channel::mpsc,
	stream::{self, BoxStream, StreamExt},
};
use parking_lot::Mutex;
use rusqlite::{
	CachedStatement, Connection, Params, Row,
	hooks::{AuthAction, AuthContext, Authorization},
	types::Value,
};

use super::{Db, DbError};

/// Tables changed by the open write, and the observers waiting on them.
#[derive(Default)]
pub struct Invalidation {
	touched: Mutex<BTreeSet<String>>,
	observers: Mutex<Vec<Observer>>,
	resolved: Mutex<HashMap<&'static str, Arc<[String]>>>,
}

struct Observer {
	tables: Arc<[String]>,
	/// A `channel(0)` holds at most one wake: a full channel means the
	/// observer is already due to re-query, so bursts conflate.
	wake: mpsc::Sender<()>,
}

/// An observer's wake signal. Dropping it is the only way an observer is
/// unregistered, so a dropped stream releases its channel at once.
struct Registration {
	wake: mpsc::Receiver<()>,
	invalidation: Arc<Invalidation>,
}

impl Drop for Registration {
	fn drop(&mut self) {
		self.wake.close();
		self.invalidation.observers.lock().retain(|observer| !observer.wake.is_closed());
	}
}

impl Observer {
	fn reads_any(&self, touched: &BTreeSet<String>) -> bool {
		self.tables.iter().any(|table| touched.contains(table))
	}

	/// Asks the observer to re-query. A full channel already holds a wake,
	/// and a closed one belongs to a registration that is being dropped.
	fn notify(&mut self) {
		let _ = self.wake.try_send(());
	}
}

impl Invalidation {
	/// Records a row change on the writer. Called by `update_hook`.
	pub fn touch(&self, table: &str) {
		self.touched.lock().insert(table.to_ascii_lowercase());
	}

	/// Wakes every observer reading a touched table. Called once the commit
	/// has returned, so the observer's reader sees the new rows.
	pub fn publish(&self) {
		let touched = core::mem::take(&mut *self.touched.lock());
		if touched.is_empty() {
			return;
		}
		for observer in self.observers.lock().iter_mut() {
			if observer.reads_any(&touched) {
				observer.notify();
			}
		}
	}

	/// Registers an observer of `tables`. It stays registered until the
	/// returned registration is dropped.
	fn register(self: &Arc<Self>, tables: Arc<[String]>) -> Registration {
		let (wake, receiver) = mpsc::channel(0);
		self.observers.lock().push(Observer { tables, wake });
		Registration { wake: receiver, invalidation: self.clone() }
	}

	/// Forgets the changes of a write that rolled back.
	pub fn discard(&self) {
		self.touched.lock().clear();
	}

	/// Wakes every observer. Its re-query meets the closed database, which
	/// ends the stream and drops its registration.
	pub fn close(&self) {
		for observer in self.observers.lock().iter_mut() {
			observer.notify();
		}
	}
}

/// The authorizer every connection carries. Answering `Ignore` for a delete
/// still deletes, but row by row: it turns off SQLite's truncate optimisation,
/// which would otherwise run an unconditional `DELETE FROM t` without calling
/// `update_hook`.
pub fn authorize_for_change_tracking(context: AuthContext<'_>) -> Authorization {
	match context.action {
		AuthAction::Delete { .. } => Authorization::Ignore,
		_ => Authorization::Allow,
	}
}

/// Makes the writer report every committed row change to `invalidation`.
pub fn track_changes(conn: &Connection, invalidation: Arc<Invalidation>) -> rusqlite::Result<()> {
	conn.authorizer(Some(authorize_for_change_tracking))?;
	conn.update_hook(Some(move |_, database: &str, table: &str, _| {
		if database == "main" {
			invalidation.touch(table);
		}
	}))
}

/// Lists the schema tables `sql` reads. An observed query that reads none
/// could never be woken, so it is rejected.
fn resolve_tables(conn: &Connection, sql: &'static str) -> Result<Arc<[String]>, DbError> {
	let tables: Arc<[String]> =
		names_read_by(conn, sql)?.intersection(&schema_tables(conn)?).cloned().collect();
	if tables.is_empty() {
		return Err(DbError::Unobservable(sql));
	}
	Ok(tables)
}

/// Every name SQLite reports as read while preparing `sql`. That covers the
/// tables behind a view or a subquery, but also views, CTEs, table-valued
/// functions and SQLite's own tables.
fn names_read_by(conn: &Connection, sql: &str) -> Result<BTreeSet<String>, DbError> {
	let reads = Arc::new(Mutex::new(BTreeSet::new()));
	let recorder = reads.clone();
	conn.authorizer(Some(move |context: AuthContext<'_>| {
		// `count(*)` reports its table with no database name.
		if let (AuthAction::Read { table_name, .. }, None | Some("main")) =
			(context.action, context.database_name)
		{
			recorder.lock().insert(table_name.to_ascii_lowercase());
		}
		authorize_for_change_tracking(context)
	}))?;
	// Uncached: a cached statement is not authorized again.
	let prepared = conn.prepare(sql).map(drop);
	conn.authorizer(Some(authorize_for_change_tracking))?;
	prepared?;
	Ok(core::mem::take(&mut *reads.lock()))
}

/// The schema's tables, without SQLite's internal ones.
fn schema_tables(conn: &Connection) -> Result<BTreeSet<String>, DbError> {
	let mut stmt = conn.prepare_cached(
		"SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
	)?;
	let names = stmt
		.query_map([], |row| row.get::<_, String>(0))?
		.map(|name| name.map(|name| name.to_ascii_lowercase()))
		.collect::<Result<_, _>>()?;
	Ok(names)
}

/// The statement an observed query runs. It records the raw values of every
/// row it reads, so an unchanged result is not emitted again. Running it
/// consumes it: one run per refresh keeps the recorded rows a faithful
/// image of the result.
pub struct ObservedStatement<'c, 's> {
	stmt: CachedStatement<'c>,
	snapshot: &'s mut Vec<Value>,
}

impl ObservedStatement<'_, '_> {
	/// Maps every row with `f`.
	pub fn query_map<T, P, F>(mut self, params: P, mut f: F) -> Result<Vec<T>, DbError>
	where
		P: Params,
		F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
	{
		let columns = self.stmt.column_count();
		let mut rows = self.stmt.query(params)?;
		let mut values = Vec::new();
		while let Some(row) = rows.next()? {
			record(self.snapshot, row, columns)?;
			values.push(f(row)?);
		}
		Ok(values)
	}

	/// Maps the first row with `f`, or returns `None` when there is none.
	pub fn query_optional<T, P, F>(mut self, params: P, f: F) -> Result<Option<T>, DbError>
	where
		P: Params,
		F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
	{
		let columns = self.stmt.column_count();
		let mut rows = self.stmt.query(params)?;
		match rows.next()? {
			Some(row) => {
				record(self.snapshot, row, columns)?;
				Ok(Some(f(row)?))
			},
			None => Ok(None),
		}
	}

	/// Maps the first row with `f`, failing with `QueryReturnedNoRows` when
	/// there is none.
	pub fn query_row<T, P, F>(self, params: P, f: F) -> Result<T, DbError>
	where
		P: Params,
		F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
	{
		self.query_optional(params, f)?
			.ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
	}
}

fn record(snapshot: &mut Vec<Value>, row: &Row<'_>, columns: usize) -> rusqlite::Result<()> {
	for column in 0..columns {
		snapshot.push(row.get::<_, Value>(column)?);
	}
	Ok(())
}

enum Phase {
	Start,
	Running {
		registration: Registration,
		/// Raw rows of the last emission. `None` after an error, so the next
		/// success is emitted even when it equals the value before the error.
		last: Option<Vec<Value>>,
	},
	Done,
}

impl Db {
	/// Streams the result of `query` over `sql`: once when first polled, then
	/// again after every commit that changes a table `sql` reads, unless the
	/// rows read are identical to the last emission. Bursts of commits are
	/// conflated, so the stream emits the latest committed state.
	///
	/// A failed query yields `Err` and keeps observing. The stream ends after
	/// yielding `Err` when its tables can't be resolved (invalid SQL, or
	/// [`DbError::Unobservable`]) and when the database closes.
	pub fn observe<T, F>(
		&self,
		sql: &'static str,
		query: F,
	) -> BoxStream<'static, Result<T, DbError>>
	where
		T: Send + 'static,
		F: Fn(ObservedStatement<'_, '_>) -> Result<T, DbError> + Send + Sync + 'static,
	{
		let db = self.clone();
		let query = Arc::new(query);
		stream::unfold(Phase::Start, move |phase| {
			let db = db.clone();
			let query = query.clone();
			async move {
				let (registration, last) = match phase {
					Phase::Done => return None,
					// Subscribe before the first read, so no commit is missed.
					Phase::Start => match db.subscribe(sql).await {
						Ok(registration) => (registration, None),
						Err(error) => return Some((Some(Err(error)), Phase::Done)),
					},
					Phase::Running { mut registration, last } => {
						registration.wake.next().await?;
						(registration, last)
					},
				};
				Some(db.requery(sql, query, registration, last).await)
			}
		})
		.filter_map(core::future::ready)
		.boxed()
	}

	/// Runs the query once and returns what to emit, `None` when its rows
	/// equal `last`, together with the phase that follows.
	async fn requery<T, F>(
		&self,
		sql: &'static str,
		query: Arc<F>,
		registration: Registration,
		last: Option<Vec<Value>>,
	) -> (Option<Result<T, DbError>>, Phase)
	where
		T: Send + 'static,
		F: Fn(ObservedStatement<'_, '_>) -> Result<T, DbError> + Send + Sync + 'static,
	{
		match self.run_observed(sql, query).await {
			Ok((_, snapshot)) if last.as_ref() == Some(&snapshot) => {
				(None, Phase::Running { registration, last })
			},
			Ok((value, snapshot)) => {
				let last = Some(snapshot);
				(Some(Ok(value)), Phase::Running { registration, last })
			},
			Err(DbError::Closed) => (Some(Err(DbError::Closed)), Phase::Done),
			Err(error) => (Some(Err(error)), Phase::Running { registration, last: None }),
		}
	}

	async fn subscribe(&self, sql: &'static str) -> Result<Registration, DbError> {
		let tables = self.tables(sql).await?;
		Ok(self.invalidation.register(tables))
	}

	/// The tables `sql` reads, resolved once per SQL constant: migrations run
	/// only in `open`, so the schema can't change under a cached answer.
	async fn tables(&self, sql: &'static str) -> Result<Arc<[String]>, DbError> {
		if let Some(tables) = self.invalidation.resolved.lock().get(sql) {
			return Ok(tables.clone());
		}
		let tables = self.readers.conn_and_then(move |conn| resolve_tables(conn, sql)).await?;
		self.invalidation.resolved.lock().insert(sql, tables.clone());
		Ok(tables)
	}

	async fn run_observed<T, F>(
		&self,
		sql: &'static str,
		query: Arc<F>,
	) -> Result<(T, Vec<Value>), DbError>
	where
		T: Send + 'static,
		F: Fn(ObservedStatement<'_, '_>) -> Result<T, DbError> + Send + Sync + 'static,
	{
		self.read(move |conn| {
			let mut snapshot = Vec::new();
			let statement =
				ObservedStatement { stmt: conn.prepare_cached(sql)?, snapshot: &mut snapshot };
			let value = query(statement)?;
			Ok((value, snapshot))
		})
		.await
	}
}

#[cfg(test)]
mod tests;
