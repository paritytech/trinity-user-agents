//! PGAS allowances served by durable claims, wherever the core runs its
//! durable engine.

use subxt::utils::AccountId32;
use tracing::debug;

use super::allowance::AllowanceAllocationError;
use crate::{
	CallContext,
	durable::DurableTxId,
	pgas_claims::{ClaimIdentity, PgasClaims, UndeliveredClaim},
	runtime::{authority::AuthoritySession, authority_cancellation_error},
};

/// A PGAS allowance request a durable claim serves: the account it credits,
/// and the identity whose session asked.
pub struct DurablePgasClaim {
	target: AccountId32,
	identity: ClaimIdentity,
}

impl DurablePgasClaim {
	/// The request `session` makes to credit `target`.
	pub fn new(session: &AuthoritySession, target: [u8; 32]) -> Self {
		let identity = ClaimIdentity::from_root_public_key(session.public_key);
		Self { target: AccountId32(target), identity }
	}

	/// Waits until a claim into the target reaches a block: the target's
	/// active claim, or else one registered now. Fails when the claim fails,
	/// after the wait limit, or once `call` is cancelled, leaving the claim to
	/// carry on in the ledger.
	pub async fn deliver(
		self,
		claims: &PgasClaims,
		call: &CallContext,
	) -> Result<(), AllowanceAllocationError> {
		let claim = claims.claim_or_join(self.target, self.identity).await?;
		let delivered = claims.wait_until_delivered(claim, call.cancel().cancelled()).await;
		delivered.map_err(|reason| undelivered_claim_error(call, claim, reason))?;
		debug!(claim = claim.0, "PGAS allowance claim reached a block");
		Ok(())
	}
}

/// What a request answers when its claim `claim` was not delivered for
/// `reason`. A cancelled request answers as every cancelled authority call
/// does.
fn undelivered_claim_error(
	call: &CallContext,
	claim: DurableTxId,
	reason: UndeliveredClaim,
) -> AllowanceAllocationError {
	match reason {
		UndeliveredClaim::Cancelled(cancellation) => {
			AllowanceAllocationError::Authority(authority_cancellation_error(call, cancellation))
		},
		reason => AllowanceAllocationError::PgasClaimNotDelivered { claim, reason },
	}
}

#[cfg(test)]
mod tests;
