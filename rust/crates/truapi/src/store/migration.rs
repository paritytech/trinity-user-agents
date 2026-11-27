//! The core database's schema migrations, one file per schema version.

mod v00_to_v01_durable_tx;
mod v01_to_v02_scheduled_submissions;
mod v02_to_v03_nft_purses;
mod v03_to_v04_nft_items;
mod v04_to_v05_nft_consumption;
mod v05_to_v06_nft_receive_keys;
mod v06_to_v07_foreign_nft_consumption;
mod v07_to_v08_funding_sessions;
mod v08_to_v09_pocket_cards;
mod v09_to_v10_public_profiles;
mod v10_to_v11_profile_cache_lru;
mod v11_to_v12_materials;
mod v12_to_v13_stack_records;
mod v13_to_v14_nft_cheques;
mod v14_to_v15_pocket_card_sides;
mod v15_to_v16_nft_recovery;
mod v16_to_v17_durable_domain_policies;
mod v17_to_v18_durable_birth_hash;
mod v18_to_v19_durable_tx_dependency;
mod v19_to_v20_confirmed_allowances;

use rusqlite_migration::Migrations;

/// Schema of the core database, one migration per change, in order. A
/// released migration never changes.
pub fn core_migrations() -> Migrations<'static> {
	Migrations::new(vec![
		v00_to_v01_durable_tx::create_durable_tx(),
		v01_to_v02_scheduled_submissions::recreate_durable_tx_for_scheduled_submissions(),
		v02_to_v03_nft_purses::create_nft_purse(),
		v03_to_v04_nft_items::create_nft_items(),
		v04_to_v05_nft_consumption::create_nft_consumption(),
		v05_to_v06_nft_receive_keys::create_nft_receive_keys(),
		v06_to_v07_foreign_nft_consumption::recreate_nft_consumption_for_foreign_keys(),
		v07_to_v08_funding_sessions::create_funding_sessions(),
		v08_to_v09_pocket_cards::create_pocket_cards(),
		v09_to_v10_public_profiles::create_public_profiles(),
		v10_to_v11_profile_cache_lru::profile_cache_lru(),
		v11_to_v12_materials::create_materials(),
		v12_to_v13_stack_records::create_stack_records(),
		v13_to_v14_nft_cheques::create_nft_cheques(),
		v14_to_v15_pocket_card_sides::key_pocket_card_faces_by_side(),
		v15_to_v16_nft_recovery::create_nft_recovery(),
		v16_to_v17_durable_domain_policies::drop_durable_tx_policy_id(),
		v17_to_v18_durable_birth_hash::drop_durable_tx_birth_hash(),
		v18_to_v19_durable_tx_dependency::create_durable_tx_dependency(),
		v19_to_v20_confirmed_allowances::create_confirmed_allowances(),
	])
}
