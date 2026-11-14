//! Schema version 13: the ledger keeps an attempt's window as its birth
//! block number and period, without the birth block's hash.

use rusqlite_migration::M;

/// Drops `durable_tx.birth_hash`. The hash is an input to the attempt's
/// signature, which is complete before the engine sees the attempt; nothing
/// the engine decides reads it.
pub fn drop_durable_tx_birth_hash() -> M<'static> {
	M::up("ALTER TABLE durable_tx DROP COLUMN birth_hash;")
}

#[cfg(test)]
mod tests;
