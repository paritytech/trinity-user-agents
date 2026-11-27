//! Schema version 20: a held NFT's fee-less moves are tracked, and every NFT
//! transfer records whether it moves fee-less or paid.

use rusqlite_migration::M;

/// Adds `nft_holding.moves`, the fee-less moves the held NFT used, which the
/// tracker fills on its next watch. Appends the fee-less mode, SCALE `0x00`,
/// to the params of every NFT transfer, whose mode is their last field. The
/// cast keeps the params a BLOB, since `||` yields TEXT.
pub fn track_nft_moves() -> M<'static> {
	M::up(
		"ALTER TABLE nft_holding ADD COLUMN moves INTEGER;
		UPDATE durable_tx SET policy_params = CAST(policy_params || X'00' AS BLOB)
			WHERE domain = 'nft' AND policy_params IS NOT NULL;",
	)
}

#[cfg(test)]
mod tests;
