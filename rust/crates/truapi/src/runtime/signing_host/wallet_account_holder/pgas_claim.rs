//! Building a durable PGAS claim as the signed-in wallet session:
//! [`WalletAccountHolder::build_pgas_claim`].
//!
//! This code answers in [`AllowanceAllocationError`] because it adapts the
//! allowance code it reuses, such as `find_including_rings`, whose errors and
//! interfaces it keeps as they are. The claim policy reads the
//! [`ClaimBuildError`] that error converts into.

use std::collections::BTreeSet;

use super::{WalletAccountHolder, allowance::AllowanceAllocationError};
use crate::{
	pgas_claims::{BuiltClaim, ClaimBuildError, ClaimSlot, build_claim_in_free_slot},
	runtime::{
		authority::{AccountHolder, AuthorityError},
		statement_allowance::{
			ChainClient, ChainContext, PersonhoodSigner, StatementAllowanceError,
			collection::PersonhoodCollection,
			find_including_rings,
			pgas::PgasClaim,
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
	/// Builds a claim of one PGAS slot into `target` under the signed-in
	/// session, proved by the person's strongest ring, in a slot `excluded`
	/// does not list. Fails with [`AuthorityError::Disconnected`] when nobody
	/// is signed in, or when the session ends while the claim is built.
	pub async fn build_pgas_claim(
		&self,
		target: &[u8; 32],
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, AllowanceAllocationError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		let chains = self.pgas_claim_chains().await?;
		let signer = self.personhood_signer(&session).await?;
		let ring = chains.strongest_ring(&signer).await?;
		self.require_current_session(&session)?;
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
		Ok(build_claim_in_free_slot(&claim, excluded).await?)
	}

	/// Connects to Asset Hub and People and reads what a claim needs of them.
	async fn pgas_claim_chains(&self) -> Result<ClaimChains, AllowanceAllocationError> {
		let chains = self.services.stack().chains();
		let asset_hub = self.pgas_chain_client("Asset Hub PGAS client", chains.asset_hub).await?;
		let people = self.pgas_chain_client("People PGAS ring client", chains.people).await?;
		Ok(ClaimChains {
			asset_hub_context: self.services.chain_context.get(&asset_hub).await?,
			people_context: self.services.chain_context.get(&people).await?,
			network_suffix: read_network_suffix(asset_hub.rpc()).await?,
			asset_hub,
			people,
		})
	}

	/// A client of the chain with `genesis`, scoped to it for the chain
	/// context cache, named `context` when it cannot connect.
	async fn pgas_chain_client(
		&self,
		context: &'static str,
		genesis: [u8; 32],
	) -> Result<ChainClient, AllowanceAllocationError> {
		let rpc = self
			.services
			.chain
			.rpc_client("PGAS claim", &genesis)
			.await
			.map_err(|source| AllowanceAllocationError::ChainRpcClient { context, source })?;
		Ok(ChainClient::new(RpcClient::new(subxt_rpcs::RpcClient::new(rpc)), genesis))
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

/// How a claim's policy treats an error: a lost session waits, a person
/// without a ring or a day without a free slot gives up, and anything else
/// is asked about again.
impl From<AllowanceAllocationError> for ClaimBuildError {
	fn from(error: AllowanceAllocationError) -> Self {
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
}

#[cfg(test)]
mod tests;
