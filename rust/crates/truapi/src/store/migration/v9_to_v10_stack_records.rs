//! Schema version 10: the last descriptor verified for each stack.

use rusqlite_migration::M;

/// Creates `stack_record`, one row per stack a host build ran. `stack_id` is
/// the key in the stack's link, and `record` is what the stack reader stores:
/// the descriptor as served, when it was fetched and its `ETag`.
pub fn create_stack_records() -> M<'static> {
	M::up(
		"CREATE TABLE stack_record (
			stack_id TEXT NOT NULL PRIMARY KEY,
			record BLOB NOT NULL
		);",
	)
}
