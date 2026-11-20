//! Schema version 19: a deferred durable transaction can wait for other
//! transactions.

use rusqlite_migration::M;

/// Creates `durable_tx_dependency`, one row per transaction a deferred one
/// waits for, with what that transaction's failure does to it. A dependency
/// is always registered before its dependent, so its id is lower and no
/// cycle can be stored.
pub fn create_durable_tx_dependency() -> M<'static> {
	M::up(
		"CREATE TABLE durable_tx_dependency (
			tx_id INTEGER NOT NULL REFERENCES durable_tx(id),
			depends_on INTEGER NOT NULL REFERENCES durable_tx(id),
			on_failure TEXT NOT NULL CHECK (on_failure IN ('IGNORE', 'CASCADE')),
			PRIMARY KEY (tx_id, depends_on),
			CHECK (depends_on < tx_id)
		);
		CREATE INDEX durable_tx_dependency_by_depends_on
			ON durable_tx_dependency (depends_on);",
	)
}

#[cfg(test)]
mod tests;
