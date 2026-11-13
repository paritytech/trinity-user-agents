use rusqlite::Connection;

use crate::store::core_migrations;

/// NFT transfers registered before the column was dropped named their
/// policy. They must keep their plan, and rows without one must stay
/// without.
#[test]
fn rows_keep_their_plan_without_naming_a_policy() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 11).unwrap();
	connection
		.execute_batch(
			"INSERT INTO durable_tx (domain, status, policy_id, policy_params)
			VALUES ('nft', 'PENDING_SUBMISSION', 'nft.scarcity.transfer', X'0102');
			INSERT INTO durable_tx (domain, status) VALUES ('nft', 'PENDING');",
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let mut statement = connection
		.prepare("SELECT domain, status, policy_params FROM durable_tx ORDER BY id")
		.unwrap();
	let rows: Vec<(String, String, Option<Vec<u8>>)> = statement
		.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
		.unwrap()
		.collect::<Result<_, _>>()
		.unwrap();

	assert_eq!(
		rows,
		vec![
			("nft".to_string(), "PENDING_SUBMISSION".to_string(), Some(vec![1, 2])),
			("nft".to_string(), "PENDING".to_string(), None),
		]
	);
	let has_policy_id: bool = connection
		.query_row(
			"SELECT EXISTS (SELECT 1 FROM pragma_table_info('durable_tx') WHERE name = 'policy_id')",
			[],
			|row| row.get(0),
		)
		.unwrap();
	assert!(!has_policy_id);
}
