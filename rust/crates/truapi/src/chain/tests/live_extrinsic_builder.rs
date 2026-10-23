//! Extrinsics the builder makes for each migrated path, dry-run against live
//! paseo-next-v2 nodes. Ignored by default; run with
//! `cargo test -p truapi --lib chain::tests::live_extrinsic_builder -- --ignored --nocapture`.
//!
//! No personhood keys are needed. Each authorizer carries a proof the
//! runtime cannot verify, so the expected answer is a validity verdict, most
//! likely invalid. What these check is that the live runtime decodes the
//! bytes and judges them: an extrinsic it cannot decode fails the dry run
//! itself.

use std::sync::Arc;

use subxt::{tx::ValidationResult, utils::H256};
use truapi_provider::{ChainSource, EmbeddedChainProvider};

use crate::{
	chain::{
		EncodedExtrinsic, ExtrinsicService, ExtrinsicServiceDeps, ExtrinsicVersion,
		RestrictOrigins, TxValidator, V4OnlyHost,
	},
	chain_runtime::{ChainRuntime, HostChainProvider},
	host_internal::{bulletin::build_signed_store_transaction, extrinsic::Sr25519Signer},
	host_logic::dotns_gateway::{Link, RegisterNameCallArgs, encode_register_full_name_extra},
	runtime::statement_allowance::{
		ExtrinsicTarget, ProofAuthorizer,
		collection::PersonhoodCollection,
		extension::{AS_DOTNS_GATEWAY, AS_PGAS, AS_RESOURCES, Metadata},
		extrinsic::{
			ClaimPgasCallArgs, SetStatementStoreAccountCallArgs, build_as_pgas_extra,
			build_as_resources_extra,
		},
		fetch_metadata,
		rpc::RpcClient,
	},
	subscription::Spawner,
};

/// A paseo-next-v2 chain: its node and the genesis the host configures.
struct LiveChain {
	ws: &'static str,
	genesis: &'static str,
}

const PEOPLE: LiveChain = LiveChain {
	ws: "wss://paseo-people-next-system-rpc.polkadot.io",
	genesis: "4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
};
const ASSET_HUB: LiveChain = LiveChain {
	ws: "wss://paseo-asset-hub-next-rpc.polkadot.io",
	genesis: "4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
};
const BULLETIN: LiveChain = LiveChain {
	ws: "wss://paseo-bulletin-next-rpc.polkadot.io",
	genesis: "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
};

/// A ring proof of the right length that no ring verifies.
const UNVERIFIABLE_PROOF: [u8; 785] = [0xEE; 785];

impl LiveChain {
	fn genesis(&self) -> H256 {
		H256::from_slice(&hex::decode(self.genesis).expect("genesis is hex"))
	}

	/// The chain runtime over this node, its tasks on the test's runtime.
	fn chain_runtime(&self) -> ChainRuntime {
		let source = ChainSource::rpc_node(self.ws.parse().expect("a WebSocket URL"));
		let provider = EmbeddedChainProvider::builder().chain(self.genesis().0, source).build();
		let handle = tokio::runtime::Handle::current();
		let spawner: Spawner = Arc::new(move |task| {
			handle.spawn(task);
		});
		ChainRuntime::new(Arc::new(HostChainProvider::new(Arc::new(provider))), spawner)
	}

	async fn allowance_metadata(&self) -> Metadata {
		let rpc = RpcClient::connect(self.ws).await.expect("connect");
		fetch_metadata(&rpc).await.expect("metadata")
	}
}

fn extrinsic_service(chain: &ChainRuntime) -> ExtrinsicService {
	ExtrinsicService::new(ExtrinsicServiceDeps {
		storage: Arc::new(chain.clone()),
		nonces: Arc::new(chain.clone()),
		versions: Arc::new(V4OnlyHost),
	})
}

/// Dry-runs `extrinsic` at the best block and returns the runtime's verdict.
/// Panics when the runtime cannot judge it, which is what a malformed
/// extrinsic does.
async fn dry_run(chain: &ChainRuntime, genesis: H256, extrinsic: Vec<u8>) -> ValidationResult {
	let verdict = chain
		.validate(genesis, &EncodedExtrinsic::new(extrinsic))
		.await
		.expect("the live runtime decodes and judges the extrinsic");
	println!("verdict: {verdict:?}");
	verdict
}

