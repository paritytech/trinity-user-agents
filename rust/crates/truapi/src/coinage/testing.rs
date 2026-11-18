//! Fixtures shared by the coinage layer's tests.

use core::time::Duration;

use parity_scale_codec::Decode;
use subxt::Metadata;

use super::{
	ChainConfig, ChainParams,
	params::{InstanceRecord, validate},
};

/// Metadata of the Paseo Next People chain, which runs the coinage pallet.
pub const PEOPLE_METADATA_BYTES: &[u8] =
	include_bytes!("../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// [`PEOPLE_METADATA_BYTES`], decoded.
pub fn people_metadata() -> Metadata {
	let prefixed =
		frame_metadata::RuntimeMetadataPrefixed::decode(&mut &PEOPLE_METADATA_BYTES[..])
			.expect("the People fixture decodes");
	Metadata::try_from(prefixed).expect("the People fixture is subxt metadata")
}

/// A configuration with the shipped values for the Paseo Next People chain.
pub fn chain_config() -> ChainConfig {
	ChainConfig {
		instance: 0,
		extension_version: 0,
		ring_lifetime: Duration::from_secs(90 * 24 * 3600),
		ring_page_size: 255,
		token_grace: Duration::from_secs(3600),
		external_minimum_balance: 1,
		block_time: Duration::from_secs(6),
		rescue_margin: Duration::from_secs(7 * 24 * 3600),
		reserve_share: 20,
		fallback_age_cutoff: 14,
		longest_window: 64,
		max_claim_window: 256,
	}
}

/// The record of the instance [`chain_config`] names.
pub fn instance_record() -> InstanceRecord {
	InstanceRecord { asset_unit: 10_000 }
}

/// Parameters validated from the People fixture with [`chain_config`] and a
/// maximum coin age of 16.
pub fn chain_params() -> ChainParams {
	validate(&people_metadata(), Some(&instance_record()), Some(16), chain_config())
		.expect("the People fixture serves the layer")
}
