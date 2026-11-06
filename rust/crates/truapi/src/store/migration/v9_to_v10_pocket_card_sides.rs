//! Schema version 10: a card's back kept beside its front.

use rusqlite_migration::M;

/// Recreates `pocket_card_face` keyed by `side` as well, so a card keeps the
/// newest face drawn for its front and for its back. Faces kept before are
/// fronts.
pub fn key_pocket_card_faces_by_side() -> M<'static> {
	M::up(
		"CREATE TABLE pocket_card_side_face (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			side TEXT NOT NULL CHECK (side IN ('FRONT', 'BACK')),
			face BLOB NOT NULL,
			draw_order INTEGER NOT NULL,
			PRIMARY KEY (product_id, card_id, side),
			FOREIGN KEY (product_id, card_id) REFERENCES pocket_card (product_id, card_id)
				ON DELETE CASCADE
		);
		INSERT INTO pocket_card_side_face (product_id, card_id, side, face, draw_order)
			SELECT product_id, card_id, 'FRONT', face, draw_order FROM pocket_card_face;
		DROP TABLE pocket_card_face;
		ALTER TABLE pocket_card_side_face RENAME TO pocket_card_face;",
	)
}

#[cfg(test)]
mod tests {
	use rusqlite::Connection;

	use crate::store::core_migrations;

	/// A face kept before cards had a back is what the card still shows, so
	/// the upgrade must keep it as the front rather than drop it.
	#[test]
	fn a_face_kept_before_backs_stays_the_front() {
		let mut conn = Connection::open_in_memory().unwrap();
		let migrations = core_migrations();
		migrations.to_version(&mut conn, 9).unwrap();
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
}
