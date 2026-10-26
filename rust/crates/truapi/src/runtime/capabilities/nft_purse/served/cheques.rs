//! Handing NFT items to another user: receive keys on the receiving side, and
//! cheques on the sending side.

use std::collections::BTreeSet;

use truapi::{CallContext, CallError, latest};

use super::{host_failure, operations::Drain};
use crate::{
	host_logic::x25519_seal::SealError,
	nft::{HandoffError, ItemKey, ItemRefusal, PurseId},
	nft_purse::{CreateReceiveKeyError, cheque_recipient},
	platform::NftPurseDrainDestination,
	runtime::ProductRuntimeHost,
};

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
		let key = purses
			.create_receive_key(self.connection.product_id(), into)
			.await
			.map_err(create_receive_key_failure)?;
		Ok(latest::HostNftPurseCreateReceiveKeyResponse { key })
	}

	/// Hands items of a purse off in a cheque, after confirming a drain of a
	/// purse the caller does not own. The items are spent from then on.
	pub async fn create_nft_cheque(
		&self,
		cx: &CallContext,
		request: latest::HostNftPurseCreateChequeRequest,
	) -> Result<
		latest::HostNftPurseCreateChequeResponse,
		CallError<latest::HostNftPurseCreateChequeError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseCreateChequeRequest { from, to, items } = request;
		let items: BTreeSet<ItemKey> = items.iter().map(|key| ItemKey(key.0)).collect();
		if items.is_empty() {
			return Err(CallError::Domain(latest::HostNftPurseCreateChequeError::NoItems));
		}
		let from = PurseId(from.0);
		let not_found = CallError::Domain(latest::HostNftPurseCreateChequeError::PurseNotFound);
		let owner = purses.owner_of(from).await.map_err(host_failure)?.ok_or(not_found)?;
		let item_count = u32::try_from(items.len()).map_err(host_failure)?;
		let destination = NftPurseDrainDestination::Cheque { recipient: cheque_recipient(&to) };
		let drain = Drain { from, owner, item_count, destination };
		let denied = latest::HostNftPurseCreateChequeError::Denied;
		self.confirm_drain(cx, drain, denied).await?;
		let caller = self.connection.product_id();
		let cheque = purses
			.create_cheque(caller, from, &to, items)
			.await
			.map_err(create_cheque_failure)?;
		Ok(latest::HostNftPurseCreateChequeResponse { cheque })
	}
}

/// The product's refusals as its domain errors, everything else as the
/// host's failure. A low-order recipient key is the product's.
fn create_cheque_failure(
	error: HandoffError<SealError>,
) -> CallError<latest::HostNftPurseCreateChequeError> {
	let refusal = match error {
		HandoffError::NoItems => latest::HostNftPurseCreateChequeError::NoItems,
		HandoffError::TagInUse(_) => latest::HostNftPurseCreateChequeError::OperationExists,
		HandoffError::Item(ItemRefusal::NotFound(_)) => {
			latest::HostNftPurseCreateChequeError::ItemNotFound
		},
		HandoffError::Item(ItemRefusal::Unavailable(_)) => {
			latest::HostNftPurseCreateChequeError::ItemUnavailable
		},
		HandoffError::Seal(SealError::LowOrderKey(_)) => {
			let reason = latest::GenericError { reason: error.to_string() };
			latest::HostNftPurseCreateChequeError::Other(reason)
		},
		other => return host_failure(other),
	};
	CallError::Domain(refusal)
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
