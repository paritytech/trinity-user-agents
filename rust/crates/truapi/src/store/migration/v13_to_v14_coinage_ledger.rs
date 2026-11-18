//! Schema version 14: the coinage layer's ledger (B2).
//!
//! Identities and values are SCALE-encoded by the coinage module; the store
//! keys rows by them and never reads inside them.

use rusqlite_migration::M;

/// Creates the coinage ledger's tables: purses, assets and their derivation
/// counters, what the chain last showed, disclosure marks, the secrets of
/// peer coins being imported, installations, and the layer's settings.
pub fn create_coinage_ledger() -> M<'static> {
	M::up(
		"CREATE TABLE coinage_purse (
			id INTEGER PRIMARY KEY,
			record BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_asset (
			asset BLOB PRIMARY KEY,
			record BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_counter (
			purse INTEGER NOT NULL,
			installation BLOB NOT NULL,
			kind INTEGER NOT NULL,
			next INTEGER NOT NULL,
			PRIMARY KEY (purse, installation, kind)
		) STRICT;
		CREATE TABLE coinage_observation (
			subject BLOB PRIMARY KEY,
			observed BLOB NOT NULL,
			value BLOB,
			included_at INTEGER
		) STRICT;
		CREATE TABLE coinage_mark (
			asset BLOB PRIMARY KEY,
			committed INTEGER NOT NULL
		) STRICT;
		CREATE TABLE coinage_peer_secret (
			account BLOB PRIMARY KEY,
			secret BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_installation (
			id BLOB PRIMARY KEY,
			learned INTEGER NOT NULL,
			current INTEGER NOT NULL,
			progress BLOB NOT NULL
		) STRICT;
		CREATE TABLE coinage_setting (
			name TEXT PRIMARY KEY,
			value BLOB NOT NULL
		) STRICT;",
	)
}
