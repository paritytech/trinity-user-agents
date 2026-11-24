//! Schema version 3: the NFT purse table, holding the main purse from the
//! start.

use rusqlite_migration::M;

/// Creates `nft_purse` with the main purse, id `u32::MAX` under the empty tag.
pub fn create_nft_purse() -> M<'static> {
	M::up(
		"CREATE TABLE nft_purse (
			id INTEGER PRIMARY KEY CHECK (id BETWEEN 0 AND 4294967295),
			name TEXT NOT NULL,
			tag BLOB NOT NULL
		);
		CREATE INDEX nft_purse_tag ON nft_purse(tag);
		INSERT INTO nft_purse (id, name, tag) VALUES (4294967295, 'Main', x'');",
	)
}
