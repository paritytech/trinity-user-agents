//! Handing NFT items to another user: receive keys on the receiving side, and
//! cheques on the sending side.

use truapi::{CallError, latest};

use super::host_failure;
use crate::{nft::PurseId, nft_purse::CreateReceiveKeyError, runtime::ProductRuntimeHost};

impl ProductRuntimeHost {
	/// A receive key into the purse the product named. Any product may receive
	/// into any purse, so nothing is asked.
	pub async fn create_nft_receive_key(
		&self,
		request: latest::HostNftPurseCreateReceiveKeyRequest,
	) -> Result<
		latest::HostNftPurseCreateReceiveKeyResponse,
		CallError<latest::HostNftPurseCreateReceiveKeyError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let into = PurseId(request.into.0);
		let created = purses.create_receive_key(self.connection.product_id(), into).await;
		let key = created.map_err(create_receive_key_failure)?;
		Ok(latest::HostNftPurseCreateReceiveKeyResponse { key })
	}
}

/// An unknown purse, or a host that kept no secret, is the product's answer.
/// Everything else is the host's.
fn create_receive_key_failure(
	error: CreateReceiveKeyError,
) -> CallError<latest::HostNftPurseCreateReceiveKeyError> {
	match error {
		CreateReceiveKeyError::UnknownPurse(_) => {
			CallError::Domain(latest::HostNftPurseCreateReceiveKeyError::PurseNotFound)
		},
		CreateReceiveKeyError::SecretNotStored(_) => {
			let reason = latest::GenericError { reason: error.to_string() };
			CallError::Domain(latest::HostNftPurseCreateReceiveKeyError::Other(reason))
		},
		error => host_failure(error),
	}
}
