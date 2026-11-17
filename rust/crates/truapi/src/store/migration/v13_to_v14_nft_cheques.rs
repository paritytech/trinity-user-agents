//! Schema version 14: the cheques products created from NFT purses.

use rusqlite_migration::M;

/// Creates `nft_cheque`, one row per cheque a product created: the creating
/// product, the cheque id, the purse its items left, the SCALE-encoded
/// destination it was created for, the SCALE-encoded cheque as the product
/// received it, and its creation time in Unix milliseconds. Rows are never
/// deleted.
pub fn create_nft_cheques() -> M<'static> {
	M::up(
		"CREATE TABLE nft_cheque (
			creator TEXT NOT NULL,
			id BLOB NOT NULL CHECK (length(id) = 32),
			purse INTEGER NOT NULL REFERENCES nft_purse(id),
			destination BLOB NOT NULL,
			cheque BLOB NOT NULL,
			created_at INTEGER NOT NULL,
			PRIMARY KEY (creator, id)
		);",
	)
}
