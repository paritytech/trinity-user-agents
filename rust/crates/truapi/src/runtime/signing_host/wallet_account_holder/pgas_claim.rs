//! What a PGAS claim is built from: the Asset Hub it is built for, and the
//! People ring and network suffix that prove the person, read for one wallet
//! session. Builds
//! claims for the durable engine, and claims directly where none runs.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::BTreeSet;

use tracing::debug;

use super::{WalletAccountHolder, allowance::AllowanceAllocationError};
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
use crate::{
	platform::ProductId,
	runtime::statement_allowance::{
		ChainClient, ChainContext, PersonhoodSigner,
		collection::PersonhoodCollection,
		find_including_rings,
		pgas::{PgasClaim, claim_pgas, holds_a_full_claim},
		ring::RingParams,
		rpc::RpcClient,
		slot::read_network_suffix,
	},
};

/// The Asset Hub a PGAS claim is built for, with its runtime.
pub struct PgasAssetHub {
	client: ChainClient,
	context: ChainContext,
}

/// What proves a PGAS claim: the strongest People ring the person is in,
/// with the People chain it was found on, and the network suffix the proof
/// is scoped to.
pub struct ClaimProofContext {
	people: ChainClient,
	context: ChainContext,
	membership: RingParams,
	network_suffix: Vec<u8>,
}

impl PgasAssetHub {
	/// Whether `target` already holds a full claim's worth of PGAS.
	pub async fn holds_a_full_claim(
		&self,
		target: &[u8; 32],
	) -> Result<bool, AllowanceAllocationError> {
		let metadata = &self.context.metadata;
		Ok(holds_a_full_claim(self.client.rpc(), metadata, target).await?)
	}
}

impl WalletAccountHolder {
	/// Connects to Asset Hub for a PGAS claim and reads its runtime.
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
		let host_rpc = subxt_rpcs::RpcClient::new(rpc);
		let client = ChainClient::new(RpcClient::new(host_rpc), genesis);
		let context = self.services.chain_context.get(&client).await?;
		Ok(PgasAssetHub { client, context })
	}

	/// What proves a claim on `asset_hub` for `signer`: its strongest People
	/// ring and Asset Hub's network suffix. Fails when the person is in no
	/// ring.
	pub async fn claim_proof_context(
		&self,
		asset_hub: &PgasAssetHub,
		signer: &dyn PersonhoodSigner,
	) -> Result<ClaimProofContext, AllowanceAllocationError> {
		let network_suffix = read_network_suffix(asset_hub.client.rpc()).await?;
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
		Ok(ClaimProofContext { people, context, membership, network_suffix })
	}

	/// Claims one PGAS slot as `claim` describes, submitting it here and
	/// waiting until the pallet records it in a block.
	pub async fn claim_pgas_directly(
		&self,
		product_id: &ProductId,
		claim: PgasClaim<'_>,
	) -> Result<(), AllowanceAllocationError> {
		let outcome = claim_pgas(claim).await?;
		debug!(
			%product_id,
			day = outcome.day,
			slot_index = outcome.slot_index,
			ring_index = outcome.ring_index,
			block = %outcome.block_hash,
			"claimed PGAS allowance"
		);
		Ok(())
	}

	/// A claim into `target` built for `asset_hub` and proved by `signer`
	/// with `proof`.
	pub fn pgas_claim<'a>(
		&'a self,
		asset_hub: &'a PgasAssetHub,
		proof: &'a ClaimProofContext,
		signer: &'a dyn PersonhoodSigner,
		target: &'a [u8; 32],
	) -> PgasClaim<'a> {
		PgasClaim {
			asset_hub_rpc: asset_hub.client.rpc(),
			asset_hub_metadata: &asset_hub.context.metadata,
			asset_hub_target: asset_hub.client.extrinsic_target(&self.services.extrinsics),
			people_rpc: proof.people.rpc(),
			people_metadata: &proof.context.metadata,
			signer,
			network_suffix: &proof.network_suffix,
			target,
			ring: &proof.membership,
		}
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl WalletAccountHolder {
	/// Builds a claim of one PGAS slot into `target` as the session of
	/// `identity`, skipping `excluded`. Refuses unless that identity is signed
	/// in now.
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
		let proof = self.claim_proof_context(&asset_hub, &signer).await?;
		self.require_current_session(session)?;
		let claim = self.pgas_claim(&asset_hub, &proof, &signer, target);
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
