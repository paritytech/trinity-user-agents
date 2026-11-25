use rusqlite::{Connection, params};

use crate::store::core_migrations;

/// The schema with every migration applied, with foreign keys off so a
/// row needs no parent and only the checks are tested.
fn migrated() -> Connection {
	let mut conn = Connection::open_in_memory().unwrap();
	core_migrations().to_latest(&mut conn).unwrap();
	conn.pragma_update(None, "foreign_keys", false).unwrap();
	conn
}

/// Whether a row of `kind` with `tx` and `handoff_tag` is accepted.
fn accepts(conn: &Connection, kind: &str, tx: Option<i64>, handoff_tag: Option<&[u8]>) -> bool {
	let insert = "INSERT INTO nft_consumption (key, kind, tx, handoff_tag) VALUES (?1, ?2, ?3, ?4)";
	conn.execute(insert, params![[0x01_u8; 32], kind, tx, handoff_tag]).is_ok()
}

/// The items of a revoked cheque are found by its tag, so a handoff
/// without one could never be found again, and SQLite cannot add the
/// check to the table later.
#[test]
fn a_handoff_carries_a_tag_and_an_input_carries_none() {
	let conn = migrated();

	assert!(accepts(&conn, "HANDOFF", None, Some(b"cheque")));
	assert!(!accepts(&conn, "HANDOFF", None, None));
	assert!(!accepts(&conn, "INPUT", Some(1), Some(b"cheque")));
}
