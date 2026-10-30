//! Schema version 6: the secrets products keep through `SecretsStorage`.

use rusqlite_migration::M;

/// Creates `product_secret`, one row per secret a product keeps: its id, and
/// the product and account that own it. The secret itself is in the host's
/// encrypted storage, never here.
pub fn create_product_secrets() -> M<'static> {
	M::up(
		"CREATE TABLE product_secret (
			id BLOB PRIMARY KEY CHECK (length(id) = 32),
			product_id TEXT NOT NULL,
			account BLOB NOT NULL CHECK (length(account) = 32)
		);
		CREATE INDEX product_secret_owner ON product_secret (product_id, account);",
	)
}
