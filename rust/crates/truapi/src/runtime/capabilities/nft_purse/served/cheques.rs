//! Handing NFT items to another user: receive keys on the receiving side, and
//! cheques on the sending side.

use std::collections::BTreeSet;

use truapi::{CallContext, CallError, latest};

use super::{host_failure, operations::Drain};
use crate::{
	durable::RegistrationError,
	host_logic::x25519_seal::SealError,
	nft::{HandoffError, ItemKey, ItemRefusal, PurseId, StartImportError, StartOperationError},
	nft_purse::{CreateReceiveKeyError, DepositRefusal, NftPurses},
	platform::{NftPurseDrainDestination, ProductId},
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
		let drain =
			Drain { from, owner, item_count, destination: NftPurseDrainDestination::Cheque };
		let denied = latest::HostNftPurseCreateChequeError::Denied;
		self.confirm_drain(cx, drain, denied).await?;
		let caller = self.connection.product_id();
		let cheque = purses
			.create_cheque(caller, from, &to, items)
			.await
			.map_err(create_cheque_failure)?;
		Ok(latest::HostNftPurseCreateChequeResponse { cheque })
	}

	/// The key products publish for `Static` cheques. Every product gets the
	/// same key, so nothing is asked.
	pub fn static_nft_receive_key(
		&self,
	) -> Result<
		latest::HostNftPurseStaticReceiveKeyResponse,
		CallError<latest::HostNftPurseStaticReceiveKeyError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let key = purses.static_receive_key().map_err(host_failure)?;
		Ok(latest::HostNftPurseStaticReceiveKeyResponse { key })
	}

	/// Opens a received cheque and starts moving its items into the purse the
	/// product named. Any product may receive into any purse, so nothing is
	/// asked.
	pub async fn deposit_nft_cheque(
		&self,
		request: latest::HostNftPurseDepositRequest,
	) -> Result<(), CallError<latest::HostNftPurseDepositError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseDepositRequest { cheque, to } = request;
		let depositor = self.connection.product_id();
		purses.deposit(depositor, &cheque, PurseId(to.0)).await.map_err(deposit_failure)
	}

	/// Starts moving the items of a cheque the product created back into its
	/// purse. Only the product's own cheques are found, and its own purse
	/// gains the items, so nothing is asked.
	pub async fn revoke_nft_cheque(
		&self,
		request: latest::HostNftPurseRevokeChequeRequest,
	) -> Result<(), CallError<latest::HostNftPurseRevokeChequeError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let creator = self.connection.product_id();
		purses.revoke_cheque(creator, &request.cheque_id).await.map_err(revoke_failure)
	}

	/// Starts returning the items of a cheque the product deposited to the
	/// cheque's keys, after confirming a drain of a purse the caller does
	/// not own. Answers which items move and which stay.
	pub async fn refund_nft_cheque(
		&self,
		cx: &CallContext,
		request: latest::HostNftPurseRefundRequest,
	) -> Result<latest::HostNftPurseRefundResponse, CallError<latest::HostNftPurseRefundError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let depositor = self.connection.product_id();
		let id = request.cheque_id;
		let drain = drain_to_confirm(&purses, depositor, &id).await?;
		self.confirm_drain(cx, drain, latest::HostNftPurseRefundError::Denied).await?;
		purses.refund(depositor, &id).await.map_err(refund_failure)
	}
}

/// The drain the user confirms for a refund of `depositor`'s cheque `id`,
/// refusing early what the refund would refuse, so the user is not asked
/// for nothing.
async fn drain_to_confirm(
	purses: &NftPurses,
	depositor: &ProductId,
	id: &latest::NftPurseChequeId,
) -> Result<Drain, CallError<latest::HostNftPurseRefundError>> {
	let drain = purses.refund_drain(depositor, id).await.map_err(refund_refusal)?;
	let owner = purses.owner_of(drain.from).await.map_err(host_failure)?.ok_or_else(|| {
		host_failure(format!("purse {} of a deposit does not exist", drain.from.0))
	})?;
	let item_count = u32::try_from(drain.item_count).map_err(host_failure)?;
	let destination = NftPurseDrainDestination::Refund;
	Ok(Drain { from: drain.from, owner, item_count, destination })
}

