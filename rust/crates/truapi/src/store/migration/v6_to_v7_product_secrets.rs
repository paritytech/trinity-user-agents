//! Schema version 7: the secrets products keep through `SecretsStorage`.

use rusqlite_migration::M;

/// Creates `product_secret`, one row per secret a product keeps: its id and
/// the product that owns it. The secret itself is in the host's encrypted
/// storage, never here.
pub fn create_product_secrets() -> M<'static> {
	M::up(
		"CREATE TABLE product_secret (
			id BLOB PRIMARY KEY CHECK (length(id) = 32),
			product_id TEXT NOT NULL
		);
		CREATE INDEX product_secret_product ON product_secret (product_id);",
	)
}
