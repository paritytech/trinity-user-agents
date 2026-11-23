//! [`ClaimParams`]: what the ledger stores with each claim.

use parity_scale_codec::{Decode, Encode};
use subxt::utils::AccountId32;

use super::builder::ClaimIdentity;
use crate::durable::PolicyParams;

/// Everything a claim is built from, so building it reads no table.
#[derive(Clone, Encode, Decode)]
pub struct ClaimParams {
	/// The account the claim mints into.
	pub target: AccountId32,
	/// The account that registered the claim, the only one that builds it.
	pub identity: ClaimIdentity,
	/// Unix time, in seconds of the host's clock, after which no attempt of
	/// the claim is built. Fixed at registration, so a restart never extends
	/// it.
	pub give_up_after: u64,
}

impl From<ClaimParams> for PolicyParams {
	fn from(params: ClaimParams) -> Self {
		PolicyParams(params.encode())
	}
}

impl TryFrom<&PolicyParams> for ClaimParams {
	type Error = parity_scale_codec::Error;

	fn try_from(params: &PolicyParams) -> Result<Self, Self::Error> {
		Self::decode(&mut &params.0[..])
	}
}
