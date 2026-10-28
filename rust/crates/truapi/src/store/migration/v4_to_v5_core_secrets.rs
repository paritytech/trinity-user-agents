//! Schema version 5: the index of secrets the core keeps in the host's
//! encrypted storage.

use rusqlite_migration::M;

/// Creates `core_secret`, one row per secret: the feature it belongs to, its
/// id within that feature, and the product and account that own it. The
/// secret itself is in the host's encrypted storage, never here.
pub fn create_core_secrets() -> M<'static> {
	M::up(
		"CREATE TABLE core_secret (
			kind TEXT NOT NULL,
			id BLOB NOT NULL,
			product_id TEXT NOT NULL,
			account BLOB CHECK (account IS NULL OR length(account) = 32),
			PRIMARY KEY (kind, id)
		);
		CREATE INDEX core_secret_owner ON core_secret (kind, product_id, account);",
	)
}
