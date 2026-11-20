//! A durable PGAS claim on the live Paseo Next Asset Hub. Ignored by default;
//! run with
//! `cargo test -p truapi --all-features --lib pgas_claims::live -- --ignored --nocapture`.
//!
//! Needs a person in a People ring: `TRUAPI_LIVE_PERSON_MNEMONIC` names one,
//! and the test skips without it. Each run spends one of the person's PGAS
//! slots for the day. `TRUAPI_LIVE_PEOPLE_WS`, `LIVE_ASSET_HUB_WS` and
//! `LIVE_ASSET_HUB_GENESIS` point it at other nodes.

use std::{collections::BTreeSet, sync::Arc};

use futures::future::pending;
use subxt::utils::{AccountId32, H256};

use super::{
	ClaimBuildError, ClaimIdentity, PgasClaimBuilder, PgasClaims, PgasClaimsDeps,
	destination_group, oracle::PgasClaimOracle, pgas_domain, with_pgas_claims,
};
use crate::{
	chain::{
		ChainHeads, ExtrinsicService, FixedExtensionVersions, live_asset_hub,
		live_asset_hub_genesis,
	},
	chain_runtime::ChainRuntime,
	durable::{
		CompletionOracle, DurableRegistry, DurableTxEntry, DurableTxId, HeadKind, Window,
		live_engine,
	},
	host_logic::product_account::{
		derive_full_person_ring_vrf_entropy, derive_lite_person_ring_vrf_entropy,
	},
	runtime::statement_allowance::{
		self as alloc, CollectionCandidate, FixedPersonhoodSigner, StatementAllowanceError,
		collection::PersonhoodCollection,
		pgas::{BuiltClaim, ClaimSlot, PgasClaim, build_pgas_claim, holds_a_full_claim},
	},
	store::open_temporary_core_db,
	time::RealTimer,
};

/// The extension pipeline the hosts map Asset Hub to, which carries `AsPgas`.
const ASSET_HUB_PIPELINE: u8 = 1;

/// The person behind a mnemonic, with the chains its claims are built from.
struct LivePerson {
	entropy: Vec<u8>,
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
	async fn connect(chain: &ChainRuntime, genesis: H256, entropy: Vec<u8>) -> Self {
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
				entropy: derive_full_person_ring_vrf_entropy(&self.entropy, suffix),
			},
			CollectionCandidate {
				collection: PersonhoodCollection::LitePeople,
				entropy: derive_lite_person_ring_vrf_entropy(&self.entropy, suffix),
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
		let collections = &PersonhoodCollection::ALL;
		let rings = alloc::find_including_rings(
			&self.people,
			&self.people_metadata,
			&signer,
			collections,
			u32::MAX,
		)
		.await?;
		let ring = rings.first().expect("the person is in a People ring");
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
			ring,
		};
		build_pgas_claim(&claim, excluded).await
	}
}

#[async_trait::async_trait]
impl PgasClaimBuilder for LivePerson {
	async fn build_claim(
		&self,
		target: AccountId32,
		_identity: ClaimIdentity,
		excluded: &BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		self.build(target, excluded)
			.await
			.map_err(|error| ClaimBuildError::Transient(anyhow::anyhow!(error.to_string())))
	}
}

/// The ledger row of claim `id` into `dest`, as a recovery pass hands it to
/// the oracle.
async fn claim_entry(claims: &PgasClaims, id: DurableTxId, dest: &AccountId32) -> DurableTxEntry {
	let group = destination_group(dest);
	let rows = claims.engine.group(&pgas_domain(), &group).await.unwrap();
	let row = rows.into_iter().find(|row| row.id == id).expect("the claim is listed");
	DurableTxEntry {
		id,
		domain: pgas_domain(),
		group: Some(group),
		tx_hash: row.tx_hash.expect("the claim was built"),
		window: Window { birth: 0, period: 0 },
		status: row.status,
		scanned_to: None,
		plan: row.plan,
	}
}

/// A claim the engine builds, submits and follows mints PGAS into a fresh
/// account, and the alias its attempt recorded proves it at the best head.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Paseo Next chains and a person's mnemonic"]
async fn a_durable_claim_lands_on_asset_hub_and_its_alias_proves_it() {
	let Ok(mnemonic) = std::env::var("TRUAPI_LIVE_PERSON_MNEMONIC") else {
		eprintln!("skipped: TRUAPI_LIVE_PERSON_MNEMONIC is not set");
		return;
	};
	let entropy = bip39::Mnemonic::parse(mnemonic.trim()).unwrap().to_entropy();
	let genesis = live_asset_hub_genesis();
	let chain = live_asset_hub(genesis);
	let person = Arc::new(LivePerson::connect(&chain, genesis, entropy).await);
	let (_dir, db) = open_temporary_core_db();
	let deps =
		PgasClaimsDeps { storage: Arc::new(chain.clone()), genesis, builder: person.clone() };
	let engine = live_engine(&chain, db, with_pgas_claims(DurableRegistry::new(), deps));
	let claims = PgasClaims::new(engine, Arc::new(RealTimer));
	let dest = AccountId32(rand::random());
	let identity = ClaimIdentity::from_root_public_key([0x1d; 32]);

	let id = claims.claim(dest, identity).await.unwrap();
	claims.wait_until_delivered(id, pending()).await.unwrap();

	let funded = holds_a_full_claim(&person.asset_hub, &person.asset_hub_metadata, &dest.0);
	assert!(funded.await.unwrap(), "the destination holds a full claim");
	let entry = claim_entry(&claims, id, &dest).await;
	let heads = chain.heads(genesis).await.unwrap();
	let oracle = PgasClaimOracle::new(Arc::new(chain.clone()), genesis);
	let scope = oracle.open_pass(std::slice::from_ref(&entry), &heads).await.unwrap();
	assert!(scope.proven_completed(&entry, HeadKind::Best), "the alias proves the claim");
}