#[tokio::test]
#[ignore = "needs network access to the live People chain"]
async fn a_statement_store_registration_is_judged_by_people() {
	let chain = PEOPLE.chain_runtime();
	let extrinsics = extrinsic_service(&chain);
	let metadata = PEOPLE.allowance_metadata().await;
	let prove = |_message: &[u8; 32]| {
		build_as_resources_extra(
			&metadata,
			&UNVERIFIABLE_PROOF,
			0,
			0,
			PersonhoodCollection::LitePeople,
		)
	};
	let call = SetStatementStoreAccountCallArgs { period: 1, seq: 0, target_account: [0x22; 32] };

	let extrinsic = ExtrinsicTarget { extrinsics: &extrinsics, genesis: PEOPLE.genesis() }
		.build_authorized(
			"Resources",
			"set_statement_store_account",
			call,
			ProofAuthorizer::new(AS_RESOURCES, prove),
		)
		.await
		.expect("build");

	dry_run(&chain, PEOPLE.genesis(), extrinsic).await;
}

#[tokio::test]
#[ignore = "needs network access to the live Asset Hub"]
async fn a_pgas_claim_is_judged_by_asset_hub() {
	let chain = ASSET_HUB.chain_runtime();
	let extrinsics = extrinsic_service(&chain);
	let metadata = ASSET_HUB.allowance_metadata().await;
	let prove = |_message: &[u8; 32]| {
		build_as_pgas_extra(
			&metadata,
			&UNVERIFIABLE_PROOF,
			0,
			0,
			1,
			PersonhoodCollection::LitePeople,
		)
	};
	let call = ClaimPgasCallArgs { slot_index: 0, target: [0x22; 32] };

	let extrinsic = ExtrinsicTarget { extrinsics: &extrinsics, genesis: ASSET_HUB.genesis() }
		.build_authorized("Pgas", "claim_pgas", call, ProofAuthorizer::new(AS_PGAS, prove))
		.await
		.expect("build");

	dry_run(&chain, ASSET_HUB.genesis(), extrinsic).await;
}

#[tokio::test]
#[ignore = "needs network access to the live Asset Hub"]
async fn a_dotns_registration_is_judged_by_asset_hub() {
	let chain = ASSET_HUB.chain_runtime();
	let extrinsics = extrinsic_service(&chain);
	let metadata = ASSET_HUB.allowance_metadata().await;
	let info_variant = metadata.dotns_register_full_name_variant().expect("RegisterFullName");
	let authorize = |_digest: &[u8; 32]| {
		Ok(encode_register_full_name_extra(info_variant, &UNVERIFIABLE_PROOF, 0, 0, &[0; 64]))
	};
	let call = RegisterNameCallArgs {
		who: [0x22; 32],
		label: b"livebuildercheck".to_vec(),
		link: Link::None([0x04; 65]),
	};

	let built = extrinsics
		.build_mortal_extrinsic(ASSET_HUB.genesis(), |builder| {
			builder.with_call_args("DotnsGateway", "register_name", call);
			builder
				.with_extension(ProofAuthorizer::new(AS_DOTNS_GATEWAY, authorize))
				.with_extension(RestrictOrigins(true))
				.with_extrinsic_version(ExtrinsicVersion::newest_declaring(AS_DOTNS_GATEWAY));
		})
		.await
		.expect("build");

	dry_run(&chain, ASSET_HUB.genesis(), built.extrinsic.bytes().to_vec()).await;
}

#[tokio::test]
#[ignore = "needs network access to the live Bulletin chain"]
async fn a_signed_store_is_judged_by_bulletin() {
	let chain = BULLETIN.chain_runtime();
	let extrinsics = extrinsic_service(&chain);
	let signer = Sr25519Signer::from_secret_bytes(&[0x07; 64]).expect("a signer");

	let built = build_signed_store_transaction(&extrinsics, BULLETIN.genesis(), &signer, b"live")
		.await
		.expect("build");

	dry_run(&chain, BULLETIN.genesis(), built.extrinsic.bytes().to_vec()).await;
}
