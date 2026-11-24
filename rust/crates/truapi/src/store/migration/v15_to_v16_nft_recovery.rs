//! Schema version 16: which installations NFT backup recovery has finished.

use rusqlite_migration::M;

/// Creates `nft_recovery`, one row per installation a registry read listed.
/// `done` is set once recovery has scanned that installation's purses, and
/// from the start for the installation the app runs as, which is never
/// scanned. A done installation is not scanned again.
pub fn create_nft_recovery() -> M<'static> {
	M::up(
		"CREATE TABLE nft_recovery (
			installation BLOB PRIMARY KEY CHECK (length(installation) = 32),
			done INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1))
		);",
	)
}
