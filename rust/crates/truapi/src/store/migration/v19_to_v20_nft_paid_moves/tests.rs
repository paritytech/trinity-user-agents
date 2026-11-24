use rusqlite::Connection;

use crate::store::core_migrations;

/// Params of other domains are not NFT transfers and stay as they were.
#[test]
fn params_of_other_domains_are_untouched() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 19).unwrap();
	connection
		.execute(
			"INSERT INTO durable_tx (domain, status, policy_params)
			VALUES ('pgas', 'PENDING_SUBMISSION', X'0102')",
			[],
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let params: Vec<u8> = connection
		.query_row("SELECT policy_params FROM durable_tx", [], |row| row.get(0))
		.unwrap();

	assert_eq!(params, vec![0x01, 0x02]);
}

/// A holding recorded before the update has no moves until the tracker
/// writes it again, so registration cannot mistake it for an NFT that used
/// none.
#[test]
fn existing_holdings_have_no_moves_recorded() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 19).unwrap();
	connection
		.execute_batch(
			"INSERT INTO nft_purse (id, name, tag) VALUES (0, 'tickets', X'');
			INSERT INTO nft_item (key, purse, installation, idx) VALUES (zeroblob(32), 0, zeroblob(32), 0);
			INSERT INTO nft_holding (key, collection, item, instance) VALUES (zeroblob(32), 1, 1, 7);",
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let moves: Option<u16> = connection
		.query_row("SELECT moves FROM nft_holding", [], |row| row.get(0))
		.unwrap();

	assert_eq!(moves, None);
}
