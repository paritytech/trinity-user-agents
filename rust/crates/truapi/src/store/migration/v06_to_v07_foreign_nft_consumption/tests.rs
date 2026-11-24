use rusqlite::Connection;

use crate::store::core_migrations;

/// An import spends a key from a cheque, which is in no purse, so the
/// table may only reference the transaction that spends it.
#[test]
fn a_spent_key_need_not_be_one_of_ours() {
	let mut conn = Connection::open_in_memory().unwrap();
	core_migrations().to_latest(&mut conn).unwrap();

	let mut statement = conn
		.prepare("SELECT \"table\" FROM pragma_foreign_key_list('nft_consumption')")
		.unwrap();
	let referenced: Vec<String> = statement
		.query_map([], |row| row.get(0))
		.unwrap()
		.collect::<Result<_, _>>()
		.unwrap();

	assert_eq!(referenced, vec!["durable_tx".to_owned()]);
}
