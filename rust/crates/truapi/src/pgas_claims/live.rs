//! A durable PGAS claim on the live Paseo Next Asset Hub. Ignored by default;
//! run with
//! `cargo test -p truapi --all-features --lib pgas_claims::live -- --ignored --nocapture`.
//!
//! Needs a person in a People ring: `TRUAPI_LIVE_PERSON_MNEMONIC` names one,
//! and the test skips without it. Each run spends one of the person's PGAS
//! slots for the day. `TRUAPI_LIVE_PEOPLE_WS`, `LIVE_ASSET_HUB_WS` and
//! `LIVE_ASSET_HUB_GENESIS` point it at other nodes.

use core::time::Duration;
use std::{collections::BTreeSet, sync::Arc};

use futures::{StreamExt, future::ready};
use subxt::utils::{AccountId32, H256};

use super::{
	BuiltClaim, ClaimBuildError, ClaimSlot, PgasClaimBuilder, PgasClaims, build_claim_in_free_slot,
	with_pgas_claims,
};
use crate::{
	chain::{ExtrinsicService, FixedExtensionVersions, live_asset_hub, live_asset_hub_genesis},
	chain_runtime::ChainRuntime,
	durable::{DurableRegistry, DurableTxEngine, DurableTxId, DurableTxStatus, live_engine},
	host_logic::root_entropy::RootEntropy,
	nft::ASSET_HUB_PIPELINE,
	runtime::statement_allowance::{
		self as alloc, CollectionCandidate, FixedPersonhoodSigner, StatementAllowanceError,
		collection::PersonhoodCollection,
		pgas::{PgasClaim, holds_a_full_claim},
		ring::RingParams,
	},
	store::open_temporary_core_db,
};

/// The person behind a mnemonic, with the chains its claims are built from.
struct LivePerson {
	entropy: RootEntropy,
	network_suffix: Vec<u8>,
	asset_hub: alloc::rpc::RpcClient,
	asset_hub_metadata: alloc::extension::Metadata,
	people: alloc::rpc::RpcClient,
	people_metadata: alloc::extension::Metadata,
	extrinsics: ExtrinsicService,
	genesis: H256,
}

impl LivePerson {
	/// The person behind `entropy`, on `chain`, the Asset Hub with `genesis`,
	/// and the People node the environment names.
	async fn connect(chain: &ChainRuntime, genesis: H256, entropy: RootEntropy) -> Self {
		let asset_hub_rpc = chain.rpc_client("live PGAS claim", &genesis.0).await.unwrap();
		let asset_hub = alloc::rpc::RpcClient::new(subxt_rpcs::RpcClient::new(asset_hub_rpc));
		let people_ws = std::env::var("TRUAPI_LIVE_PEOPLE_WS")
			.unwrap_or_else(|_| "wss://paseo-people-next-system-rpc.polkadot.io".into());
		let people = alloc::rpc::RpcClient::connect(&people_ws).await.unwrap();
		let versions = FixedExtensionVersions::one_chain(genesis, ASSET_HUB_PIPELINE);
		Self {
			entropy,
			network_suffix: alloc::slot::read_network_suffix(&asset_hub).await.unwrap(),
			asset_hub_metadata: alloc::fetch_metadata(&asset_hub).await.unwrap(),
			people_metadata: alloc::fetch_metadata(&people).await.unwrap(),
			asset_hub,
			people,
			extrinsics: ExtrinsicService::over_chain(chain.clone(), Arc::new(versions)),
			genesis,
		}
	}

	/// The person's ring-VRF keys in each collection.
	fn candidates(&self) -> [CollectionCandidate; 2] {
		let suffix = std::str::from_utf8(&self.network_suffix).unwrap();
		[
			CollectionCandidate {
				collection: PersonhoodCollection::People,
				entropy: self.entropy.derive_full_person_ring_vrf_entropy(suffix),
			},
			CollectionCandidate {
				collection: PersonhoodCollection::LitePeople,
				entropy: self.entropy.derive_lite_person_ring_vrf_entropy(suffix),
			},
		]
	}

