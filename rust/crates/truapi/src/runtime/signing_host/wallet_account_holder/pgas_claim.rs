//! What a PGAS claim is built from: the Asset Hub it is built for and the
//! People ring that proves the person, read for one wallet session.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::BTreeSet;

use super::{WalletAccountHolder, allowance::AllowanceAllocationError};
use crate::runtime::statement_allowance::{
	ChainClient, ChainContext, PersonhoodSigner, collection::PersonhoodCollection,
	find_including_rings, pgas::PgasClaim, ring::RingParams, rpc::RpcClient,
	slot::read_network_suffix,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
	pgas_claims::{ClaimBuildError, ClaimIdentity},
	runtime::{
		authority::{AccountHolder, AuthorityError, AuthoritySession},
		statement_allowance::{
			StatementAllowanceError,
			pgas::{BuiltClaim, ClaimSlot, build_pgas_claim},
			slot::SlotError,
		},
	},
};

/// The Asset Hub a PGAS claim is built for, with its runtime and the suffix
/// its proofs are scoped to.
pub struct PgasAssetHub {
	/// The connection the claim is read and submitted over.
	pub client: ChainClient,
	/// Its runtime metadata.
	pub context: ChainContext,
	/// The runtime-wide suffix of the claim's alias and proof context.
	pub network_suffix: Vec<u8>,
}

/// The People ring a PGAS claim proves the person in, with the People chain
/// it was found on.
pub struct PersonhoodRing {
	people: ChainClient,
	context: ChainContext,
	membership: RingParams,
}

impl WalletAccountHolder {
	/// Connects to Asset Hub for a PGAS claim and reads its runtime and
	/// network suffix.
	pub async fn pgas_asset_hub(&self) -> Result<PgasAssetHub, AllowanceAllocationError> {
		let genesis = self.services.asset_hub_chain_genesis_hash();
		let rpc =
			self.services
				.chain
				.rpc_client("PGAS allowance", &genesis)
				.await
				.map_err(|source| AllowanceAllocationError::ChainRpcClient {
					context: "Asset Hub PGAS client",
					source,
				})?;
		let client = ChainClient::new(RpcClient::new(subxt_rpcs::RpcClient::new(rpc)), genesis);
		let context = self.services.chain_context.get(&client).await?;
		let network_suffix = read_network_suffix(client.rpc()).await?;
		Ok(PgasAssetHub { client, context, network_suffix })
	}

	/// The strongest People ring `signer` is a member of, as a PGAS claim
	/// uses it. Fails when the person is in no ring.
	pub async fn strongest_personhood_ring(
		&self,
		signer: &dyn PersonhoodSigner,
	) -> Result<PersonhoodRing, AllowanceAllocationError> {
		let people = self.services.statement_store.chain_client("PGAS allowance ring").await?;
		let context = self.services.chain_context.get(&people).await?;
		let collections = &PersonhoodCollection::ALL;
		let rings =
			find_including_rings(people.rpc(), &context.metadata, signer, collections, u32::MAX)
				.await?;
		let membership = rings
			.into_iter()
			.next()
			.ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })?;
		Ok(PersonhoodRing { people, context, membership })
	}

	/// A claim into `target` built for `asset_hub` and proved by `signer` in
	/// `ring`.
	pub fn pgas_claim<'a>(
		&'a self,
		asset_hub: &'a PgasAssetHub,
		ring: &'a PersonhoodRing,
		signer: &'a dyn PersonhoodSigner,
		target: &'a [u8; 32],
	) -> PgasClaim<'a> {
		PgasClaim {
			asset_hub_rpc: asset_hub.client.rpc(),
			asset_hub_metadata: &asset_hub.context.metadata,
			asset_hub_target: asset_hub.client.extrinsic_target(&self.services.extrinsics),
			people_rpc: ring.people.rpc(),
			people_metadata: &ring.context.metadata,
			signer,
			network_suffix: &asset_hub.network_suffix,
			target,
			ring: &ring.membership,
		}
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl WalletAccountHolder {
	/// Builds a claim of one PGAS slot into `target` as the session of
	/// `identity`, skipping `excluded`. Refuses unless that identity is signed
	/// in now.
	pub async fn build_pgas_claim_as(
		&self,
		target: [u8; 32],
		identity: ClaimIdentity,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		let session = self.current_session().ok_or(ClaimBuildError::NoSession)?;
		if ClaimIdentity::from_root_public_key(session.public_key) != identity {
			return Err(ClaimBuildError::OtherIdentity);
		}
		self.build_session_pgas_claim(&session, &target, excluded)
			.await
			.map_err(claim_build_error)
	}

	/// Builds a claim of one PGAS slot into `target` under `session`,
	/// skipping `excluded`.
	async fn build_session_pgas_claim(
		&self,
		session: &AuthoritySession,
		target: &[u8; 32],
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, AllowanceAllocationError> {
		let asset_hub = self.pgas_asset_hub().await?;
		let signer = self.personhood_signer(session).await?;
		let ring = self.strongest_personhood_ring(&signer).await?;
		self.require_current_session(session)?;
		let claim = self.pgas_claim(&asset_hub, &ring, &signer, target);
		Ok(build_pgas_claim(&claim, excluded).await?)
	}
}

/// How a claim's policy treats `error`: a lost session waits, a person
/// without a ring or a day without a free slot gives up, and anything else
/// is asked about again.
#[cfg(not(target_arch = "wasm32"))]
fn claim_build_error(error: AllowanceAllocationError) -> ClaimBuildError {
	use AllowanceAllocationError::{Authority, MissingPersonhoodMembership, StatementAllowance};

	match error {
		Authority(AuthorityError::Disconnected) |
		StatementAllowance(StatementAllowanceError::Authority(AuthorityError::Disconnected)) => {
			ClaimBuildError::NoSession
		},
		MissingPersonhoodMembership { .. } |
		StatementAllowance(StatementAllowanceError::Slot(SlotError::NoCollectionMembership)) => {
			ClaimBuildError::NoPersonhood
		},
		StatementAllowance(StatementAllowanceError::Slot(SlotError::NoFreePgasSlot {
			day,
			max,
		})) => ClaimBuildError::NoFreeSlot { day, max },
		other => ClaimBuildError::Transient(other.into()),
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
