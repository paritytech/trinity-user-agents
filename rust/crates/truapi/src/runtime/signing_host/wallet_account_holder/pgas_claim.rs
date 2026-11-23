//! Building a durable PGAS claim as the signed-in wallet session:
//! [`WalletAccountHolder::build_pgas_claim_for_identity`].

use std::collections::BTreeSet;

use super::{WalletAccountHolder, allowance::AllowanceAllocationError};
use crate::{
	pgas_claims::{ClaimBuildError, ClaimIdentity},
	runtime::{
		authority::{AccountHolder, AuthorityError, AuthoritySession},
		statement_allowance::{
			ChainClient, ChainContext, PersonhoodSigner, StatementAllowanceError,
			collection::PersonhoodCollection,
			find_including_rings,
			pgas::{BuiltClaim, ClaimSlot, PgasClaim, build_pgas_claim},
			ring::RingParams,
			rpc::RpcClient,
			slot::{SlotError, read_network_suffix},
		},
	},
};

/// The chains a PGAS claim is built from: Asset Hub, where it is submitted,
/// and People, where the person's ring is found, with Asset Hub's network
/// suffix the proof is scoped to.
struct ClaimChains {
	asset_hub: ChainClient,
	asset_hub_context: ChainContext,
	people: ChainClient,
	people_context: ChainContext,
	network_suffix: Vec<u8>,
}

impl WalletAccountHolder {
	/// Builds a claim of one PGAS slot into `target` as the session of
	/// `identity`, in a slot `excluded` does not list. Refuses unless that
	/// identity is signed in now.
	pub async fn build_pgas_claim_for_identity(
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

	/// Builds a claim of one PGAS slot into `target` under `session`, proved
	/// by the person's strongest ring, in a slot `excluded` does not list.
	async fn build_session_pgas_claim(
		&self,
		session: &AuthoritySession,
		target: &[u8; 32],
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, AllowanceAllocationError> {
		let chains = self.pgas_claim_chains().await?;
		let signer = self.personhood_signer(session).await?;
		let ring = chains.strongest_ring(&signer).await?;
		self.require_current_session(session)?;
		let claim = PgasClaim {
			asset_hub_rpc: chains.asset_hub.rpc(),
			asset_hub_metadata: &chains.asset_hub_context.metadata,
			asset_hub_target: chains.asset_hub.extrinsic_target(&self.services.extrinsics),
			people_rpc: chains.people.rpc(),
			people_metadata: &chains.people_context.metadata,
			signer: &signer,
			network_suffix: &chains.network_suffix,
			target,
			ring: &ring,
		};
		Ok(build_pgas_claim(&claim, excluded).await?)
	}

	/// Connects to Asset Hub and People and reads what a claim needs of them.
	async fn pgas_claim_chains(&self) -> Result<ClaimChains, AllowanceAllocationError> {
		let asset_hub_genesis = self.services.asset_hub_chain_genesis_hash();
		let asset_hub_rpc =
			self.services.chain.rpc_client("PGAS claim", &asset_hub_genesis).await.map_err(
				|source| AllowanceAllocationError::ChainRpcClient {
					context: "Asset Hub PGAS client",
					source,
				},
			)?;
		let asset_hub_rpc = RpcClient::new(subxt_rpcs::RpcClient::new(asset_hub_rpc));
		let asset_hub = ChainClient::new(asset_hub_rpc, asset_hub_genesis);
		let people = self.services.statement_store.chain_client("PGAS claim ring").await?;
		Ok(ClaimChains {
			asset_hub_context: self.services.chain_context.get(&asset_hub).await?,
			people_context: self.services.chain_context.get(&people).await?,
			network_suffix: read_network_suffix(asset_hub.rpc()).await?,
			asset_hub,
			people,
		})
	}
}

impl ClaimChains {
	/// The strongest People ring `signer` proves the person in.
	async fn strongest_ring(
		&self,
		signer: &dyn PersonhoodSigner,
	) -> Result<RingParams, AllowanceAllocationError> {
		let metadata = &self.people_context.metadata;
		let collections = &PersonhoodCollection::ALL;
		let rings =
			find_including_rings(self.people.rpc(), metadata, signer, collections, u32::MAX)
				.await?;
		rings
			.into_iter()
			.next()
			.ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })
	}
}

/// How a claim's policy treats `error`: a lost session waits, a person
/// without a ring or a day without a free slot gives up, and anything else
/// is asked about again.
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

#[cfg(test)]
mod tests;
