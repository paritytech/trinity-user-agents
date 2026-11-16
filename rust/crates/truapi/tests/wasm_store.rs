//! Wasm-target checks of the core store, whose closures run in place on one
//! connection there instead of on connection threads.

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use futures::StreamExt;
use rusqlite::types::Value;
use rusqlite_migration::{M, Migrations};
use truapi::store::{Db, DbConfig, DbError, DbLocation, core_migrations};
use wasm_bindgen_test::wasm_bindgen_test;

fn ledger_migrations() -> Migrations<'static> {
	Migrations::new(vec![M::up("CREATE TABLE ledger (id INTEGER PRIMARY KEY, note TEXT NOT NULL)")])
}

async fn open(migrations: fn() -> Migrations<'static>) -> Db {
	let config = DbConfig { location: DbLocation::Memory, migrations, readers: 0 };
	Db::open(config).await.expect("in-memory database opens")
}

fn insert(tx: &rusqlite::Transaction<'_>, note: &str) -> Result<(), DbError> {
	tx.execute("INSERT INTO ledger (note) VALUES (?1)", [note])?;
	Ok(())
}

async fn notes(db: &Db) -> Vec<String> {
	db.read(|conn| {
		let mut statement = conn.prepare("SELECT note FROM ledger ORDER BY id")?;
		let rows = statement.query_map([], |row| row.get(0))?;
		Ok(rows.collect::<Result<_, _>>()?)
	})
	.await
	.expect("ledger is readable")
}

#[wasm_bindgen_test]
async fn the_core_schema_reaches_the_version_its_migrations_define() {
	// Every module built on the store assumes the full core schema, so a
	// migration the browser's SQLite build rejects must fail here.
	let mut plain = rusqlite::Connection::open_in_memory().unwrap();
	core_migrations().to_latest(&mut plain).unwrap();
	let expected: u32 = plain.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();

	let status = open(core_migrations).await.status().await.unwrap();

	assert_eq!(status.schema_version, expected);
}

#[wasm_bindgen_test]
async fn a_refused_write_leaves_only_what_was_committed() {
	// The durable ledger relies on a refusal undoing every row of its write.
	let db = open(ledger_migrations).await;
	db.write(|tx| insert(tx, "committed")).await.unwrap();

	let refused = db
		.write_unless_refused(|tx| {
			insert(tx, "refused")?;
			Ok(Err::<(), _>("refused"))
		})
		.await
		.unwrap();

	assert_eq!((refused, notes(&db).await), (Err("refused"), vec!["committed".to_string()]));
}

#[wasm_bindgen_test]
async fn an_observed_query_emits_again_after_a_commit_to_its_table() {
	// The submission executor starts from an observed query, so a commit that
	// wakes nobody would leave scheduled transactions unbuilt.
	let db = open(ledger_migrations).await;
	let mut counts = db.observe("SELECT COUNT(*) FROM ledger", |statement| {
		statement.query_row([], |row| row.get::<_, i64>(0))
	});
	let before = counts.next().await.unwrap().unwrap();

	db.write(|tx| insert(tx, "first")).await.unwrap();
	let after = counts.next().await.unwrap().unwrap();

	assert_eq!((before, after), (0, 1));
}

#[wasm_bindgen_test]
async fn a_list_parameter_reaches_the_array_module() {
	// NFT queries pass their keys as one `rarray` parameter.
	let db = open(ledger_migrations).await;

	let values: Vec<i64> = db
		.read(|conn| {
			let list = Rc::new(vec![Value::Integer(7), Value::Integer(9)]);
			let mut statement = conn.prepare("SELECT value FROM rarray(?1) ORDER BY value")?;
			let rows = statement.query_map([list], |row| row.get(0))?;
			Ok(rows.collect::<Result<_, _>>()?)
		})
		.await
		.unwrap();

	assert_eq!(values, vec![7, 9]);
}

#[wasm_bindgen_test]
async fn a_closed_database_answers_closed() {
	// An observed stream ends on `Closed`, so any other answer would keep it
	// polling a database that is gone.
	let db = open(ledger_migrations).await;
	db.close().await.unwrap();

	let read = db.read(|_| Ok(())).await;

	assert!(matches!(read, Err(DbError::Closed)));
}

#[wasm_bindgen_test]
async fn a_file_location_is_refused() {
	// Nothing persists in the browser yet, and an in-memory stand-in for a
	// file would lose a durable ledger without saying so.
	let location = DbLocation::File("core.sqlite3".into());
	let config = DbConfig { location, migrations: ledger_migrations, readers: 0 };

	let opened = Db::open(config).await.err();

	assert!(matches!(opened, Some(DbError::Open(_))));
}