/// A refund refused by Layer 1 as the product's error. A host without NFT
/// transfers does not support refunds. Everything else is the host's failure.
fn refund_failure(
	error: RegistrationError<StartOperationError>,
) -> CallError<latest::HostNftPurseRefundError> {
	match error {
		RegistrationError::Hook(refusal) => refund_refusal(refusal),
		RegistrationError::UnknownDomain(_) => CallError::Unsupported,
		other => host_failure(other),
	}
}

/// A deposit this product never made is not found, one still moving is in
/// progress, a repeated refund exists, and a refund that would move nothing
/// is unavailable. Anything else is the host's failure.
fn refund_refusal(refusal: StartOperationError) -> CallError<latest::HostNftPurseRefundError> {
	let domain = match refusal {
		StartOperationError::ImportNotFound => latest::HostNftPurseRefundError::ChequeNotFound,
		StartOperationError::ImportUnsettled => latest::HostNftPurseRefundError::DepositInProgress,
		StartOperationError::OperationExists => latest::HostNftPurseRefundError::OperationExists,
		StartOperationError::NothingIdle => latest::HostNftPurseRefundError::ItemUnavailable,
		other => return host_failure(other),
	};
	CallError::Domain(domain)
}

/// A cheque the product did not create is not found. A host without NFT
/// transfers does not support revokes. Everything else is the host's failure.
fn revoke_failure(
	error: RegistrationError<StartOperationError>,
) -> CallError<latest::HostNftPurseRevokeChequeError> {
	match error {
		RegistrationError::Hook(StartOperationError::TagNotFound) => {
			CallError::Domain(latest::HostNftPurseRevokeChequeError::ChequeNotFound)
		},
		RegistrationError::UnknownDomain(_) => CallError::Unsupported,
		other => host_failure(other),
	}
}

/// The product's refusals as its domain errors, everything else as the
/// host's failure. A receive key that is not the depositor's answers as a
/// corrupted cheque, so a product learns nothing about another's keys.
fn deposit_failure(refusal: DepositRefusal) -> CallError<latest::HostNftPurseDepositError> {
	let domain = match refusal {
		DepositRefusal::EmptyCheque => latest::HostNftPurseDepositError::EmptyCheque,
		DepositRefusal::PurseNotFound(_) => latest::HostNftPurseDepositError::PurseNotFound,
		DepositRefusal::PurseMismatch { .. } => latest::HostNftPurseDepositError::PurseMismatch,
		DepositRefusal::UnknownReceiveKey(_) |
		DepositRefusal::ChequeIdMismatch { .. } |
		DepositRefusal::CorruptedCheque(_) => latest::HostNftPurseDepositError::CorruptedCheque,
		DepositRefusal::ReceiveSecret(_) |
		DepositRefusal::Start(StartImportError::StoreSecrets(_)) => {
			let reason = latest::GenericError { reason: refusal.to_string() };
			latest::HostNftPurseDepositError::Other(reason)
		},
		DepositRefusal::Start(StartImportError::Registration(error)) => {
			return import_registration_failure(error);
		},
		other => return host_failure(other),
	};
	CallError::Domain(domain)
}

/// A deposit already registered under the cheque id is the product's error.
/// A host without NFT transfers does not support deposits. Everything else
/// is the host's failure.
fn import_registration_failure(
	error: RegistrationError<StartOperationError>,
) -> CallError<latest::HostNftPurseDepositError> {
	match error {
		RegistrationError::Hook(StartOperationError::OperationExists) => {
			CallError::Domain(latest::HostNftPurseDepositError::OperationExists)
		},
		RegistrationError::UnknownDomain(_) => CallError::Unsupported,
		other => host_failure(other),
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
