//! Schema version 5: funding sessions and the summaries of compacted ones.

use rusqlite_migration::M;

/// Creates `funding_session`, one row per stored session, and
/// `funding_summary`, one row per session compacted out of it. Each record is
/// a SCALE value led by its format as a `u16`, so a format bump is read row
/// by row.
pub fn create_funding_sessions() -> M<'static> {
	M::up(
		"CREATE TABLE funding_session (
			intent TEXT PRIMARY KEY NOT NULL,
			record BLOB NOT NULL
		);
		CREATE TABLE funding_summary (
			intent TEXT PRIMARY KEY NOT NULL,
			record BLOB NOT NULL
		);",
	)
}
