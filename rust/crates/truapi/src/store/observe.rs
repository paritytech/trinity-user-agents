//! Observed queries: a query's result as a stream that re-emits after every
//! commit that changes a table the query reads.
//!
//! The writer's `update_hook` collects the tables a write touches. They are
//! published only after the commit returns, so an observer never re-reads
//! before its reader can see the new rows, and a rolled-back write publishes
//! nothing. Each observer re-runs its query on a reader and emits the result
//! unless the raw rows equal the last emission's.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parking_lot::Mutex;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::types::Value;
use rusqlite::{CachedStatement, Connection, Params, Row};

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
        self.invalidation
            .observers
            .lock()
            .retain(|observer| !observer.wake.is_closed());
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
        Registration {
            wake: receiver,
            invalidation: self.clone(),
        }
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
    let tables: Arc<[String]> = names_read_by(conn, sql)?
        .intersection(&schema_tables(conn)?)
        .cloned()
        .collect();
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
            }
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
    pub fn observe<T, F>(&self, sql: &'static str, query: F) -> BoxStream<'static, Result<T, DbError>>
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
                    }
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
            Ok((_, snapshot)) if last.as_ref() == Some(&snapshot) => (None, Phase::Running { registration, last }),
            Ok((value, snapshot)) => {
                let last = Some(snapshot);
                (Some(Ok(value)), Phase::Running { registration, last })
            }
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

    async fn run_observed<T, F>(&self, sql: &'static str, query: Arc<F>) -> Result<(T, Vec<Value>), DbError>
    where
        T: Send + 'static,
        F: Fn(ObservedStatement<'_, '_>) -> Result<T, DbError> + Send + Sync + 'static,
    {
        self.read(move |conn| {
            let mut snapshot = Vec::new();
            let statement = ObservedStatement {
                stmt: conn.prepare_cached(sql)?,
                snapshot: &mut snapshot,
            };
            let value = query(statement)?;
            Ok((value, snapshot))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use futures::FutureExt;
    use futures::executor::block_on;
    use rusqlite_migration::{M, Migrations};

    use super::*;
    use crate::store::{DbConfig, DbLocation, core_migrations};

    fn migrations() -> Migrations<'static> {
        Migrations::new(vec![M::up(
            "CREATE TABLE ledger (id INTEGER PRIMARY KEY, note TEXT NOT NULL UNIQUE);
             CREATE TABLE other (id INTEGER PRIMARY KEY);
             CREATE TABLE child (
                 id INTEGER PRIMARY KEY,
                 ledger_id INTEGER NOT NULL REFERENCES ledger (id) ON DELETE CASCADE
             );
             CREATE VIEW ledger_children AS
                 SELECT ledger.note, child.id FROM ledger JOIN child ON child.ledger_id = ledger.id;",
        )])
    }

    fn open(dir: &tempfile::TempDir) -> Db {
        block_on(Db::open(DbConfig {
            location: DbLocation::File(dir.path().join("core.sqlite3")),
            migrations,
            readers: 2,
        }))
        .unwrap()
    }

    fn exec(db: &Db, sql: &'static str) {
        block_on(db.write(move |tx| {
            tx.execute_batch(sql)?;
            Ok(())
        }))
        .unwrap();
    }

    const NOTES_SQL: &str = "SELECT note FROM ledger ORDER BY id";

    fn notes(db: &Db) -> BoxStream<'static, Result<Vec<String>, DbError>> {
        db.observe(NOTES_SQL, |q| q.query_map([], |row| row.get(0)))
    }

    fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> Option<Result<T, DbError>> {
        block_on(stream.next())
    }

    fn tables(db: &Db, sql: &'static str) -> Result<Vec<String>, DbError> {
        block_on(db.read(move |conn| resolve_tables(conn, sql))).map(|tables| tables.to_vec())
    }

    /// Whether a write since the last check woke an observer of `sql`. The
    /// wake is queued before `write` returns, so no waiting is needed.
    fn woken(registration: &mut Registration) -> bool {
        registration.wake.try_recv().is_ok()
    }

    /// Notes observed by a query that counts its runs and fails every run
    /// while `fail` is set.
    struct CountedNotes {
        stream: BoxStream<'static, Result<Vec<String>, DbError>>,
        runs: Arc<AtomicUsize>,
        fail: Arc<AtomicBool>,
    }

    fn counted_notes(db: &Db) -> CountedNotes {
        let runs = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(false));
        let (counter, failing) = (runs.clone(), fail.clone());
        let stream = db.observe(NOTES_SQL, move |q| {
            let notes = q.query_map([], |row| row.get(0))?;
            counter.fetch_add(1, Ordering::SeqCst);
            if failing.load(Ordering::SeqCst) {
                return Err(DbError::Connection("flaky".into()));
            }
            Ok(notes)
        });
        CountedNotes { stream, runs, fail }
    }

    /// Polls `stream` until its query has run `run` times, then commits
    /// `then` and returns the next item. That is the result of run `run` if
    /// the stream emitted it, or the result after `then` if it was suppressed.
    fn next_after_run(
        db: &Db,
        stream: &mut BoxStream<'static, Result<Vec<String>, DbError>>,
        runs: &AtomicUsize,
        run: usize,
        then: &'static str,
    ) -> Vec<String> {
        let mut pending = stream.next();
        while runs.load(Ordering::SeqCst) < run {
            if let Some(item) = (&mut pending).now_or_never() {
                return item.unwrap().unwrap();
            }
            std::thread::yield_now();
        }
        exec(db, then);
        block_on(pending).unwrap().unwrap()
    }

    #[test]
    fn emits_the_current_state_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(&db, "INSERT INTO ledger (note) VALUES ('first')");

        assert_eq!(next(&mut notes(&db)).unwrap().unwrap(), vec!["first"]);
    }

    #[test]
    fn emits_again_after_each_commit_to_a_read_table() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut stream = notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), Vec::<String>::new());

        exec(&db, "INSERT INTO ledger (note) VALUES ('first')");
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["first"]);

        exec(&db, "UPDATE ledger SET note = 'renamed'");
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["renamed"]);
    }

    #[test]
    fn ignores_commits_to_other_tables() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut registration = block_on(db.subscribe(NOTES_SQL)).unwrap();

        exec(&db, "INSERT INTO other (id) VALUES (1)");

        assert!(!woken(&mut registration));
    }

    #[test]
    fn emits_nothing_for_a_rolled_back_write() {
        // A registration that rolls back must never reach an observer, and
        // its touched tables must not ride along with the next commit.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut registration = block_on(db.subscribe(NOTES_SQL)).unwrap();

        let result = block_on(db.write(|tx| {
            tx.execute("INSERT INTO ledger (note) VALUES ('orphan')", [])?;
            Err::<(), _>(DbError::Connection("caller gave up".into()))
        }));
        assert!(result.is_err());
        assert!(!woken(&mut registration));

        exec(&db, "INSERT INTO other (id) VALUES (1)");
        assert!(!woken(&mut registration));
    }

    #[test]
    fn conflates_a_burst_into_the_latest_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut registration = block_on(db.subscribe(NOTES_SQL)).unwrap();
        let mut stream = notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), Vec::<String>::new());

        for sql in [
            "INSERT INTO ledger (note) VALUES ('a')",
            "INSERT INTO ledger (note) VALUES ('b')",
            "INSERT INTO ledger (note) VALUES ('c')",
        ] {
            exec(&db, sql);
        }

        assert!(woken(&mut registration));
        assert!(!woken(&mut registration), "three commits queue a single wake");
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn a_commit_that_leaves_the_rows_unchanged_emits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(&db, "INSERT INTO ledger (note) VALUES ('first')");
        let CountedNotes {
            mut stream, runs, ..
        } = counted_notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["first"]);

        // Touches the table without changing what the query reads.
        exec(&db, "UPDATE ledger SET note = note");

        assert_eq!(
            next_after_run(&db, &mut stream, &runs, 2, "INSERT INTO ledger (note) VALUES ('second')"),
            vec!["first", "second"]
        );
    }

    #[test]
    fn detects_every_table_of_a_join_and_subquery() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);

        assert_eq!(
            tables(
                &db,
                "SELECT ledger.note FROM ledger JOIN child ON child.ledger_id = ledger.id
                 WHERE ledger.id IN (SELECT id FROM other)"
            )
            .unwrap(),
            vec!["child", "ledger", "other"]
        );
    }

    #[test]
    fn detects_a_views_underlying_tables() {
        // SQLite also reports the view's own name; only real tables count.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);

        assert_eq!(
            tables(&db, "SELECT * FROM ledger_children").unwrap(),
            vec!["child", "ledger"]
        );
    }

    #[test]
    fn detects_the_table_of_count_star() {
        // `count(*)` reads no column and reports its table with no database.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);

        assert_eq!(
            tables(&db, "SELECT count(*) FROM ledger").unwrap(),
            vec!["ledger"]
        );
    }

    #[test]
    fn an_unconditional_delete_still_notifies() {
        // Without the writer's authorizer SQLite truncates the table and
        // skips `update_hook`. Foreign keys also prevent truncation, so the
        // table here takes part in none.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(&db, "INSERT INTO other (id) VALUES (1), (2)");
        let mut registration = block_on(db.subscribe("SELECT id FROM other")).unwrap();

        exec(&db, "DELETE FROM other");

        assert!(woken(&mut registration));
    }

    #[test]
    fn a_cascade_notifies_the_child_table() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(
            &db,
            "INSERT INTO ledger (id, note) VALUES (1, 'a');
             INSERT INTO child (id, ledger_id) VALUES (10, 1);",
        );
        let mut registration = block_on(db.subscribe("SELECT id FROM child")).unwrap();

        exec(&db, "DELETE FROM ledger WHERE id = 1");

        assert!(woken(&mut registration));
    }

    #[test]
    fn a_replace_notifies() {
        // SQLite never reports the row a REPLACE deletes, but the insert
        // lands in the same table.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(&db, "INSERT INTO ledger (id, note) VALUES (1, 'a')");
        let mut stream = notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["a"]);

        exec(&db, "INSERT OR REPLACE INTO ledger (id, note) VALUES (2, 'a')");
        exec(&db, "INSERT INTO ledger (note) VALUES ('b')");

        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn a_query_reading_no_table_is_unobservable_and_ends() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut stream = db.observe("SELECT 1", |q| q.query_row([], |row| row.get::<_, i64>(0)));

        assert!(matches!(
            next(&mut stream),
            Some(Err(DbError::Unobservable("SELECT 1")))
        ));
        assert!(next(&mut stream).is_none());
    }

    #[test]
    fn invalid_sql_ends_the_stream() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut stream = db.observe("SELECT note FROM missing", |q| {
            q.query_map([], |row| row.get::<_, String>(0))
        });

        assert!(matches!(next(&mut stream), Some(Err(DbError::Sqlite(_)))));
        assert!(next(&mut stream).is_none());
    }

    #[test]
    fn a_failed_requery_yields_err_and_keeps_observing() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let CountedNotes {
            mut stream, fail, ..
        } = counted_notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), Vec::<String>::new());

        fail.store(true, Ordering::SeqCst);
        exec(&db, "INSERT INTO ledger (note) VALUES ('a')");
        assert!(matches!(next(&mut stream), Some(Err(DbError::Connection(_)))));

        fail.store(false, Ordering::SeqCst);
        exec(&db, "INSERT INTO ledger (note) VALUES ('b')");
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn a_success_after_an_error_is_emitted_even_if_unchanged() {
        // The consumer saw an error last; only a value tells it the query
        // recovered, even when the rows equal the last emission's.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        exec(&db, "INSERT INTO ledger (note) VALUES ('a')");
        let CountedNotes {
            mut stream,
            runs,
            fail,
        } = counted_notes(&db);
        assert_eq!(next(&mut stream).unwrap().unwrap(), vec!["a"]);

        fail.store(true, Ordering::SeqCst);
        exec(&db, "UPDATE ledger SET note = note");
        assert!(next(&mut stream).unwrap().is_err());

        fail.store(false, Ordering::SeqCst);
        exec(&db, "UPDATE ledger SET note = note");
        assert_eq!(
            next_after_run(&db, &mut stream, &runs, 3, "INSERT INTO ledger (note) VALUES ('b')"),
            vec!["a"]
        );
    }

    #[test]
    fn query_optional_and_query_row_read_the_first_row() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut optional = db.observe(NOTES_SQL, |q| q.query_optional([], |row| row.get::<_, String>(0)));
        let mut row = db.observe(NOTES_SQL, |q| q.query_row([], |row| row.get::<_, String>(0)));
        assert_eq!(next(&mut optional).unwrap().unwrap(), None);
        assert!(matches!(
            next(&mut row),
            Some(Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)))
        ));

        exec(&db, "INSERT INTO ledger (note) VALUES ('a'), ('b')");

        assert_eq!(next(&mut optional).unwrap().unwrap(), Some("a".to_owned()));
        assert_eq!(next(&mut row).unwrap().unwrap(), "a");
    }

    #[test]
    fn dropping_the_stream_unregisters_it() {
        // Without a later write to prune it, a dropped observer would keep its
        // channel and the waiting task's waker alive.
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut stream = notes(&db);
        next(&mut stream).unwrap().unwrap();

        drop(stream);

        assert!(db.invalidation.observers.lock().is_empty());
    }

    #[test]
    fn a_closed_database_ends_the_stream() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let mut stream = notes(&db);
        next(&mut stream).unwrap().unwrap();

        block_on(db.close()).unwrap();

        assert!(matches!(next(&mut stream), Some(Err(DbError::Closed))));
        assert!(next(&mut stream).is_none());
    }

    #[test]
    fn no_core_table_is_without_rowid() {
        // `update_hook` never fires for a WITHOUT ROWID table, so observers
        // of one would silently miss every change.
        let db = block_on(Db::open(DbConfig {
            location: DbLocation::Memory,
            migrations: core_migrations,
            readers: 1,
        }))
        .unwrap();

        let without_rowid: Vec<String> = block_on(db.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT name FROM pragma_table_list WHERE schema = 'main' AND wr = 1",
            )?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            Ok(rows.collect::<Result<_, _>>()?)
        }))
        .unwrap();

        assert_eq!(without_rowid, Vec::<String>::new());
    }
}
