//! Fixtures shared by the pallet's tests.

use parity_scale_codec::Decode;
use subxt::ArcMetadata;

use super::move_budget::{MoveBudget, MoveBudgetSource};
use crate::chain::StorageWatchError;

/// Metadata of Paseo Asset Hub Next at spec 3004000, which runs the pallet.
pub const ASSET_HUB_METADATA_BYTES: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");

/// The extension pipeline the native hosts map Asset Hub to, which carries
/// `AsScarcity` and is signed by `VerifyMultiSignature`.
pub const ASSET_HUB_PIPELINE: u8 = 1;

/// [`ASSET_HUB_METADATA_BYTES`], decoded.
pub fn asset_hub_metadata() -> ArcMetadata {
	let prefixed =
		frame_metadata::RuntimeMetadataPrefixed::decode(&mut &ASSET_HUB_METADATA_BYTES[..])
			.expect("the Asset Hub fixture decodes");
	let metadata = subxt::Metadata::try_from(prefixed).expect("the fixture is subxt metadata");
	ArcMetadata::from(metadata)
}

/// A move budget that never reads the chain.
pub struct FixedMoveBudget(pub MoveBudget);

#[async_trait::async_trait]
impl MoveBudgetSource for FixedMoveBudget {
	async fn current_budget(&self) -> Result<MoveBudget, StorageWatchError> {
		Ok(self.0)
	}
}
