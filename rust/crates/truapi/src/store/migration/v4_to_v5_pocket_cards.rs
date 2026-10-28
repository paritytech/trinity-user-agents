//! Schema version 5: the Pocket collection and the newest face of each card.

use rusqlite_migration::M;

/// Creates `pocket_card`, one row per card in the Pocket, and
/// `pocket_card_face`, the newest face drawn for a card. `added_at` is NULL on
/// a card that is there only because the host pins it. Faces have their own
/// table so a face write does not wake card-list observers, and they go with
/// their card.
pub fn create_pocket_cards() -> M<'static> {
	M::up(
		"CREATE TABLE pocket_card (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			title TEXT NOT NULL,
			privileged INTEGER NOT NULL DEFAULT 0 CHECK (privileged IN (0, 1)),
			added_at INTEGER,
			PRIMARY KEY (product_id, card_id)
		);
		CREATE TABLE pocket_card_face (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			face BLOB NOT NULL,
			PRIMARY KEY (product_id, card_id),
			FOREIGN KEY (product_id, card_id) REFERENCES pocket_card (product_id, card_id)
				ON DELETE CASCADE
		);",
	)
}
