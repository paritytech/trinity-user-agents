use rusqlite::Connection;

use crate::store::core_migrations;

#[test]
fn profiles_extend_a_pocket_database_without_losing_cards() {
	let mut connection = Connection::open_in_memory().unwrap();
	core_migrations().to_version(&mut connection, 9).unwrap();
	connection
		.execute_batch(
			"INSERT INTO funding_session VALUES ('kept', X'0102');
			INSERT INTO pocket_card VALUES ('test.dot', 'card', 'Kept card', 1, 2);
			INSERT INTO pocket_card_face VALUES ('test.dot', 'card', X'0304', 5);",
		)
		.unwrap();

	core_migrations().to_latest(&mut connection).unwrap();
	connection
		.execute("INSERT INTO public_profiles VALUES ('alice.01', X'00', 3600)", [])
		.unwrap();
	let session: Vec<u8> = connection
		.query_row("SELECT record FROM funding_session WHERE intent = 'kept'", [], |row| row.get(0))
		.unwrap();
	let card: (String, u64, u64, Vec<u8>, u64) = connection
		.query_row(
			"SELECT title, pin, added_order, face, draw_order
			FROM pocket_card JOIN pocket_card_face USING (product_id, card_id)
			WHERE product_id = 'test.dot' AND card_id = 'card'",
			[],
			|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
		)
		.unwrap();
	let profile: (Vec<u8>, u64) = connection
		.query_row(
			"SELECT public_record, expires_at FROM public_profiles WHERE username = 'alice.01'",
			[],
			|row| Ok((row.get(0)?, row.get(1)?)),
		)
		.unwrap();

	assert_eq!(session, vec![1, 2]);
	assert_eq!(card, ("Kept card".to_string(), 1, 2, vec![3, 4], 5));
	assert_eq!(profile, (vec![0], 3600));
}
