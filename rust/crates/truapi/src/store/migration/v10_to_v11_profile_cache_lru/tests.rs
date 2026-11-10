use rusqlite::Connection;

use crate::store::core_migrations;

#[test]
fn profiles_keep_records_and_freshness_when_access_order_is_added() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 10).unwrap();
	connection
		.execute_batch(
			"INSERT INTO public_profiles VALUES ('bob.02', X'02', 200);
			INSERT INTO public_profiles VALUES ('alice.01', X'01', 100);
			INSERT INTO public_profiles VALUES ('carol.03', X'03', 200);
			INSERT INTO pocket_card VALUES ('test.dot', 'card', 'Kept card', 1, 2);",
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	let profiles: Vec<(String, Vec<u8>, u64, u64)> = connection
		.prepare("SELECT username, public_record, expires_at, access_order FROM public_profiles ORDER BY access_order")
		.unwrap()
		.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
		.unwrap()
		.collect::<Result<_, _>>()
		.unwrap();
	let card: String = connection
		.query_row("SELECT title FROM pocket_card WHERE card_id = 'card'", [], |row| row.get(0))
		.unwrap();

	assert_eq!(
		(profiles, card),
		(
			vec![
				("alice.01".to_owned(), vec![1], 100, 1),
				("bob.02".to_owned(), vec![2], 200, 2),
				("carol.03".to_owned(), vec![3], 200, 3),
			],
			"Kept card".to_owned(),
		),
	);
}
