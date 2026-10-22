//! The chain operations of NFT purses: registering a rebalance or a transfer,
//! and following an operation's status.

use std::collections::BTreeSet;

use futures::StreamExt;
use truapi::{CallError, Subscription, latest};

use super::host_failure;
use crate::{
	durable::RegistrationError,
	nft::{AllocateItemsError, ItemKey, PurseId, StartOperationError},
	nft_purse::{NftPurses, PurseOwner, drain_needs_confirmation},
	platform::{NftPurseDrainDestination, NftPurseDrainReview, UserConfirmationReview},
	runtime::ProductRuntimeHost,
};

/// How one call's error type names the refusals its operation can meet.
struct OperationRefusals<E> {
	/// The user did not confirm draining the purse.
	denied: E,
	/// The operation names no item.
	no_items: E,
	/// Some item is not held in the purse.
	item_not_found: E,
	/// Some item is in use.
	item_unavailable: E,
	/// An operation is already registered under the key.
	operation_exists: E,
	/// Any other refusal that is the product's.
	other: fn(latest::GenericError) -> E,
}

const REBALANCE_REFUSALS: OperationRefusals<latest::HostNftPurseRebalancePurseError> =
	OperationRefusals {
		denied: latest::HostNftPurseRebalancePurseError::Denied,
		no_items: latest::HostNftPurseRebalancePurseError::NoItems,
		item_not_found: latest::HostNftPurseRebalancePurseError::ItemNotFound,
		item_unavailable: latest::HostNftPurseRebalancePurseError::ItemUnavailable,
		operation_exists: latest::HostNftPurseRebalancePurseError::OperationExists,
		other: latest::HostNftPurseRebalancePurseError::Other,
	};

const TRANSFER_REFUSALS: OperationRefusals<latest::HostNftPurseTransferItemError> =
	OperationRefusals {
		denied: latest::HostNftPurseTransferItemError::Denied,
		no_items: latest::HostNftPurseTransferItemError::ItemNotFound,
		item_not_found: latest::HostNftPurseTransferItemError::ItemNotFound,
		item_unavailable: latest::HostNftPurseTransferItemError::ItemUnavailable,
		operation_exists: latest::HostNftPurseTransferItemError::OperationExists,
		other: latest::HostNftPurseTransferItemError::Other,
	};

impl<E> OperationRefusals<E> {
	/// `error` as the call's error: the product's refusals as its domain
	/// errors, everything else as the host's failure.
	fn refusal_of(self, error: RegistrationError<StartOperationError>) -> CallError<E> {
		let refusal = match error {
			RegistrationError::Hook(refusal) => refusal,
			RegistrationError::UnknownDomain(_) => return CallError::Unsupported,
			other => return host_failure(other),
		};
		let domain = match refusal {
			StartOperationError::NoItems => self.no_items,
			StartOperationError::OperationExists => self.operation_exists,
			StartOperationError::ItemNotFound(_) => self.item_not_found,
			StartOperationError::ItemUnavailable(_) => self.item_unavailable,
			StartOperationError::Allocate(
				refused @ (AllocateItemsError::UnknownPurse(_) |
				AllocateItemsError::IndicesExhausted { .. }),
			) => (self.other)(latest::GenericError { reason: refused.to_string() }),
			other => return host_failure(other),
		};
		CallError::Domain(domain)
	}
}

/// A drain of `item_count` items out of `from` to `destination`.
struct Drain {
	from: PurseId,
	owner: PurseOwner,
	item_count: u32,
	destination: NftPurseDrainDestination,
}

