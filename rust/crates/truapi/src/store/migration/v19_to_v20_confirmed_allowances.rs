//! Schema version 20: the allowances a wallet confirmed on chain.

use rusqlite_migration::M;

/// Creates `confirmed_allowance`, one row per product and resource.
/// `resource` names the allowance, and `period` is the Statement Store allowance period the slot
/// was confirmed in, or null for a Bulletin authorization. No key is stored:
/// the wallet derives it.
pub fn create_confirmed_allowances() -> M<'static> {
	M::up(
		"CREATE TABLE confirmed_allowance (
			product_id TEXT NOT NULL,
			resource TEXT NOT NULL,
			period INTEGER,
			PRIMARY KEY (product_id, resource)
		);",
	)
}
