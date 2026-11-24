//! [`MovePayments`]: how each source of one registration pays for its move,
//! decided before the registration's write, with the PGAS claims its paid
//! moves wait for.

use std::collections::HashMap;

use subxt::utils::AccountId32;

use super::{
	keys::ItemKey,
	operation::StartOperationError,
	scarcity::{DestinationKind, MoveBudget, TransferMode, TransferParams},
};
use crate::{
	chain::StorageWatchError,
	durable::{DeferredSubmission, DurableTxId, RegistrationError, TransactionDependency},
	pgas_claims::{PgasClaimError, PgasClaims},
};

/// How each source of one registration pays for its move.
pub struct MovePayments {
	by_source: HashMap<ItemKey, MovePayment>,
}

impl MovePayments {
	/// How `source` pays. Refuses a source the registration did not plan,
	/// which a concurrent spend can cause when the write picks its sources.
	pub fn payment_of(&self, source: &ItemKey) -> Result<&MovePayment, UnplannedSource> {
		self.by_source.get(source).ok_or(UnplannedSource(*source))
	}
}

/// How one move pays.
pub enum MovePayment {
	/// It moves fee-less and waits for nothing.
	FeeLess,
	/// It moves paid, once the PGAS claim `claim` into its source took effect.
	PaidByClaim {
		/// The claim it waits for.
		claim: DurableTxId,
	},
}

impl MovePayment {
	/// The mode the move is stored with.
	pub fn transfer_mode(&self) -> TransferMode {
		match self {
			Self::FeeLess => TransferMode::FeeLess,
			Self::PaidByClaim { .. } => TransferMode::Paid,
		}
	}

	/// The move `params` describes, waiting for its claim when it is paid. A
	/// failed claim releases it all the same, and its policy gives up.
	pub fn submission(&self, params: TransferParams) -> DeferredSubmission {
		let depends_on = match self {
			Self::FeeLess => vec![],
			Self::PaidByClaim { claim } => vec![TransactionDependency::new(*claim)],
		};
		DeferredSubmission { params: params.into(), depends_on }
	}
}

/// A source the registration write picked that was not planned before it.
#[derive(Debug, thiserror::Error)]
#[error("item {} was not planned before its registration", hex::encode(.0.0))]
pub struct UnplannedSource(pub ItemKey);

/// Why the moves of a registration could not be paid for. Nothing of the
/// moves was registered; claims registered before the failure stay.
#[derive(Debug, thiserror::Error)]
pub enum PaidMoveError {
	/// The runtime's move budget could not be read.
	#[error("the fee-less move budget could not be read: {0}")]
	Budget(#[source] StorageWatchError),
	/// A claim into a paid move's source was not registered.
	#[error("a PGAS claim for a paid move was not registered: {0}")]
	Claim(#[source] RegistrationError<PgasClaimError>),
}

/// The mode of each of `sources`, from the fee-less moves the tracker
/// recorded, as moves to `destination` under `budget`. Refuses a source
/// whose moves are not recorded yet as unavailable.
pub fn transfer_modes(
	budget: &MoveBudget,
	sources: &[(ItemKey, Option<u16>)],
	destination: DestinationKind,
) -> Result<Vec<(ItemKey, TransferMode)>, StartOperationError> {
	let mode_of = |&(source, moves): &(ItemKey, Option<u16>)| {
		let moves = moves.ok_or(StartOperationError::ItemUnavailable(source))?;
		Ok((source, budget.transfer_mode(moves, destination)))
	};
	sources.iter().map(mode_of).collect()
}

/// Registers a PGAS claim into each source `modes` makes paid, or reuses the
/// claim already active there, and returns how every source pays.
pub async fn claim_paid_sources(
	claims: &PgasClaims,
	modes: Vec<(ItemKey, TransferMode)>,
) -> Result<MovePayments, PaidMoveError> {
	let mut by_source = HashMap::new();
	for (source, mode) in modes {
		let payment = match mode {
			TransferMode::FeeLess => MovePayment::FeeLess,
			TransferMode::Paid => MovePayment::PaidByClaim { claim: new_or_active_claim(claims, source).await? },
		};
		by_source.insert(source, payment);
	}
	Ok(MovePayments { by_source })
}

/// The claim into `source`: a new one, or the one already active.
async fn new_or_active_claim(claims: &PgasClaims, source: ItemKey) -> Result<DurableTxId, PaidMoveError> {
	match claims.claim(AccountId32(source.0)).await {
		Ok(claim) | Err(RegistrationError::Hook(PgasClaimError::AlreadyClaiming(claim))) => Ok(claim),
		Err(error) => Err(PaidMoveError::Claim(error)),
	}
}

#[cfg(test)]
mod tests;
