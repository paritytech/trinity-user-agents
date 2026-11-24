use rusqlite::Connection;

use crate::store::core_migrations;

/// A face kept before cards had a back is what the card still shows, so
/// the upgrade must keep it as the front rather than drop it.
#[test]
fn a_face_kept_before_backs_stays_the_front() {
	let mut conn = Connection::open_in_memory().unwrap();
	let migrations = core_migrations();
	migrations.to_version(&mut conn, 14).unwrap();
	conn.execute_batch(
		"INSERT INTO pocket_card (product_id, card_id, title) VALUES ('loyalty.dot', 'stamps', 'Stamps');
		INSERT INTO pocket_card_face (product_id, card_id, face, draw_order)
			VALUES ('loyalty.dot', 'stamps', x'01', 7);",
	)
	.unwrap();

	migrations.to_latest(&mut conn).unwrap();

	let kept: (String, Vec<u8>, i64) = conn
		.query_row("SELECT side, face, draw_order FROM pocket_card_face", [], |row| {
			Ok((row.get(0)?, row.get(1)?, row.get(2)?))
		})
		.unwrap();
	assert_eq!(kept, ("FRONT".to_string(), vec![1], 7));
}
