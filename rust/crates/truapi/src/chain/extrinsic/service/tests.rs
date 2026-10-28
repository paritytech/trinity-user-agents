//! Service tests against the scripted node, which serves the Bulletin fixture.

use std::sync::{
	Arc,
	atomic::{AtomicUsize, Ordering},
};

use futures::executor::block_on;
use subxt::{
	dynamic::{Value, tx},
	utils::H256,
};
use truapi::latest::GenericError;

use super::ExtrinsicService;
use crate::{
	chain::{ExtrinsicBuildError, ExtrinsicVersion, MortalExtrinsic, Node, method_count, serve},
	host_internal::extrinsic::{Sr25519Signer, tests::split_v4},
	platform::{ChainExtensionVersion, ExtensionVersionMap, ExtrinsicVersionHost},
	test_support::ScriptedProvider,
};

const GENESIS: H256 = H256([0xab; 32]);
const FINALIZED: u64 = 5;

/// Answers the host map it was given, or fails, and counts the reads.
struct CountingHost {
	answer: Result<ExtensionVersionMap, GenericError>,
	reads: AtomicUsize,
}

impl CountingHost {
	fn answering(answer: Result<ExtensionVersionMap, GenericError>) -> Arc<Self> {
		Arc::new(Self { answer, reads: AtomicUsize::new(0) })
	}

	fn reads(&self) -> usize {
		self.reads.load(Ordering::SeqCst)
	}
}

#[async_trait::async_trait]
impl ExtrinsicVersionHost for CountingHost {
	async fn transaction_extension_versions(&self) -> Result<ExtensionVersionMap, GenericError> {
		self.reads.fetch_add(1, Ordering::SeqCst);
		self.answer.clone()
	}
}

fn service_over(node: Node, host: Arc<CountingHost>) -> (ExtrinsicService, Arc<ScriptedProvider>) {
	let (_node, provider, chain) = serve(node);
	let service = ExtrinsicService::over_chain(chain.clone(), host);
	(service, provider)
}

fn no_v5_chains() -> Arc<CountingHost> {
	CountingHost::answering(Ok(ExtensionVersionMap::default()))
}

fn test_signer() -> Sr25519Signer {
	let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
		.unwrap()
		.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
	Sr25519Signer::from_keypair(&keypair)
}

fn build_remark(
	service: &ExtrinsicService,
	signer: &Sr25519Signer,
	nonce: Option<u64>,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let mut builder = service
		.build_mortal_extrinsic(GENESIS)
		.with_call(tx("System", "remark", vec![Value::from_bytes([1, 2, 3])]))
		.with_signer(signer);
	if let Some(nonce) = nonce {
		builder = builder.with_nonce(nonce);
	}
	block_on(builder.build())
}

/// The nonce `CheckNonce` carries in a Bulletin V4 extrinsic: right after
/// the two era bytes, compact encoded.
fn bulletin_nonce(extrinsic: &MortalExtrinsic) -> u8 {
	let (_, _, values_and_call) = split_v4(extrinsic.extrinsic.bytes());
	values_and_call[2] >> 2
}

/// The engine judges expiry from the birth it is handed, so the birth must
/// be the finalized head the era was signed against.
#[test]
fn an_extrinsic_is_born_at_the_finalized_head_for_64_blocks() {
	let (service, _) = service_over(Node::new(FINALIZED, 8), no_v5_chains());

	let built = build_remark(&service, &test_signer(), Some(0)).unwrap();

	assert_eq!(built.mortality.birth().number, FINALIZED);
	assert_eq!(built.mortality.period(), 64);
}

/// A signed build takes the pool's next index, so it does not reuse the
/// nonce of a transaction still waiting in the pool.
#[test]
fn a_signed_build_takes_the_pool_nonce() {
	let mut node = Node::new(FINALIZED, 8);
	node.account_next_index = 9;
	let (service, _) = service_over(node, no_v5_chains());

	let built = build_remark(&service, &test_signer(), None).unwrap();

	assert_eq!(bulletin_nonce(&built), 9);
}

#[test]
fn a_caller_nonce_skips_the_pool_read() {
	let mut node = Node::new(FINALIZED, 8);
	node.account_next_index = 9;
	let (service, provider) = service_over(node, no_v5_chains());

	let built = build_remark(&service, &test_signer(), Some(3)).unwrap();

	assert_eq!(bulletin_nonce(&built), 3);
	assert_eq!(method_count(&provider, "system_accountNextIndex"), 0);
}

/// The host is asked once; its answer holds for the life of the service.
#[test]
fn the_host_map_is_read_once() {
	let host = no_v5_chains();
	let (service, _) = service_over(Node::new(FINALIZED, 8), host.clone());
	let signer = test_signer();

	build_remark(&service, &signer, Some(0)).unwrap();
	build_remark(&service, &signer, Some(0)).unwrap();

	assert_eq!(host.reads(), 1);
}

/// A failed read is not an empty map: the build fails, and the next build
/// asks the host again.
#[test]
fn a_failed_host_map_read_fails_the_build_and_is_not_kept() {
	let failure = GenericError { reason: "remote config unavailable".to_string() };
	let host = CountingHost::answering(Err(failure));
	let (service, _) = service_over(Node::new(FINALIZED, 8), host.clone());
	let signer = test_signer();

	let first = build_remark(&service, &signer, Some(0));
	let second = build_remark(&service, &signer, Some(0));

	assert!(
		matches!(first, Err(ExtrinsicBuildError::HostVersions(reason)) if reason == "remote config unavailable")
	);
	assert!(second.is_err());
	assert_eq!(host.reads(), 2);
}

/// A signed build on a chain the host lists as V5 falls back to V4 when
/// that pipeline cannot carry a signature, as the native hosts do. Bulletin's
/// pipeline has no `VerifyMultiSignature`.
#[test]
fn a_signed_build_falls_back_to_v4_where_the_pipeline_cannot_sign() {
	let map = ExtensionVersionMap {
		chains: vec![ChainExtensionVersion { genesis_hash: GENESIS.0, extension_version: 0 }],
	};
	let (service, _) = service_over(Node::new(FINALIZED, 8), CountingHost::answering(Ok(map)));

	let built = build_remark(&service, &test_signer(), Some(0)).unwrap();

	assert_eq!(bulletin_nonce(&built), 0);
}

/// A build that pins its version never needs the host's map, so a host that
/// cannot answer does not stop it.
#[test]
fn a_pinned_build_does_not_read_the_host_map() {
	let failure = GenericError { reason: "remote config unavailable".to_string() };
	let host = CountingHost::answering(Err(failure));
	let (service, _) = service_over(Node::new(FINALIZED, 8), host.clone());
	let signer = test_signer();
	let v4 = ExtrinsicVersion::V4;

	let builder = service
		.build_mortal_extrinsic(GENESIS)
		.with_call(tx("System", "remark", vec![Value::from_bytes([1, 2, 3])]))
		.with_signer(&signer)
		.with_nonce(0)
		.with_extrinsic_version(v4);
	let built = block_on(builder.build());

	assert!(built.is_ok());
	assert_eq!(host.reads(), 0);
}

/// Each build reads the finalized head afresh, so a retry never reuses a
/// birth block that has aged.
#[test]
fn every_build_reads_a_fresh_birth() {
	let (service, provider) = service_over(Node::new(FINALIZED, 8), no_v5_chains());
	let signer = test_signer();

	build_remark(&service, &signer, Some(0)).unwrap();
	build_remark(&service, &signer, Some(0)).unwrap();

	assert_eq!(method_count(&provider, "chain_getFinalizedHead"), 2);
}
