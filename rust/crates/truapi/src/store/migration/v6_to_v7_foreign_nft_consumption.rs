//! Schema version 7: spends of item keys the host did not derive.

use rusqlite_migration::M;

/// Recreates `nft_consumption` so it records spends of any item key, ours or
/// one imported from a cheque, so a key that a live transaction moves is
/// refused whatever its origin. A key no longer has to be in `nft_item`.
/// Rows of the previous table are dropped, none exist outside development.
pub fn recreate_nft_consumption_for_foreign_keys() -> M<'static> {
	M::up(
		"DROP TABLE nft_consumption;
		CREATE TABLE nft_consumption (
			key BLOB NOT NULL,
			kind TEXT NOT NULL CHECK (kind IN ('INPUT', 'HANDOFF')),
			tx INTEGER UNIQUE REFERENCES durable_tx(id),
			handoff_tag BLOB,
			CHECK ((kind = 'INPUT') = (tx IS NOT NULL)),
			CHECK ((kind = 'HANDOFF') = (handoff_tag IS NOT NULL))
		);
		CREATE INDEX nft_consumption_key ON nft_consumption(key);
		CREATE INDEX nft_consumption_handoff_tag ON nft_consumption(handoff_tag);",
	)
}

#[cfg(test)]
mod tests;
