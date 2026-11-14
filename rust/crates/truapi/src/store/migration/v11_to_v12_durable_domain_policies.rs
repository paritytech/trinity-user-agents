//! Schema version 12: a durable transaction is built by its domain's one
//! policy, so rows no longer name a policy.

use rusqlite_migration::M;

/// Drops `durable_tx.policy_id`. A row has a plan exactly when it has
/// `policy_params`, which were always written together with it.
pub fn drop_durable_tx_policy_id() -> M<'static> {
	M::up("ALTER TABLE durable_tx DROP COLUMN policy_id;")
}

#[cfg(test)]
mod tests;
