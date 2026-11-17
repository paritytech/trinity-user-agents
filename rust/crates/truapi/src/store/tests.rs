use futures::executor::block_on;
use rusqlite_migration::M;

use super::*;

fn migrations() -> Migrations<'static> {
	Migrations::new(vec![
		M::up("CREATE TABLE ledger (id INTEGER PRIMARY KEY, note TEXT NOT NULL)"),
		M::up("ALTER TABLE ledger ADD COLUMN amount INTEGER NOT NULL DEFAULT 0"),
	])
}

fn file_config(dir: &tempfile::TempDir) -> DbConfig {
	DbConfig { location: DbLocation::File(dir.path().join("core.sqlite3")), migrations, readers: 2 }
}

fn memory_config() -> DbConfig {
	DbConfig { location: DbLocation::Memory, migrations, readers: 2 }
}

fn insert(db: &Db, note: &'static str) -> Result<(), DbError> {
	block_on(db.write(move |tx| {
		tx.execute("INSERT INTO ledger (note) VALUES (?1)", [note])?;
		Ok(())
	}))
}

fn notes(db: &Db) -> Vec<String> {
	block_on(db.read(|conn| {
		let mut stmt = conn.prepare("SELECT note FROM ledger ORDER BY id")?;
		let rows = stmt.query_map([], |row| row.get(0))?;
		Ok(rows.collect::<Result<Vec<String>, _>>()?)
	}))
	.unwrap()
}

#[test]
fn readers_see_what_the_writer_committed() {
	// Durable callers write on the writer and read back through the pool;
	// a commit that the pool cannot see would make recovery act on stale state.
	let dir = tempfile::tempdir().unwrap();
	let db = block_on(Db::open(file_config(&dir))).unwrap();

	insert(&db, "first").unwrap();
	insert(&db, "second").unwrap();

	assert_eq!(notes(&db), vec!["first".to_owned(), "second".to_owned()]);
}

#[test]
fn a_failing_write_leaves_nothing_behind() {
	// Domain rows and ledger rows are registered in one write; a partial
	// commit would leave a transaction without the state it consumes.
	let db = block_on(Db::open(memory_config())).unwrap();

	let result = block_on(db.write(|tx| {
		tx.execute("INSERT INTO ledger (note) VALUES ('orphan')", [])?;
		Err::<(), _>(DbError::Connection("caller gave up".into()))
	}));

	assert!(matches!(result, Err(DbError::Connection(_))));
	assert_eq!(notes(&db), Vec::<String>::new());
}

#[test]
fn a_refused_write_leaves_nothing_behind_and_returns_the_refusal() {
	// A domain hook refuses a registration after the ledger rows are
	// written; the refusal must undo them and still reach the caller.
	let db = block_on(Db::open(memory_config())).unwrap();

	let result = block_on(db.write_unless_refused(|tx| {
		tx.execute("INSERT INTO ledger (note) VALUES ('refused')", [])?;
		Ok(Err::<(), _>("item in use"))
	}));

	assert_eq!(result.unwrap(), Err("item in use"));
	assert_eq!(notes(&db), Vec::<String>::new());
}

#[test]
fn an_accepted_write_unless_refused_commits() {
	let db = block_on(Db::open(memory_config())).unwrap();

	let result = block_on(db.write_unless_refused(|tx| {
		tx.execute("INSERT INTO ledger (note) VALUES ('kept')", [])?;
		Ok(Ok::<_, &str>(()))
	}));

	assert_eq!(result.unwrap(), Ok(()));
	assert_eq!(notes(&db), vec!["kept".to_owned()]);
}

#[test]
fn migrations_run_once_and_survive_reopening() {
	// Reopening an existing file must not re-run migrations or lose rows.
	let dir = tempfile::tempdir().unwrap();
	let db = block_on(Db::open(file_config(&dir))).unwrap();
	insert(&db, "kept").unwrap();
	block_on(db.close()).unwrap();

	let reopened = block_on(Db::open(file_config(&dir))).unwrap();
	let schema_version: u32 = block_on(
		reopened.read(|conn| Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)),
	)
	.unwrap();

	assert_eq!((schema_version, notes(&reopened)), (2, vec!["kept".to_owned()]));
}

#[test]
fn readers_cannot_write() {
	// A read that silently wrote would bypass the single writer and its
	// transaction boundaries.
	let dir = tempfile::tempdir().unwrap();
	let db = block_on(Db::open(file_config(&dir))).unwrap();

	let result = block_on(db.read(|conn| {
		conn.execute("INSERT INTO ledger (note) VALUES ('sneaky')", [])?;
		Ok(())
	}));

	assert!(matches!(result, Err(DbError::Sqlite(_))));
	assert_eq!(notes(&db), Vec::<String>::new());
}

#[test]
fn the_writer_is_durable_and_uses_the_write_ahead_log() {
	// The durable engine records a transaction before broadcasting it, so
	// a commit must survive power loss (synchronous = FULL = 2).
	let dir = tempfile::tempdir().unwrap();
	let db = block_on(Db::open(file_config(&dir))).unwrap();

	let settings: (String, i64, i64) = block_on(db.write(|tx| {
		Ok((
			tx.pragma_query_value(None, "journal_mode", |row| row.get(0))?,
			tx.pragma_query_value(None, "synchronous", |row| row.get(0))?,
			tx.pragma_query_value(None, "foreign_keys", |row| row.get(0))?,
		))
	}))
	.unwrap();

	assert_eq!(settings, ("wal".to_owned(), 2, 1));
}

#[test]
fn status_reports_the_file_schema_and_sqlite_version() {
	// Hosts read this to confirm on a device that the bundled SQLite
	// opened the configured file and applied every migration.
	let dir = tempfile::tempdir().unwrap();
	let db = block_on(Db::open(file_config(&dir))).unwrap();

	let status = block_on(db.status()).unwrap();

	let expected_path = dir.path().canonicalize().unwrap().join("core.sqlite3");
	assert_eq!(
		status,
		DbStatus {
			sqlite_version: rusqlite::version().to_owned(),
			schema_version: 2,
			path: Some(expected_path.to_string_lossy().into_owned()),
		}
	);
}

#[test]
fn a_closed_database_rejects_further_work() {
	let db = block_on(Db::open(memory_config())).unwrap();
	block_on(db.close()).unwrap();

	assert!(matches!(insert(&db, "late"), Err(DbError::Closed)));
}
