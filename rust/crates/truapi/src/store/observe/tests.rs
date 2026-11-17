use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures::{FutureExt, executor::block_on};
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
	let CountedNotes { mut stream, runs, .. } = counted_notes(&db);
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

	assert_eq!(tables(&db, "SELECT * FROM ledger_children").unwrap(), vec!["child", "ledger"]);
}

#[test]
fn detects_the_table_of_count_star() {
	// `count(*)` reads no column and reports its table with no database.
	let dir = tempfile::tempdir().unwrap();
	let db = open(&dir);

	assert_eq!(tables(&db, "SELECT count(*) FROM ledger").unwrap(), vec!["ledger"]);
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

	assert!(matches!(next(&mut stream), Some(Err(DbError::Unobservable("SELECT 1")))));
	assert!(next(&mut stream).is_none());
}

#[test]
fn invalid_sql_ends_the_stream() {
	let dir = tempfile::tempdir().unwrap();
	let db = open(&dir);
	let mut stream =
		db.observe("SELECT note FROM missing", |q| q.query_map([], |row| row.get::<_, String>(0)));

	assert!(matches!(next(&mut stream), Some(Err(DbError::Sqlite(_)))));
	assert!(next(&mut stream).is_none());
}

#[test]
fn a_failed_requery_yields_err_and_keeps_observing() {
	let dir = tempfile::tempdir().unwrap();
	let db = open(&dir);
	let CountedNotes { mut stream, fail, .. } = counted_notes(&db);
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
	let CountedNotes { mut stream, runs, fail } = counted_notes(&db);
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
	let mut optional =
		db.observe(NOTES_SQL, |q| q.query_optional([], |row| row.get::<_, String>(0)));
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
		let mut stmt =
			conn.prepare("SELECT name FROM pragma_table_list WHERE schema = 'main' AND wr = 1")?;
		let rows = stmt.query_map([], |row| row.get(0))?;
		Ok(rows.collect::<Result<_, _>>()?)
	}))
	.unwrap();

	assert_eq!(without_rowid, Vec::<String>::new());
}
