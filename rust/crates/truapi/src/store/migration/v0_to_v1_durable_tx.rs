//! Schema version 1: the `durable_tx` ledger of presigned transactions.

use rusqlite_migration::M;

/// Creates the `durable_tx` ledger.
pub fn create_durable_tx() -> M<'static> {
	M::up(
		"CREATE TABLE durable_tx (
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
			failure_kind TEXT CHECK (failure_kind IN ('EXPIRED', 'DISPATCH_FAILED', 'REJECTED')),
			policy_id TEXT,
			policy_params BLOB
		);
		CREATE INDEX durable_tx_group ON durable_tx(domain, group_id);
		CREATE INDEX durable_tx_status ON durable_tx(domain, status);",
	)
}
