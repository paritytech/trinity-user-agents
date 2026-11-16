use core::time::Duration;

use parity_scale_codec::Decode;

use super::*;
use crate::coinage::testing::{chain_config, chain_params, instance_record, people_metadata};

fn refusal(result: Result<ChainParams, UnsupportedRuntime>) -> String {
	result.expect_err("the check fails").check
}

#[test]
fn the_people_runtime_gives_the_published_limits() {
	let params = chain_params();
	assert_eq!(params.largest_exponent().get(), 14);
	assert_eq!(params.exponents().count(), 15);
	assert_eq!(params.max_split_outputs(), 32);
	assert_eq!(params.max_unload_entries(), 64);
	assert_eq!(params.max_load_entries(), 10);
	assert_eq!(params.ring_size(), 767);
	assert_eq!(params.token_period(), Duration::from_secs(86_400));
	assert_eq!(params.lock_period(), Duration::from_secs(60));
	assert_eq!(params.instance(), &instance_record());
}

#[test]
fn the_age_cutoff_is_two_below_the_maximum_or_the_fallback() {
	let metadata = people_metadata();
	let with_maximum = validate(&metadata, Some(&instance_record()), Some(16), chain_config());
	assert_eq!(with_maximum.map(|params| params.age_cutoff()), Ok(14));
	let lower = validate(&metadata, Some(&instance_record()), Some(10), chain_config());
	assert_eq!(lower.map(|params| params.age_cutoff()), Ok(8));
	let config = ChainConfig { fallback_age_cutoff: 12, ..chain_config() };
	let without = validate(&metadata, Some(&instance_record()), None, config);
	assert_eq!(without.map(|params| params.age_cutoff()), Ok(12));
}

#[test]
fn a_missing_instance_fails_validation() {
	let check = refusal(validate(&people_metadata(), None, Some(16), chain_config()));
	assert_eq!(check, "no instance 0");
}

#[test]
fn a_runtime_without_the_pallet_fails_validation() {
	let bytes = include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");
	let prefixed = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &bytes[..]).unwrap();
	let asset_hub = Metadata::try_from(prefixed).unwrap();
	let check = refusal(validate(&asset_hub, Some(&instance_record()), Some(16), chain_config()));
	assert_eq!(check, "no pallet Coinage");
}

#[test]
fn a_pipeline_without_the_coinage_extension_fails_validation() {
	let config = ChainConfig { extension_version: 7, ..chain_config() };
	let check = refusal(validate(&people_metadata(), Some(&instance_record()), Some(16), config));
	assert_eq!(check, "no extension pipeline 7");
}

#[test]
fn the_longest_window_must_be_an_era_period_within_the_claim_window() {
	let metadata = people_metadata();
	let record = instance_record();
	for window in [0, 3, 63, 131_072] {
		let config = ChainConfig { longest_window: window, ..chain_config() };
		let check = refusal(validate(&metadata, Some(&record), Some(16), config));
		assert_eq!(check, "the longest validity window is not a power of two from 4 to 65536");
	}
	let config = ChainConfig { longest_window: 512, max_claim_window: 256, ..chain_config() };
	let check = refusal(validate(&metadata, Some(&record), Some(16), config));
	assert_eq!(check, "the longest validity window exceeds the maximum claim validity window");
}

#[test]
fn the_rescue_margin_must_exceed_the_longest_wait() {
	let metadata = people_metadata();
	let token_period = Duration::from_secs(86_400);
	let window = Duration::from_secs(6 * 64);
	let exactly = token_period + window + MAX_REBUILD_COOLDOWN;
	let config = ChainConfig { rescue_margin: exactly, ..chain_config() };
	let check = refusal(validate(&metadata, Some(&instance_record()), Some(16), config));
	assert!(check.starts_with("the rescue margin does not exceed"), "{check}");
	let config = ChainConfig { rescue_margin: exactly + Duration::from_secs(1), ..chain_config() };
	assert!(validate(&metadata, Some(&instance_record()), Some(16), config).is_ok());
}

#[test]
fn a_reserve_share_above_100_fails_validation() {
	let config = ChainConfig { reserve_share: 101, ..chain_config() };
	let check = refusal(validate(&people_metadata(), Some(&instance_record()), Some(16), config));
	assert_eq!(check, "the token reserve share is above 100 percent");
}
