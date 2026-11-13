//! The core database's schema migrations, one file per schema version.

mod v0_to_v1_durable_tx;
mod v10_to_v11_durable_domain_policies;
mod v1_to_v2_scheduled_submissions;
mod v2_to_v3_nft_purses;
mod v3_to_v4_nft_items;
mod v4_to_v5_nft_consumption;
mod v5_to_v6_nft_receive_keys;
mod v6_to_v7_foreign_nft_consumption;
mod v7_to_v8_funding_sessions;
mod v8_to_v9_pocket_cards;
mod v9_to_v10_public_profiles;

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
		v6_to_v7_foreign_nft_consumption::recreate_nft_consumption_for_foreign_keys(),
		v7_to_v8_funding_sessions::create_funding_sessions(),
		v8_to_v9_pocket_cards::create_pocket_cards(),
		v9_to_v10_public_profiles::create_public_profiles(),
		v10_to_v11_durable_domain_policies::drop_durable_tx_policy_id(),
	])
}
