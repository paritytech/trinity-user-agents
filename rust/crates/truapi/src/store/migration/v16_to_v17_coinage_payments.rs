//! Schema version 17: coinage payments' records (layer 2, J2).
//!
//! Records are SCALE-encoded by the payments module; the store keys rows by
//! them and never reads inside them.

use rusqlite_migration::M;

/// Creates the payment records: receivables, deposits, outgoing cheques,
/// RFC-6 operations, and the refusals of operations that never started.
pub fn create_coinage_payments() -> M<'static> {
	M::up(
		"CREATE TABLE coinage_receivable (
			receivable BLOB PRIMARY KEY,
			record BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_deposit (
			cheque BLOB PRIMARY KEY,
			receivable BLOB NOT NULL,
			record BLOB NOT NULL
		) STRICT;
		CREATE INDEX coinage_deposit_receivable ON coinage_deposit(receivable);
		CREATE TABLE coinage_outgoing (
			cheque BLOB PRIMARY KEY,
			record BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_rfc6 (
			product BLOB NOT NULL,
			id BLOB NOT NULL,
			record BLOB NOT NULL,
			PRIMARY KEY (product, id)
		) STRICT;
		CREATE TABLE coinage_refusal (
			operation BLOB PRIMARY KEY,
			error BLOB NOT NULL
		) STRICT;",
	)
}
