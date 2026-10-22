//! Schema version 5: which NFT item keys are spent.

use rusqlite_migration::M;

/// Creates `nft_consumption`, one row per spend of an item key: the input of
/// a durable transaction, or a handoff of its secret. Rows are never deleted.
/// An input stops blocking its key once its transaction fails, which the
/// availability query reads from `durable_tx`. A key can be spent more than
/// once over time, so only the transaction is unique.
pub fn create_nft_consumption() -> M<'static> {
	M::up(
		"CREATE TABLE nft_consumption (
			key BLOB NOT NULL REFERENCES nft_item(key),
			kind TEXT NOT NULL CHECK (kind IN ('INPUT', 'HANDOFF')),
			tx INTEGER UNIQUE REFERENCES durable_tx(id),
			CHECK ((kind = 'INPUT') = (tx IS NOT NULL))
		);
		CREATE INDEX nft_consumption_key ON nft_consumption(key);",
	)
}