impl ProductRuntimeHost {
	/// Registers a rebalance after checking both purses and confirming a
	/// drain of a purse the caller does not own.
	pub async fn rebalance_nft_purse(
		&self,
		request: latest::HostNftPurseRebalancePurseRequest,
	) -> Result<(), CallError<latest::HostNftPurseRebalancePurseError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseRebalancePurseRequest { from, to, keys, idempotency_key } = request;
		let items: BTreeSet<ItemKey> = keys.iter().map(|key| ItemKey(key.0)).collect();
		if items.is_empty() {
			return Err(CallError::Domain(REBALANCE_REFUSALS.no_items));
		}
		let (from, to) = (PurseId(from.0), PurseId(to.0));
		let drain = rebalance_drain(&purses, from, to, items.len()).await?;
		self.confirm_drain(drain, REBALANCE_REFUSALS.denied).await?;
		let caller = self.connection.product_id();
		let started = purses.rebalance(caller, from, to, items, idempotency_key).await;
		started.map_err(|error| REBALANCE_REFUSALS.refusal_of(error))
	}

	/// Registers a transfer of an item of ours to an outside key, after
	/// confirming a drain of a purse the caller does not own.
	pub async fn transfer_nft_item(
		&self,
		request: latest::HostNftPurseTransferItemRequest,
	) -> Result<(), CallError<latest::HostNftPurseTransferItemError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseTransferItemRequest { item, destination, idempotency_key } =
			request;
		let not_found = || CallError::Domain(TRANSFER_REFUSALS.item_not_found);
		let from = purses.purse_of(item).await.map_err(host_failure)?.ok_or_else(not_found)?;
		let owner = purses.owner_of(from).await.map_err(host_failure)?.ok_or_else(not_found)?;
		let to_key = NftPurseDrainDestination::ExternalKey { key: destination.0 };
		let drain = Drain { from, owner, item_count: 1, destination: to_key };
		self.confirm_drain(drain, TRANSFER_REFUSALS.denied).await?;
		let caller = self.connection.product_id();
		let started = purses.transfer(caller, from, item, destination, idempotency_key).await;
		started.map_err(|error| TRANSFER_REFUSALS.refusal_of(error))
	}

	/// The status of an operation the caller registered, until it ends.
	pub fn observe_nft_operation(
		&self,
		request: latest::HostNftPurseObserveOperationStatusRequest,
	) -> Subscription<
		latest::NftPurseStatus,
		CallError<latest::HostNftPurseObserveOperationStatusError>,
	> {
		if let Err(error) = self.require_session() {
			return Subscription::interrupted(error);
		}
		let purses = match self.connection.nft_purses() {
			Ok(purses) => purses,
			Err(error) => return Subscription::interrupted(error),
		};
		let caller = self.connection.product_id();
		let statuses = purses.observe_status(caller, request.idempotency_key);
		let found =
			|read: Result<Option<_>, _>| read.map_err(host_failure)?.ok_or(OPERATION_NOT_FOUND);
		Subscription::new(statuses.map(found))
	}

	/// Asks the user to confirm `drain` when the caller does not own its
	/// purse. A refusal is `denied`.
	async fn confirm_drain<E>(&self, drain: Drain, denied: E) -> Result<(), CallError<E>> {
		let caller = self.connection.product_id();
		if !drain_needs_confirmation(caller, &drain.owner) {
			return Ok(());
		}
		let review = UserConfirmationReview::NftPurseDrain(NftPurseDrainReview {
			calling_product_id: caller.clone(),
			from_purse: drain.from.0,
			owner: drain.owner.to_wire(),
			item_count: drain.item_count,
			destination: drain.destination,
		});
		let confirmed = self.connection.confirm_user_action_if_needed(review).await;
		let confirmed = confirmed.map_err(|error| host_failure(error.reason))?;
		if confirmed { Ok(()) } else { Err(CallError::Domain(denied)) }
	}
}

const OPERATION_NOT_FOUND: CallError<latest::HostNftPurseObserveOperationStatusError> =
	CallError::Domain(latest::HostNftPurseObserveOperationStatusError::OperationNotFound);

/// The drain a rebalance of `item_count` items from `from` into `to` makes,
/// refusing it when either purse does not exist.
async fn rebalance_drain(
	purses: &NftPurses,
	from: PurseId,
	to: PurseId,
	item_count: usize,
) -> Result<Drain, CallError<latest::HostNftPurseRebalancePurseError>> {
	let owner = owner_or_purse_not_found(purses, from).await?;
	owner_or_purse_not_found(purses, to).await?;
	let item_count = u32::try_from(item_count).map_err(host_failure)?;
	let destination = NftPurseDrainDestination::Purse { purse: to.0 };
	Ok(Drain { from, owner, item_count, destination })
}

/// The owner of `purse`, refusing a purse that does not exist.
async fn owner_or_purse_not_found(
	purses: &NftPurses,
	purse: PurseId,
) -> Result<PurseOwner, CallError<latest::HostNftPurseRebalancePurseError>> {
	let owner = purses.owner_of(purse).await.map_err(host_failure)?;
	owner.ok_or(CallError::Domain(latest::HostNftPurseRebalancePurseError::PurseNotFound))
}
