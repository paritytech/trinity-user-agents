use rusqlite::Connection;

use crate::store::core_migrations;

/// A connection migrated to the previous version with two waiting rows, then
/// to the latest.
fn migrated_with_two_rows() -> Connection {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 18).unwrap();
	connection
		.execute_batch(
			"INSERT INTO durable_tx (domain, status, policy_params)
			VALUES ('nft', 'PENDING_SUBMISSION', X'01'), ('nft', 'PENDING_SUBMISSION', X'02');",
		)
		.unwrap();
	core_migrations().to_latest(&mut connection).unwrap();
	connection
}

/// Rows registered before the table existed stay as they were.
#[test]
fn existing_rows_survive_the_migration() {
	let connection = migrated_with_two_rows();
	let mut statement = connection.prepare("SELECT policy_params FROM durable_tx ORDER BY id").unwrap();
	let params: Vec<Vec<u8>> =
		statement.query_map([], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap();

	assert_eq!(params, vec![vec![1], vec![2]]);
}

/// A transaction may wait for an older one.
#[test]
fn a_row_can_depend_on_an_older_row() {
	let connection = migrated_with_two_rows();
	let inserted = connection.execute(
		"INSERT INTO durable_tx_dependency (tx_id, depends_on, on_failure) VALUES (2, 1, 'CASCADE')",
		[],
	);

	assert_eq!(inserted.unwrap(), 1);
}

/// Only an older row can be waited for, so no cycle can be stored.
#[test]
fn a_row_cannot_depend_on_a_younger_row() {
	let connection = migrated_with_two_rows();
	let inserted = connection.execute(
		"INSERT INTO durable_tx_dependency (tx_id, depends_on, on_failure) VALUES (1, 2, 'IGNORE')",
		[],
	);

	assert!(inserted.is_err());
}
