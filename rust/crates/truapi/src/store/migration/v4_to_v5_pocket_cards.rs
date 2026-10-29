//! Schema version 5: the Pocket collection and the newest face of each card.

use rusqlite_migration::M;

/// Creates `pocket_card`, one row per card in the Pocket, and
/// `pocket_card_face`, the newest face drawn for a card. `pin` is the card's
/// place among the host's pins, NULL when the host does not pin it, and
/// `added_order` is the order the user added cards in, NULL on a card that is
/// there only because the host pins it.
/// Faces have their own table so a face write does not wake card-list
/// observers, and they go with their card. `draw_order` orders faces from
/// renders of one card that overlap.
pub fn create_pocket_cards() -> M<'static> {
	M::up(
		"CREATE TABLE pocket_card (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			title TEXT NOT NULL,
			pin INTEGER,
			added_order INTEGER,
			PRIMARY KEY (product_id, card_id)
		);
		CREATE TABLE pocket_card_face (
			product_id TEXT NOT NULL,
			card_id TEXT NOT NULL,
			face BLOB NOT NULL,
			draw_order INTEGER NOT NULL,
			PRIMARY KEY (product_id, card_id),
			FOREIGN KEY (product_id, card_id) REFERENCES pocket_card (product_id, card_id)
				ON DELETE CASCADE
		);",
	)
}