	/// A claim of one slot into `target`, in the person's strongest ring.
	async fn build(
		&self,
		target: AccountId32,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, StatementAllowanceError> {
		let candidates = self.candidates();
		let signer = FixedPersonhoodSigner::new(&candidates).await?;
		let ring = self.strongest_ring(&signer).await?;
		let claim = PgasClaim {
			asset_hub_rpc: &self.asset_hub,
			asset_hub_metadata: &self.asset_hub_metadata,
			asset_hub_target: alloc::ExtrinsicTarget {
				extrinsics: &self.extrinsics,
				genesis: self.genesis,
			},
			people_rpc: &self.people,
			people_metadata: &self.people_metadata,
			signer: &signer,
			network_suffix: &self.network_suffix,
			target: &target.0,
			ring: &ring,
		};
		build_claim_in_free_slot(&claim, excluded).await
	}

	/// The strongest People ring `signer` proves the person in.
	async fn strongest_ring(
		&self,
		signer: &FixedPersonhoodSigner<'_>,
	) -> Result<RingParams, StatementAllowanceError> {
		let collections = &PersonhoodCollection::ALL;
		let people = &self.people;
		let rings = alloc::find_including_rings(
			people,
			&self.people_metadata,
			signer,
			collections,
			u32::MAX,
		)
		.await?;
		Ok(rings.into_iter().next().expect("the person is in a People ring"))
	}
}

/// The first status of claim `id` past submission: in a block, or failed.
/// Panics when none comes within three minutes, the inclusion wait of a
/// direct claim plus the ring revision wait of a build.
async fn settled_status(engine: &DurableTxEngine, id: DurableTxId) -> DurableTxStatus {
	let under_way = |status: &DurableTxStatus| {
		matches!(status, DurableTxStatus::PendingSubmission | DurableTxStatus::Pending)
	};
	let statuses = engine.observe_status(id).map(|read| read.unwrap().expect("the claim is kept"));
	let mut settled = statuses.filter(|status| ready(!under_way(status)));
	let first_settled = tokio::time::timeout(Duration::from_secs(180), settled.next());
	first_settled
		.await
		.expect("the claim settles in time")
		.expect("the ledger stays open")
}

#[async_trait::async_trait]
impl PgasClaimBuilder for LivePerson {
	async fn build_claim(
		&self,
		target: AccountId32,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		self.build(target, excluded)
			.await
			.map_err(|error| ClaimBuildError::Transient(error.into()))
	}
}

/// A claim the engine builds, submits and follows mints PGAS into a fresh
/// account, and the engine sees it reach a block from its extrinsic alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Paseo Next chains and a person's mnemonic"]
async fn a_durable_claim_lands_on_asset_hub_and_the_engine_sees_it() {
	let Ok(mnemonic) = std::env::var("TRUAPI_LIVE_PERSON_MNEMONIC") else {
		eprintln!("skipped: TRUAPI_LIVE_PERSON_MNEMONIC is not set");
		return;
	};
	let entropy = RootEntropy::new(bip39::Mnemonic::parse(mnemonic.trim()).unwrap().to_entropy());
	let genesis = live_asset_hub_genesis();
	let chain = live_asset_hub(genesis);
	let person = Arc::new(LivePerson::connect(&chain, genesis, entropy).await);
	let (_dir, db) = open_temporary_core_db();
	let registry = with_pgas_claims(DurableRegistry::new(), genesis, person.clone());
	let engine = live_engine(&chain, db, registry);
	let claims = PgasClaims::new(engine.clone());
	let dest = AccountId32(rand::random());

	let id = claims.claim(dest).await.unwrap();
	let settled = settled_status(&engine, id).await;

	assert!(
		matches!(
			settled,
			DurableTxStatus::PendingSuccess { .. } | DurableTxStatus::FinalizedSuccess { .. }
		),
		"the claim reaches a block, got {settled:?}"
	);

	let funded = holds_a_full_claim(&person.asset_hub, &person.asset_hub_metadata, &dest.0);
	assert!(funded.await.unwrap(), "the destination holds a full claim");
}
