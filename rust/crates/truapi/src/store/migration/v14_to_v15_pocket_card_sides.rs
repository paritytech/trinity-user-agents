//! Schema version 15: a card's back kept beside its front.

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
mod tests;
