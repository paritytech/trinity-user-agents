//! Schema version 6: the receive keys products created for NFT purses.

use rusqlite_migration::M;

/// Creates `nft_receive_key`, one row per receive key: its X25519 public key,
/// the purse cheques for it are deposited into, and the product that created
/// it. The secret is in the host's encrypted storage, never here. Rows are
/// never deleted.
pub fn create_nft_receive_keys() -> M<'static> {
	M::up(
		"CREATE TABLE nft_receive_key (
			key BLOB PRIMARY KEY CHECK (length(key) = 32),
			purse INTEGER NOT NULL REFERENCES nft_purse(id),
			creator TEXT NOT NULL
		);",
	)
}
