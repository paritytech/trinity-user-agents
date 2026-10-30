//! The live People chain's statement store, for ignored live tests:
//! `TRUAPI_LIVE_PEOPLE_WS` and `TRUAPI_LIVE_PEOPLE_GENESIS` pick the node,
//! Paseo People Next by default.

use std::sync::Arc;

use subxt::utils::H256;
use subxt_rpcs::RpcClient;

use super::{start_statement_watch_over, submit_sso};
use crate::{
	chain::{ExtrinsicService, FixedExtensionVersions, tokio_spawner},
	host_logic::{
		product_account::{derive_lite_person_ring_vrf_entropy, derive_sr25519_hard_path},
		statement_store::{
			StatementBatches, StatementStoreApi, StatementWatchError, TopicFilter,
			validated_topic_filter,
		},
	},
	host_rpc_client::HostRpcClient,
	runtime::statement_allowance::{self as alloc, collection::PersonhoodCollection},
};

const LITE_PEOPLE: PersonhoodCollection = PersonhoodCollection::LitePeople;

/// How long a live test waits for the chain to answer.
pub const LIVE_WAIT: core::time::Duration = core::time::Duration::from_secs(60);

/// A connection to the live People chain's statement store.
pub struct LivePeopleStatements {
	provider: Arc<truapi_provider::EmbeddedChainProvider>,
	genesis: [u8; 32],
	host_client: HostRpcClient,
}

impl LivePeopleStatements {
	/// Connects to the node the environment names.
	pub async fn connect() -> Self {
		let url = std::env::var("TRUAPI_LIVE_PEOPLE_WS")
			.unwrap_or_else(|_| "wss://paseo-people-next-system-rpc.polkadot.io".into());
		let genesis = std::env::var("TRUAPI_LIVE_PEOPLE_GENESIS").unwrap_or_else(|_| {
			"4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48".into()
		});
		let genesis: [u8; 32] = hex::decode(genesis).unwrap().try_into().unwrap();
		let source = truapi_provider::ChainSource::rpc_node(url.parse().unwrap());
		let provider =
			truapi_provider::EmbeddedChainProvider::builder().chain(genesis, source).build();
		let provider = Arc::new(provider);
		let connection = truapi_provider::platform::ChainProvider::connect(&*provider, genesis)
			.await
			.unwrap();
		let host_client = HostRpcClient::new(Arc::from(connection), tokio_spawner());
		Self { provider, genesis, host_client }
	}

	/// A key that may sign statements this period, derived from the person
	/// behind `entropy`. Its allowance comes from a slot of the person's People
	/// Lite ring, which every run reuses.
	pub async fn statement_signer_with_allowance(&self, entropy: &[u8]) -> schnorrkel::Keypair {
		let signer =
			derive_sr25519_hard_path(entropy, &["truapi-live-test", "statement-store"]).unwrap();
		self.allocate_statement_allowance(entropy, signer.public.to_bytes()).await;
		signer
	}

	/// Submits `statement`, retrying while its signer's allowance is not
	/// visible yet.
	pub async fn publish(&self, statement: Vec<u8>) -> Result<(), String> {
		submit_sso(&self.rpc_client(), statement, "live statement").await
	}

	fn rpc_client(&self) -> RpcClient {
		RpcClient::new(self.host_client.clone())
	}

	/// Points a slot of the People Lite ring of the person behind `entropy` at
	/// `target`, unless one already does.
	async fn allocate_statement_allowance(&self, entropy: &[u8], target: [u8; 32]) {
		let rpc = alloc::rpc::RpcClient::new(self.rpc_client());
		let metadata = alloc::fetch_metadata(&rpc).await.unwrap();
		let person = LitePerson::from_entropy(&rpc, entropy).await;
		let signer = person.ring_signer().await;
		let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
		let slot = SlotRequest { period: alloc::slot::current_period(now.as_secs()), target };
		let memberships =
			alloc::find_including_rings(&rpc, &metadata, &signer, &[LITE_PEOPLE], u32::MAX)
				.await
				.unwrap();
		let scan = slot.scan_params(&person.network_suffix);
		let scans = alloc::scan_collections(&rpc, &metadata, &signer, &[LITE_PEOPLE], scan)
			.await
			.unwrap();
		let versions = Arc::new(FixedExtensionVersions::one_chain(H256(self.genesis), 1));
		let extrinsics =
			ExtrinsicService::from_provider(self.provider.clone(), tokio_spawner(), versions);
		let target_chain =
			alloc::ExtrinsicTarget { extrinsics: &extrinsics, genesis: H256(self.genesis) };
		let registration = slot.registration_params(&person.network_suffix);
		alloc::register_statement_account_pooled(
			&rpc,
			&metadata,
			target_chain,
			&signer,
			&scans,
			&memberships,
			registration,
		)
		.await
		.unwrap();
	}
}

/// The People Lite ring identity of a person, and the network suffix its
/// aliases are scoped to.
struct LitePerson {
	candidates: [alloc::CollectionCandidate; 1],
	network_suffix: Vec<u8>,
}

impl LitePerson {
	/// The person behind `entropy` on the chain `rpc` reaches.
	async fn from_entropy(rpc: &alloc::rpc::RpcClient, entropy: &[u8]) -> Self {
		let network_suffix = alloc::slot::read_network_suffix(rpc).await.unwrap();
		let suffix = String::from_utf8(network_suffix.clone()).unwrap();
		let entropy = derive_lite_person_ring_vrf_entropy(entropy, &suffix);
		let candidates = [alloc::CollectionCandidate { collection: LITE_PEOPLE, entropy }];
		Self { candidates, network_suffix }
	}

	/// Proves the person's ring membership.
	async fn ring_signer(&self) -> alloc::FixedPersonhoodSigner<'_> {
		alloc::FixedPersonhoodSigner::new(&self.candidates).await.unwrap()
	}
}

/// A slot for `target` in `period`, reusing one it already holds and never
/// evicting another account.
struct SlotRequest {
	period: u32,
	target: [u8; 32],
}

impl SlotRequest {
	fn scan_params<'a>(&'a self, network_suffix: &'a [u8]) -> alloc::CollectionScanParams<'a> {
		alloc::CollectionScanParams {
			network_suffix,
			period: self.period,
			target: &self.target,
			reuse_existing: true,
		}
	}

	fn registration_params<'a>(
		&'a self,
		network_suffix: &'a [u8],
	) -> alloc::PooledRegistrationParams<'a> {
		alloc::PooledRegistrationParams {
			target: &self.target,
			period: self.period,
			network_suffix,
			reuse_existing: true,
			allow_eviction: false,
			protected: &[],
		}
	}
}

#[async_trait::async_trait]
impl StatementStoreApi for LivePeopleStatements {
	async fn start_statement_watch(
		&self,
		filter: TopicFilter,
	) -> Result<StatementBatches, StatementWatchError> {
		let (kind, topics) = validated_topic_filter(filter).unwrap();
		start_statement_watch_over(&self.rpc_client(), kind, &topics).await
	}
}
