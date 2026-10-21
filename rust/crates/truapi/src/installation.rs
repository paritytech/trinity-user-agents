//! The app installation item keys are derived under, and the host's
//! provider of it.

use async_trait::async_trait;
use truapi::latest::GenericError;

/// The app installation keys are derived under. Recovery finds it because
/// the host registers it before handing it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[async_trait]
pub trait InstallationIdProvider: Send + Sync {
	/// The id of the current installation, created on first use.
	async fn current_installation(&self) -> Result<InstallationId, GenericError>;
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Keys derived under a truncated or padded id would land in a subtree
	/// recovery never scans, so only exactly 32 bytes make an id.
	#[test]
	fn an_installation_id_takes_exactly_32_bytes() {
		let read =
			|length: usize| InstallationId::try_from(vec![7; length]).map_err(|error| error.reason);
		assert_eq!(
			[read(31), read(32), read(33)],
			[
				Err("an installation id has 32 bytes, got 31".to_owned()),
				Ok(InstallationId([7; 32])),
				Err("an installation id has 32 bytes, got 33".to_owned()),
			]
		);
	}
}
