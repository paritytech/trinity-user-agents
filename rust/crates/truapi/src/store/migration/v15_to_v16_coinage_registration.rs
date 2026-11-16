//! Schema version 16: the rows that tie each of the coinage layer's durable
//! transactions to the assets it consumes and creates (C1).

use rusqlite_migration::M;

/// Creates the registration rows: each record's operation and domain, its
/// inputs, its outputs, the unload tokens it presents, and the records it
/// depends on. Rows are never deleted.
pub fn create_coinage_registration() -> M<'static> {
	M::up(
		"CREATE TABLE coinage_record (
			record INTEGER PRIMARY KEY,
			domain TEXT NOT NULL,
			operation BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_input (
			asset BLOB NOT NULL,
			record INTEGER NOT NULL,
			PRIMARY KEY (asset, record)
		) STRICT;
		CREATE TABLE coinage_output (
			asset BLOB PRIMARY KEY,
			record INTEGER NOT NULL
		) STRICT;
		CREATE INDEX coinage_output_record ON coinage_output(record);
		CREATE TABLE coinage_token (
			token BLOB NOT NULL,
			record INTEGER NOT NULL,
			PRIMARY KEY (token, record)
		) STRICT;
		CREATE TABLE coinage_dependency (
			record INTEGER NOT NULL,
			depends_on INTEGER NOT NULL,
			PRIMARY KEY (record, depends_on)
		) STRICT;",
	)
}
