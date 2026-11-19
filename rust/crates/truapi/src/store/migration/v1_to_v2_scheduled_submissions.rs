//! Schema version 2: what scheduled submissions need, a failure kind for a
//! transaction its policy never built and a count of the attempts built.

use rusqlite_migration::M;

/// Recreates `durable_tx` with the `ABANDONED` failure kind and an
/// `attempts` count. Nothing registers durable transactions before this
/// version, so the table is dropped rather than copied.
pub fn recreate_durable_tx_for_scheduled_submissions() -> M<'static> {
	M::up(
		"DROP TABLE durable_tx;
		CREATE TABLE durable_tx (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			domain TEXT NOT NULL,
			group_id TEXT,
			tx_hash BLOB,
			birth_number INTEGER,
			birth_hash BLOB,
			period INTEGER,
			success_number INTEGER,
			success_hash BLOB,
			scanned_number INTEGER,
			status TEXT NOT NULL CHECK (status IN
				('PENDING_SUBMISSION', 'PENDING', 'PENDING_SUCCESS', 'FINALIZED_SUCCESS', 'FAILURE')),
			failure_kind TEXT CHECK (failure_kind IN
				('EXPIRED', 'DISPATCH_FAILED', 'REJECTED', 'ABANDONED')),
			attempts INTEGER NOT NULL DEFAULT 0,
			policy_id TEXT,
			policy_params BLOB
		);
		CREATE INDEX durable_tx_group ON durable_tx(domain, group_id);
		CREATE INDEX durable_tx_status ON durable_tx(domain, status);",
	)
}
