//! [`PgasClaimBuilder`]: what builds a claim, and why it could not.

use std::{collections::BTreeSet, sync::Weak};

use subxt::utils::AccountId32;

use super::claim_extrinsic::{BuiltClaim, ClaimSlot};

/// Builds PGAS claims from the signed-in session and its personhood.
#[async_trait::async_trait]
pub trait PgasClaimBuilder: Send + Sync {
	/// Builds a claim of one slot into `target`, in a slot `excluded` does
	/// not list. Refuses unless a session is signed in now.
	async fn build_claim(
		&self,
		target: AccountId32,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError>;
}

/// Why a claim was not built, which decides what its policy answers.
#[derive(Debug, thiserror::Error)]
pub enum ClaimBuildError {
	/// No session is signed in. The claim waits for one until its deadline.
	#[error("no wallet session is signed in to build a PGAS claim")]
	NoSession,
	/// The signed-in account is in no personhood ring. The claim can never be
	/// built.
	#[error("the signed-in account is in no personhood ring, so it cannot claim PGAS")]
	NoPersonhood,
	/// Every slot of the day is claimed. The claim can never be built.
	#[error("no free PGAS slot on day {day}, all {max} are claimed")]
	NoFreeSlot {
		/// The day scanned.
		day: u32,
		/// The slots that day has.
		max: u32,
	},
	/// A read, the ring revision wait or the proof failed. The claim is asked
	/// about again later.
	#[error("PGAS claim could not be built for now: {0}")]
	Transient(#[source] anyhow::Error),
}

/// For a builder that owns the durable engine, which must not keep it alive
/// in turn. Once the builder is gone, no session is signed in.
#[async_trait::async_trait]
impl<Builder: PgasClaimBuilder> PgasClaimBuilder for Weak<Builder> {
	async fn build_claim(
		&self,
		target: AccountId32,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		let Some(builder) = self.upgrade() else {
			return Err(ClaimBuildError::NoSession);
		};
		builder.build_claim(target, excluded).await
	}
}
