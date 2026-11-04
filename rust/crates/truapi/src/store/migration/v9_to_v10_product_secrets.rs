//! Schema version 10: the secrets products keep through `SecretsStorage`.

use rusqlite_migration::M;

/// Creates `product_secret`, one row per secret a product keeps: its id, the
/// product that owns it, and whether a clear dropped it while its secret may
/// still be in the host's encrypted storage. The secret itself is there, never
/// here.
pub fn create_product_secrets() -> M<'static> {
	M::up(
		"CREATE TABLE product_secret (
			id BLOB PRIMARY KEY CHECK (length(id) = 32),
			product_id TEXT NOT NULL,
			dropped INTEGER NOT NULL DEFAULT 0 CHECK (dropped IN (0, 1))
		);
		CREATE INDEX product_secret_product ON product_secret (product_id);",
	)
}
