//! Schema version 10: materials kept on the device.

use rusqlite_migration::M;

/// Creates `material`, the text of each material kept, under its ID. A
/// material never changes under its ID, so a row is only ever added or
/// evicted, and `rowid` orders rows by when they were kept.
pub fn create_materials() -> M<'static> {
	M::up(
		"CREATE TABLE material (
			id BLOB PRIMARY KEY,
			text BLOB NOT NULL
		);",
	)
}
