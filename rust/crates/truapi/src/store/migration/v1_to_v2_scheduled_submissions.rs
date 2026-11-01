//! Schema version 2: what scheduled submissions need, a failure kind for a
//! transaction its policy never built and a count of the attempts built.

use rusqlite_migration::M;

/// Adds `ABANDONED` and an `attempts` count while preserving every ledger row
/// and the id sequence. Rows outside the supported statuses abort the migration.
pub fn recreate_durable_tx_for_scheduled_submissions() -> M<'static> {
	M::up(
		"CREATE TABLE durable_tx_v2 (
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
		INSERT INTO durable_tx_v2
			(id, domain, group_id, tx_hash, birth_number, birth_hash, period,
			 success_number, success_hash, scanned_number, status, failure_kind,
			 attempts, policy_id, policy_params)
		SELECT id, domain, group_id, tx_hash, birth_number, birth_hash, period,
			success_number, success_hash, scanned_number, status, failure_kind,
			0, policy_id, policy_params
		FROM durable_tx;
		UPDATE sqlite_sequence
			SET seq = MAX(seq, COALESCE(
				(SELECT seq FROM sqlite_sequence WHERE name = 'durable_tx'), 0))
			WHERE name = 'durable_tx_v2';
		INSERT INTO sqlite_sequence (name, seq)
			SELECT 'durable_tx_v2', seq FROM sqlite_sequence
			WHERE name = 'durable_tx' AND NOT EXISTS
				(SELECT 1 FROM sqlite_sequence WHERE name = 'durable_tx_v2');
		DROP TABLE durable_tx;
		ALTER TABLE durable_tx_v2 RENAME TO durable_tx;
		CREATE INDEX durable_tx_group ON durable_tx(domain, group_id);
		CREATE INDEX durable_tx_status ON durable_tx(domain, status);",
	)
}

#[cfg(test)]
mod tests {
	use rusqlite::{Connection, types::Value};

	use crate::store::core_migrations;

	fn populated_v1() -> Connection {
		let mut conn = Connection::open_in_memory().unwrap();
		core_migrations().to_version(&mut conn, 1).unwrap();
		conn.execute_batch(
			"INSERT INTO durable_tx
				(id, domain, group_id, tx_hash, birth_number, birth_hash, period,
				 success_number, success_hash, scanned_number, status, failure_kind,
				 policy_id, policy_params)
			VALUES
				(10, 'coinage', 'pending', zeroblob(32), 100, zeroblob(32), 64,
				 NULL, NULL, 110, 'PENDING', NULL, 'retry', X'0102'),
				(20, 'coinage', 'included', zeroblob(32), 101, zeroblob(32), 64,
				 120, zeroblob(32), NULL, 'PENDING_SUCCESS', NULL, NULL, NULL),
				(30, 'coinage', 'completed', zeroblob(32), 102, zeroblob(32), 64,
				 121, zeroblob(32), 119, 'FINALIZED_SUCCESS', NULL, NULL, NULL),
				(40, 'coinage', 'failed', zeroblob(32), 103, zeroblob(32), 64,
				 NULL, NULL, 166, 'FAILURE', 'EXPIRED', NULL, NULL),
				(99, 'coinage', NULL, NULL, NULL, NULL, NULL,
				 NULL, NULL, NULL, 'PENDING_SUBMISSION', NULL, 'retry', X'03');
			DELETE FROM durable_tx WHERE id = 99;",
		)
		.unwrap();
		conn
	}

	fn rows(conn: &Connection) -> Vec<Vec<Value>> {
		let mut statement = conn
			.prepare(
				"SELECT id, domain, group_id, tx_hash, birth_number, birth_hash, period,
					success_number, success_hash, scanned_number, status, failure_kind,
					policy_id, policy_params FROM durable_tx ORDER BY id",
			)
			.unwrap();
		let columns = statement.column_count();
		statement
			.query_map([], |row| (0..columns).map(|column| row.get(column)).collect())
			.unwrap()
			.collect::<Result<_, _>>()
			.unwrap()
	}

	#[test]
	fn pending_and_completed_transactions_keep_their_evidence_and_ids() {
		let mut conn = populated_v1();
		let before = rows(&conn);

		core_migrations().to_latest(&mut conn).unwrap();

		assert_eq!(rows(&conn), before);
		let attempts: Vec<(i64, u32)> = conn
			.prepare("SELECT id, attempts FROM durable_tx ORDER BY id")
			.unwrap()
			.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
			.unwrap()
			.collect::<Result<_, _>>()
			.unwrap();
		assert_eq!(attempts, vec![(10, 0), (20, 0), (30, 0), (40, 0)]);
		let next_id: i64 = conn
			.query_row(
				"INSERT INTO durable_tx (domain, status, policy_id, policy_params)
				 VALUES ('coinage', 'PENDING_SUBMISSION', 'retry', X'04') RETURNING id",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(next_id, 100);
	}

	#[test]
	fn an_emptied_ledger_does_not_reuse_an_issued_id() {
		let mut conn = populated_v1();
		conn.execute("DELETE FROM durable_tx", []).unwrap();

		core_migrations().to_latest(&mut conn).unwrap();

		let next_id: i64 = conn
			.query_row(
				"INSERT INTO durable_tx (domain, status, policy_id, policy_params)
				 VALUES ('coinage', 'PENDING_SUBMISSION', 'retry', X'04') RETURNING id",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(next_id, 100);
	}

	#[test]
	fn an_unsupported_record_rolls_back_the_entire_migration() {
		let mut conn = populated_v1();
		conn.execute_batch(
			"PRAGMA ignore_check_constraints = ON;
			UPDATE durable_tx SET status = 'UNKNOWN' WHERE id = 30;
			PRAGMA ignore_check_constraints = OFF;",
		)
		.unwrap();
		let before = rows(&conn);

		assert!(core_migrations().to_latest(&mut conn).is_err());

		assert_eq!(rows(&conn), before);
		let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
		assert_eq!(version, 1);
		let leftover: bool = conn
			.query_row(
				"SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'durable_tx_v2')",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert!(!leftover);
		conn.execute("UPDATE durable_tx SET status = 'FINALIZED_SUCCESS' WHERE id = 30", [])
			.unwrap();
		let repaired = rows(&conn);
		core_migrations().to_latest(&mut conn).unwrap();
		assert_eq!(rows(&conn), repaired);
	}
}
