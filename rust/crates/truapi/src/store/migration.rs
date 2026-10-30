//! The core database's schema migrations, one file per schema version.

mod v0_to_v1_durable_tx;
mod v1_to_v2_scheduled_submissions;
mod v2_to_v3_nft_purses;
mod v3_to_v4_nft_items;
mod v4_to_v5_nft_consumption;
mod v5_to_v6_nft_receive_keys;

use rusqlite_migration::Migrations;

/// Schema of the core database, one migration per change, in order. A
/// released migration never changes.
pub fn core_migrations() -> Migrations<'static> {
	Migrations::new(vec![
		v0_to_v1_durable_tx::create_durable_tx(),
		v1_to_v2_scheduled_submissions::recreate_durable_tx_for_scheduled_submissions(),
		v2_to_v3_nft_purses::create_nft_purse(),
		v3_to_v4_nft_items::create_nft_items(),
		v4_to_v5_nft_consumption::create_nft_consumption(),
		v5_to_v6_nft_receive_keys::create_nft_receive_keys(),
	])
}
