//! Schema version 15: the coinage layer's operations (B4).

use rusqlite_migration::M;

/// Creates `coinage_operation`: each operation's identity, kind and
/// arguments, a requested cancellation, an export's release and the coins it
/// hands to its payee, an offload's stored wait, and the stored end of any
/// operation.
pub fn create_coinage_operations() -> M<'static> {
	M::up(
		"CREATE TABLE coinage_operation (
			scope BLOB NOT NULL,
			id BLOB NOT NULL,
			kind BLOB NOT NULL,
			arguments BLOB NOT NULL,
			cancel_requested INTEGER NOT NULL DEFAULT 0,
			released INTEGER NOT NULL DEFAULT 0,
			offload_wait BLOB,
			ended BLOB,
			exported BLOB,
			PRIMARY KEY (scope, id)
		) STRICT;",
	)
}
