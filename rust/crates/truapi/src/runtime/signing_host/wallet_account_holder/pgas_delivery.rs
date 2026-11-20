//! PGAS allowances served by durable claims, wherever the core runs its
//! durable engine.

use subxt::utils::AccountId32;
use tracing::debug;

use super::allowance::AllowanceAllocationError;
use crate::{
	CancellationToken,
	pgas_claims::{ClaimIdentity, PgasClaims},
	runtime::authority::AuthoritySession,
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
	/// after the wait limit, or once `cancel` fires, leaving the claim to
	/// carry on in the ledger.
	pub async fn deliver(
		self,
		claims: &PgasClaims,
		cancel: &CancellationToken,
	) -> Result<(), AllowanceAllocationError> {
		let claim = claims.claim_or_join(self.target, self.identity).await?;
		claims
			.wait_until_delivered(claim, cancel.cancelled())
			.await
			.map_err(|reason| AllowanceAllocationError::PgasClaimNotDelivered { claim, reason })?;
		debug!(claim = claim.0, "PGAS allowance claim reached a block");
		Ok(())
	}
}

#[cfg(test)]
mod tests;
