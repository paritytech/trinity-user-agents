//! Schema version 20: the allowances a wallet confirmed on chain.

use rusqlite_migration::M;

/// Creates `confirmed_allowance`, one row per wallet, product and resource.
/// `root_public_key` is the wallet's root account, `resource` names the
/// allowance, and `period` is the Statement Store allowance period the slot
/// was confirmed in, or null for a Bulletin authorization. No key is stored:
/// the wallet derives it.
pub fn create_confirmed_allowances() -> M<'static> {
	M::up(
		"CREATE TABLE confirmed_allowance (
			root_public_key BLOB NOT NULL CHECK (length(root_public_key) = 32),
			product_id TEXT NOT NULL,
			resource TEXT NOT NULL,
			period INTEGER,
			PRIMARY KEY (root_public_key, product_id, resource)
		);",
	)
}
