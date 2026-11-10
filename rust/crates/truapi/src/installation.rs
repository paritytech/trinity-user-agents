//! The id of this app installation, which NFT item keys are derived under,
//! and the trait the host provides it through.

use parity_scale_codec::{Decode, Encode};
use truapi::latest::GenericError;

/// The id of this app installation, which keys are derived under. Recovery
/// can find it because the host registers it before handing it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode)]
pub struct InstallationId(pub [u8; 32]);

impl TryFrom<Vec<u8>> for InstallationId {
	type Error = GenericError;

	/// The id in `bytes`, which must be exactly 32 of them.
	fn try_from(bytes: Vec<u8>) -> Result<Self, GenericError> {
		let length = bytes.len();
		let id = <[u8; 32]>::try_from(bytes).map_err(|_| GenericError {
			reason: format!("an installation id has 32 bytes, got {length}"),
		})?;
		Ok(Self(id))
	}
}

/// Hands out the current app installation.
#[async_trait::async_trait]
pub trait InstallationIdProvider: Send + Sync {
	/// The id of the current installation, created on first use.
	async fn current_installation(&self) -> Result<InstallationId, GenericError>;
}

#[cfg(test)]
mod tests;
