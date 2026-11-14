use rusqlite::Connection;

use crate::store::core_migrations;

/// Rows written before the column was dropped keep their window: the block
/// search and expiry read the birth number and period.
#[test]
fn rows_keep_their_birth_number_and_period() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 12).unwrap();
	connection
		.execute(
			"INSERT INTO durable_tx (domain, status, tx_hash, birth_number, birth_hash, period)
			VALUES ('nft', 'PENDING', X'01', 100, X'02', 64)",
			[],
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let window: (u64, u64) = connection
		.query_row("SELECT birth_number, period FROM durable_tx", [], |row| {
			Ok((row.get(0)?, row.get(1)?))
		})
		.unwrap();

	assert_eq!(window, (100, 64));
}
