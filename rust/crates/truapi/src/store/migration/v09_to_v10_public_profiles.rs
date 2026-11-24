//! Schema version 10: resolved public profile records, kept until they expire.

use rusqlite_migration::M;

/// Creates `public_profiles`, one SCALE-encoded `ProfileRecord` per username.
pub fn create_public_profiles() -> M<'static> {
	M::up(
		"CREATE TABLE public_profiles (
			username TEXT PRIMARY KEY,
			public_record BLOB NOT NULL,
			expires_at INTEGER NOT NULL
		);
		CREATE INDEX public_profiles_expires_at ON public_profiles(expires_at);",
	)
}

#[cfg(test)]
mod tests;
