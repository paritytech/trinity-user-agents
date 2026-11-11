//! Schema version 11: profile metadata retained by last access, independently of freshness.

use rusqlite_migration::M;

/// Tracks profile access order without changing existing records or freshness deadlines.
///
/// Existing records start ordered by expiry and username. Subsequent reads and writes
/// advance the access order; the least recently used records are evicted over capacity.
pub fn profile_cache_lru() -> M<'static> {
	M::up(
		"ALTER TABLE public_profiles ADD COLUMN access_order INTEGER NOT NULL DEFAULT 0;
		WITH ordered AS (
			SELECT username, row_number() OVER (ORDER BY expires_at, username) AS position
			FROM public_profiles
		)
		UPDATE public_profiles SET access_order = (
			SELECT position FROM ordered WHERE ordered.username = public_profiles.username
		);
		DROP INDEX public_profiles_expires_at;
		CREATE INDEX public_profiles_access_order ON public_profiles(access_order);",
	)
}

