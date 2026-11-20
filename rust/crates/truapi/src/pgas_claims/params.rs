//! [`ClaimParams`]: what the ledger stores with each claim.

use parity_scale_codec::{Decode, Encode};
use subxt::utils::AccountId32;

use super::builder::ClaimIdentity;
use crate::{
	durable::PolicyParams,
	runtime::statement_allowance::pgas::{BuiltClaim, ClaimSlot},
};

/// Everything a claim is built and decided from, so neither reads a table.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ClaimParams {
	/// The account the claim mints into.
	pub target: AccountId32,
	/// The account that registered the claim, the only one that builds it.
	pub identity: ClaimIdentity,
	/// Unix time, in seconds of the host's clock, after which a claim still
	/// waiting for a session is given up. Fixed at registration, so a restart
	/// never extends it.
	pub give_up_after: u64,
	/// The live attempt. `None` until the first build.
	pub attempt: Option<ClaimAttempt>,
}

/// The slot an attempt claims and the alias the pallet records once it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ClaimAttempt {
	/// The slot claimed.
	pub slot: ClaimSlot,
	/// The person's alias for `slot`, which `Pgas.ClaimedGasAliases` records.
	pub alias: [u8; 32],
}

impl From<&BuiltClaim> for ClaimAttempt {
	fn from(built: &BuiltClaim) -> Self {
		Self { slot: built.slot, alias: built.alias }
	}
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
