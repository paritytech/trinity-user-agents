//! Refunds: returning the items of a deposited cheque to the cheque's keys.

use truapi::latest::{HostNftPurseRefundResponse, NftPurseChequeId, NftPurseIdempotencyKey};

use super::{
	NftPurses,
	deposit::deposit_operation,
	item_key,
	operation::{cheque_operation_key, operation_key},
};
use crate::{
	durable::RegistrationError,
	nft::{PurseId, ReturnRequest, StartOperationError},
	platform::ProductId,
};

/// Domain of the hash that derives the idempotency key of a cheque's refund.
const REFUND_KEY_DOMAIN: &[u8] = b"nft-purse/refund";

/// The purse a refund takes items out of, and how many.
#[derive(Debug)]
pub struct RefundDrain {
	/// The purse the deposit moved the items into.
	pub from: PurseId,
	/// How many of them the refund would move now.
	pub item_count: usize,
}

impl NftPurses {
	/// What a refund of `depositor`'s deposit of cheque `id` would take, read
	/// without registering anything, so the user confirms what will move.
	/// Refuses as [`Self::refund`] would.
	pub async fn refund_drain(
		&self,
		depositor: &ProductId,
		id: &NftPurseChequeId,
	) -> Result<RefundDrain, StartOperationError> {
		let slots = self.ledger.slots_to_return(refund_request(depositor, id)).await?;
		let from = slots.first().ok_or(StartOperationError::NothingIdle)?.purse;
		Ok(RefundDrain { from, item_count: slots.len() })
	}

	/// Starts moving the items `depositor` deposited from cheque `id` back to
	/// the cheque's keys, once every move of the deposit is final. Items spent
	/// since stay and are listed as unavailable.
	pub async fn refund(
		&self,
		depositor: &ProductId,
		id: &NftPurseChequeId,
	) -> Result<HostNftPurseRefundResponse, RegistrationError<StartOperationError>> {
		let started = self.ledger.start_returns(refund_request(depositor, id)).await?;
		Ok(HostNftPurseRefundResponse {
			started: started.started.into_iter().map(item_key).collect(),
			unavailable: started.unavailable.into_iter().map(item_key).collect(),
		})
	}
}

/// The return of `depositor`'s deposit of cheque `id`, under its refund key.
fn refund_request(depositor: &ProductId, id: &NftPurseChequeId) -> ReturnRequest {
	ReturnRequest {
		operation: operation_key(depositor, refund_key(id)),
		import: deposit_operation(depositor, id),
	}
}

/// The idempotency key the refund of cheque `id` is observed under.
fn refund_key(id: &NftPurseChequeId) -> NftPurseIdempotencyKey {
	cheque_operation_key(REFUND_KEY_DOMAIN, id)
}
