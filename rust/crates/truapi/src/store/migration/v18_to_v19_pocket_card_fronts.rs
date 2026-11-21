//! Schema version 19: a card keeps one face.

use rusqlite_migration::M;

/// Recreates `pocket_card_face` keyed by card alone, keeping each card's
/// front. A card draws both its faces in one tree, so a kept back is never
/// read again.
pub fn keep_pocket_card_fronts_only() -> M<'static> {
	M::up(
		"CREATE TABLE pocket_card_front_face (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			face BLOB NOT NULL,
			draw_order INTEGER NOT NULL,
			PRIMARY KEY (product_id, card_id),
			FOREIGN KEY (product_id, card_id) REFERENCES pocket_card (product_id, card_id)
				ON DELETE CASCADE
		);
		INSERT INTO pocket_card_front_face (product_id, card_id, face, draw_order)
			SELECT product_id, card_id, face, draw_order FROM pocket_card_face WHERE side = 'FRONT';
		DROP TABLE pocket_card_face;
		ALTER TABLE pocket_card_front_face RENAME TO pocket_card_face;",
	)
}

#[cfg(test)]
mod tests {
	use rusqlite::Connection;

	use crate::store::core_migrations;

	/// The front is what the card shows, so the upgrade keeps it and drops
	/// only the back.
	#[test]
	fn a_card_keeps_its_front_and_loses_its_back() {
		let mut conn = Connection::open_in_memory().unwrap();
		let migrations = core_migrations();
		migrations.to_version(&mut conn, 18).unwrap();
		conn.execute_batch(
			"INSERT INTO pocket_card (product_id, card_id, title) VALUES ('loyalty.dot', 'stamps', 'Stamps');
			INSERT INTO pocket_card_face (product_id, card_id, side, face, draw_order)
				VALUES ('loyalty.dot', 'stamps', 'FRONT', x'01', 7), ('loyalty.dot', 'stamps', 'BACK', x'02', 8);",
		)
		.unwrap();

		migrations.to_latest(&mut conn).unwrap();

		let mut kept = conn.prepare("SELECT card_id, face, draw_order FROM pocket_card_face").unwrap();
		let rows: Vec<(String, Vec<u8>, i64)> = kept
			.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
			.unwrap()
			.collect::<Result<_, _>>()
			.unwrap();
		assert_eq!(rows, vec![("stamps".to_string(), vec![1], 7)]);
	}
}
