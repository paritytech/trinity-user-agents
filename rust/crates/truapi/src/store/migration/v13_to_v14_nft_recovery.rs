//! Schema version 14: which installations NFT backup recovery has finished.

use rusqlite_migration::M;

/// Creates `nft_recovery`, one row per installation recovery has started
/// scanning. `done` is set once every purse pass of that installation has
/// ended, and a done installation is not scanned again.
pub fn create_nft_recovery() -> M<'static> {
	M::up(
		"CREATE TABLE nft_recovery (
			installation BLOB PRIMARY KEY CHECK (length(installation) = 32),
			done INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1))
		);",
	)
}
