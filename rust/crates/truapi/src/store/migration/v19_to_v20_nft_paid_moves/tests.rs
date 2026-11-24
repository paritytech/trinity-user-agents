use rusqlite::Connection;

use crate::store::core_migrations;

/// A transfer registered before the update keeps decoding: its params gain
/// the fee-less mode as a trailing byte and stay a BLOB, which is the only
/// type the durable store reads params as.
#[test]
fn stored_nft_params_gain_the_fee_less_mode_as_a_blob() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 19).unwrap();
	connection
		.execute(
			"INSERT INTO durable_tx (domain, status, policy_params)
			VALUES ('nft', 'PENDING_SUBMISSION', X'0102')",
			[],
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let params: Vec<u8> = connection
		.query_row("SELECT policy_params FROM durable_tx", [], |row| row.get(0))
		.unwrap();

	assert_eq!(params, vec![0x01, 0x02, 0x00]);
}

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
