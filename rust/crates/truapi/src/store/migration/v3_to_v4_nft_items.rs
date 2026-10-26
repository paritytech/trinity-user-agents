//! Schema version 4: item keys of NFT purses, and what each key holds on
//! chain.

use rusqlite_migration::M;

/// Creates `nft_item`, one row per allocated key, and `nft_holding`, the
/// tracker's cache of the NFT each key holds at the best block. A holding row
/// is removed when its key becomes empty: it is a cache the tracker rebuilds,
/// not a record.
pub fn create_nft_items() -> M<'static> {
	M::up(
		"CREATE TABLE nft_item (
			key BLOB PRIMARY KEY CHECK (length(key) = 32),
			purse INTEGER NOT NULL REFERENCES nft_purse(id),
			installation BLOB NOT NULL CHECK (length(installation) = 32),
			idx INTEGER NOT NULL CHECK (idx BETWEEN 0 AND 4294967295),
			UNIQUE (purse, installation, idx)
		);
		CREATE TABLE nft_holding (
			key BLOB PRIMARY KEY REFERENCES nft_item(key),
			collection INTEGER NOT NULL,
			item INTEGER NOT NULL,
			instance INTEGER NOT NULL
		);",
	)
}
