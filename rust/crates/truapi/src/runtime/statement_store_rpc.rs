//! Runtime helper for People-chain statement-store JSON-RPC.
//!
//! Statement subscriptions open their own host RPC connection for their
//! lifetime instead of riding the shared
//! [`crate::chain_runtime::ChainRuntime`] chainHead runtime. If a host shares
//! same-topic statement subscriptions on one upstream connection, the host
//! broker must fan out and ref-count same-upstream-token notifications.
//!
//! Submissions and the allowance paths carry no statement subscription, so
//! they share the [`ChainRuntime`]'s People-chain connection.

use std::sync::Arc;

use core::{future::Future, time::Duration};
use futures::StreamExt as _;
use serde_json::{Value, json};

use crate::platform::{JsonRpcConnection, Platform, ProviderError};
use subxt_rpcs::{
	RpcClient,
	client::{RpcSubscription, rpc_params},
};
use thiserror::Error;
use tracing::warn;
use truapi::latest::RemoteStatementStoreSubscribeRequest;

use crate::{
	chain_runtime::{ChainRuntime, RuntimeFailure},
	host_logic::statement_store::{
		NewStatements, SUBMIT_STATEMENT_METHOD, SUBSCRIBE_STATEMENT_METHOD, StatementBatches,
		StatementStoreApi, StatementWatchError, TopicFilterKind, UNSUBSCRIBE_STATEMENT_METHOD,
		backlog_then_live_batches, hex_topic, parse_new_statements_result, validated_topic_filter,
	},
	host_rpc_client::HostRpcClient,
	stream_extensions::StreamExtensions as _,
	subscription::Spawner,
};

/// One attempt plus ten retries, 2 s apart, matching the native iOS and Android hosts.
const SSO_NO_ALLOWANCE_RETRY_ATTEMPTS: usize = 11;
#[cfg(not(test))]
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_secs(2);
#[cfg(test)]
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_millis(1);

/// Error opening a statement-store RPC client over the host platform.
#[derive(Debug, Error)]
pub enum StatementStoreRpcClientError {
	/// The host failed to open a People-chain JSON-RPC connection.
	#[error("{label} connect failed: {error:?}")]
	Connect {
		/// Operation label requesting the connection.
		label: &'static str,
		/// Chain provider error.
		error: ProviderError,
	},
	/// The pooled People-chain connection could not be opened.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
}

/// People-chain statement-store RPC client factory.
#[derive(Clone)]
pub struct StatementStoreRpc {
	platform: Arc<dyn Platform>,
	people_chain_genesis_hash: [u8; 32],
	spawner: Spawner,
	chain: ChainRuntime,
}

impl StatementStoreRpc {
	/// Build a helper that opens subscription connections through
	/// `platform` and sends requests over `chain`'s pooled connection.
	pub fn new(
		platform: Arc<dyn Platform>,
		chain: ChainRuntime,
		people_chain_genesis_hash: [u8; 32],
		spawner: Spawner,
	) -> Self {
		Self { platform, people_chain_genesis_hash, spawner, chain }
	}

	/// Open a People-chain RPC client already scoped to its genesis hash, for
	/// the native allowance paths that key the chain-context cache by it.
	pub async fn chain_client(
		&self,
		label: &'static str,
	) -> Result<crate::runtime::statement_allowance::ChainClient, StatementStoreRpcClientError> {
		Ok(crate::runtime::statement_allowance::ChainClient::new(
			crate::runtime::statement_allowance::rpc::RpcClient::new(RpcClient::new(
				self.request_client(label).await?,
			)),
			self.people_chain_genesis_hash,
		))
	}

	/// Open a statement-store RPC client over its own host-provided
	/// People-chain connection, for statement subscriptions.
	pub async fn client(
		&self,
		label: &'static str,
	) -> Result<RpcClient, StatementStoreRpcClientError> {
		let connection = self.connect(label).await?;
		Ok(RpcClient::new(HostRpcClient::new(connection, self.spawner.clone())))
	}

	/// Submit a SCALE-encoded statement and wait for the JSON-RPC ack.
	pub async fn submit(&self, statement: Vec<u8>, label: &'static str) -> Result<(), String> {
		let rpc_client = self.request_client(label).await.map_err(|err| err.to_string())?;
		submit(&RpcClient::new(rpc_client), statement).await
	}

	/// Submit an SSO statement, tolerating the short propagation window after
	/// an allowance registration is included but not yet visible to the
	/// Statement Store RPC backend.
	pub async fn submit_sso(&self, statement: Vec<u8>, label: &'static str) -> Result<(), String> {
		retry_until_allowance_visible(label, || self.submit(statement.clone(), label)).await
	}

	/// Submit a SCALE-encoded statement without waiting for the JSON-RPC ack.
	pub async fn submit_fire_and_forget(
		&self,
		statement: Vec<u8>,
		label: &'static str,
	) -> Result<(), String> {
		let rpc_client = self.request_client(label).await.map_err(|err| err.to_string())?;
		rpc_client
			.send_fire_and_forget(
				SUBMIT_STATEMENT_METHOD,
				rpc_params![format!("0x{}", hex::encode(&statement))].build(),
			)
			.map_err(rpc_error_message)
	}

	async fn request_client(
		&self,
		label: &'static str,
	) -> Result<HostRpcClient, StatementStoreRpcClientError> {
		Ok(self.chain.rpc_client(label, &self.people_chain_genesis_hash).await?)
	}

	async fn connect(
		&self,
		label: &'static str,
	) -> Result<Arc<dyn JsonRpcConnection>, StatementStoreRpcClientError> {
		self.platform
			.connect(self.people_chain_genesis_hash)
			.await
			.map(Arc::from)
			.map_err(|error| StatementStoreRpcClientError::Connect { label, error })
	}
}

#[async_trait::async_trait]
impl StatementStoreApi for StatementStoreRpc {
	async fn start_statement_watch(
		&self,
		filter: RemoteStatementStoreSubscribeRequest,
	) -> Result<StatementBatches, StatementWatchError> {
		let (kind, topics) =
			validated_topic_filter(filter).map_err(StatementWatchError::NotStarted)?;
		let rpc_client = self
			.client("statement-watch")
			.await
			.map_err(|error| StatementWatchError::NotStarted(error.to_string()))?;
		start_statement_watch_over(&rpc_client, kind, &topics).await
	}
}

/// Starts watching the statements `kind` and `topics` match over
/// `rpc_client`, as [`StatementStoreApi::start_statement_watch`] describes.
async fn start_statement_watch_over(
	rpc_client: &RpcClient,
	kind: TopicFilterKind,
	topics: &[[u8; 32]],
) -> Result<StatementBatches, StatementWatchError> {
	let subscription = subscribe(rpc_client, kind, topics)
		.await
		.map_err(|error| StatementWatchError::NotStarted(rpc_error_message(error)))?;
	let subscription_id = subscription
		.subscription_id()
		.map(ToString::to_string)
		.ok_or_else(|| StatementWatchError::NotStarted("the subscription has no id".to_owned()))?;
	let pages =
		subscription.map(move |notification| parse_statement_page(&subscription_id, notification));
	Ok(backlog_then_live_batches(pages).take_until_inclusive(Result::is_err))
}

/// The page a statement subscription notification carries.
fn parse_statement_page(
	subscription_id: &str,
	notification: Result<Value, subxt_rpcs::Error>,
) -> Result<NewStatements, StatementWatchError> {
	let value = notification
		.map_err(|error| StatementWatchError::Subscription(rpc_error_message(error)))?;
	parse_new_statements_result(subscription_id.to_string(), &value)
		.map_err(StatementWatchError::MalformedPage)
}

/// Subscribe to statements matching the requested topic filter.
pub async fn subscribe(
	rpc_client: &RpcClient,
	kind: TopicFilterKind,
	topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
	rpc_client
		.subscribe::<Value>(
			SUBSCRIBE_STATEMENT_METHOD,
			rpc_params![filter(kind, topics)],
			UNSUBSCRIBE_STATEMENT_METHOD,
		)
		.await
}

/// Subscribe to statements matching every topic.
pub async fn subscribe_match_all(
	rpc_client: &RpcClient,
	topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
	subscribe(rpc_client, TopicFilterKind::MatchAll, topics).await
}

/// Submit a SCALE-encoded statement and confirm the store accepted it.
///
/// `statement_submit` returns an RPC error only for internal failures; a
/// rejected or invalid statement (e.g. `NoAllowance`, `BadProof`) comes back as
/// `Ok(SubmitResult)`. Treat only `new`/`known` as success, so allowance/proof
/// rejections surface instead of being silently dropped.
pub async fn submit(rpc_client: &RpcClient, statement: Vec<u8>) -> Result<(), String> {
	let result = rpc_client
		.request::<Value>(
			SUBMIT_STATEMENT_METHOD,
			rpc_params![format!("0x{}", hex::encode(&statement))],
		)
		.await
		.map_err(rpc_error_message)?;
	match result.get("status").and_then(Value::as_str) {
		Some("new") | Some("known") => Ok(()),
		_ => Err(format!("statement_submit not accepted: {result}")),
	}
}

pub async fn submit_sso(
	rpc_client: &RpcClient,
	statement: Vec<u8>,
	label: &'static str,
) -> Result<(), String> {
	retry_until_allowance_visible(label, || submit(rpc_client, statement.clone())).await
}

/// Run `submit` until it succeeds, fails with anything but `noAllowance`, or
/// runs out of attempts.
async fn retry_until_allowance_visible<SubmitFn, SubmitFuture>(
	label: &'static str,
	submit: SubmitFn,
) -> Result<(), String>
where
	SubmitFn: Fn() -> SubmitFuture,
	SubmitFuture: Future<Output = Result<(), String>>,
{
	for attempt in 1..=SSO_NO_ALLOWANCE_RETRY_ATTEMPTS {
		match submit().await {
			Ok(()) => return Ok(()),
			Err(reason)
				if is_no_allowance_rejection(&reason) &&
					attempt < SSO_NO_ALLOWANCE_RETRY_ATTEMPTS =>
			{
				warn!(
					label,
					attempt,
					max_attempts = SSO_NO_ALLOWANCE_RETRY_ATTEMPTS,
					"SSO allowance not visible yet; retrying statement submission"
				);
				futures_timer::Delay::new(SSO_NO_ALLOWANCE_RETRY_DELAY).await;
			},
			Err(reason) => return Err(reason),
		}
	}
	unreachable!("the bounded SSO submit loop always returns")
}

/// Whether a [`submit`] failure is a `noAllowance` rejection.
pub fn is_no_allowance_rejection(reason: &str) -> bool {
	reason.contains("noAllowance")
}

/// Statement-store topic filter encoded as JSON-RPC params.
pub fn filter(kind: TopicFilterKind, topics: &[[u8; 32]]) -> Value {
	let topics = topics.iter().map(hex_topic).collect::<Vec<_>>();
	match kind {
		TopicFilterKind::MatchAll => json!({ "matchAll": topics }),
		TopicFilterKind::MatchAny => json!({ "matchAny": topics }),
	}
}

/// Human-readable JSON-RPC error message, preserving user error text when
/// provided by the remote endpoint.
pub fn rpc_error_message(error: subxt_rpcs::Error) -> String {
	match error {
		subxt_rpcs::Error::User(error) => error.message,
		other => other.to_string(),
	}
}

#[cfg(test)]
mod tests {
	use std::sync::{
		Arc, Mutex,
		atomic::{AtomicUsize, Ordering},
	};

	use async_trait::async_trait;
	use futures::{
		channel::mpsc,
		stream::{BoxStream, StreamExt},
	};
	use serde_json::json;

	use futures::executor::{block_on, block_on_stream};
	use parity_scale_codec::Encode;
	use subxt::utils::H256;
	use subxt_rpcs::RpcClient;
	use truapi::{latest::RemoteStatementStoreSubscribeRequest, v01};

	use super::{
		StatementStoreRpc, TopicFilterKind, is_no_allowance_rejection, start_statement_watch_over,
		submit_sso,
	};
	use crate::{
		chain::{ExtrinsicService, FixedExtensionVersions, tokio_spawner},
		chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure},
		host_logic::{
			product_account::{derive_lite_person_ring_vrf_entropy, derive_sr25519_hard_path},
			statement_store::{
				MAX_MATCH_ALL_TOPICS, StatementBatches, StatementField, StatementStoreApi,
				StatementWatchError, hex_topic, sign_statement_fields, validated_topic_filter,
			},
			x25519_seal::{SealedBox, X25519PrivateKey},
		},
		host_rpc_client::HostRpcClient,
		nft_purse::{TEST_STATIC_SECRET, test_purses_over},
		platform::{JsonRpcConnection, ProductId},
		runtime::statement_allowance::{self as alloc, collection::PersonhoodCollection},
		store::open_temporary_core_db,
		test_support::{StubPlatform, statement_page_frame, subscribe_ack_frame, test_spawner},
	};

	const LITE_PEOPLE: PersonhoodCollection = PersonhoodCollection::LitePeople;

	const PEOPLE_GENESIS: [u8; 32] = [0x11; 32];

	/// What one People-chain connection does with the requests it receives.
	#[derive(Clone, Copy)]
	enum Upstream {
		/// Closes as soon as a request arrives.
		Drops,
		/// Answers every submission with this status.
		Answers(&'static str),
	}

	/// Chain whose n-th connection behaves as the n-th scripted [`Upstream`],
	/// the last one repeating.
	struct PeopleChain {
		upstreams: Vec<Upstream>,
		connects: AtomicUsize,
		requests: Arc<AtomicUsize>,
	}

	impl PeopleChain {
		fn new(upstreams: Vec<Upstream>) -> Arc<Self> {
			Arc::new(Self { upstreams, connects: AtomicUsize::new(0), requests: Arc::default() })
		}

		fn connects(&self) -> usize {
			self.connects.load(Ordering::SeqCst)
		}

		fn requests(&self) -> usize {
			self.requests.load(Ordering::SeqCst)
		}
	}

	#[async_trait]
	impl RuntimeChainProvider for PeopleChain {
		async fn connect(
			&self,
			_genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			let earlier_connects = self.connects.fetch_add(1, Ordering::SeqCst);
			let upstream = self.upstreams[earlier_connects.min(self.upstreams.len() - 1)];
			let (sender, receiver) = mpsc::unbounded();
			Ok(Arc::new(PeopleChainConnection {
				upstream,
				requests: self.requests.clone(),
				sender: Mutex::new(Some(sender)),
				receiver: Mutex::new(Some(receiver)),
			}))
		}
	}

	struct PeopleChainConnection {
		upstream: Upstream,
		requests: Arc<AtomicUsize>,
		sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
		receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
	}

	impl JsonRpcConnection for PeopleChainConnection {
		fn send(&self, request: String) {
			self.requests.fetch_add(1, Ordering::SeqCst);
			let mut sender = self.sender.lock().unwrap();
			let status = match self.upstream {
				Upstream::Drops => {
					sender.take();
					return;
				},
				Upstream::Answers(status) => status,
			};
			let request: serde_json::Value = serde_json::from_str(&request).unwrap();
			let response = json!({
				"jsonrpc": "2.0",
				"id": request["id"],
				"result": { "status": status, "reason": "noAllowance" },
			});
			if let Some(sender) = sender.as_ref() {
				let _ = sender.unbounded_send(response.to_string());
			}
		}

		fn responses(&self) -> BoxStream<'static, String> {
			self.receiver.lock().unwrap().take().unwrap().boxed()
		}

		fn close(&self) {
			self.sender.lock().unwrap().take();
		}
	}

	fn statement_store(chain: &Arc<PeopleChain>) -> StatementStoreRpc {
		StatementStoreRpc::new(
			Arc::new(StubPlatform::default()),
			ChainRuntime::new(chain.clone(), test_spawner()),
			PEOPLE_GENESIS,
			test_spawner(),
		)
	}

	/// A product sends one statement per chat message, so a connection per
	/// submit puts a WebSocket handshake in front of every message.
	#[test]
	fn consecutive_submits_share_one_connection() {
		let chain = PeopleChain::new(vec![Upstream::Answers("new")]);
		let statement_store = statement_store(&chain);

		futures::executor::block_on(async {
			statement_store.submit(vec![1], "first").await.unwrap();
			statement_store.submit(vec![2], "second").await.unwrap();
		});

		assert_eq!(chain.connects(), 1);
	}

	#[test]
	fn a_later_submit_reconnects_after_a_dropped_connection() {
		let chain = PeopleChain::new(vec![Upstream::Drops, Upstream::Answers("new")]);
		let statement_store = statement_store(&chain);

		let (first, second) = futures::executor::block_on(async {
			let first = statement_store.submit(vec![1], "first").await;
			let second = statement_store.submit(vec![2], "second").await;
			(first, second)
		});

		assert_eq!(
			(first.is_err(), second, chain.connects(), chain.requests()),
			(true, Ok(()), 2, 2)
		);
	}

	/// A rejection is the store's answer, not a transport failure. Resending
	/// the statement would only repeat the rejection.
	#[test]
	fn a_rejected_submit_is_not_resent() {
		let chain = PeopleChain::new(vec![Upstream::Answers("rejected")]);
		let statement_store = statement_store(&chain);

		let result = futures::executor::block_on(statement_store.submit(vec![1], "submit"));

		assert_eq!((result.is_err(), chain.requests()), (true, 1));
	}

	#[test]
	fn an_sso_submit_reports_a_dropped_connection() {
		let chain = PeopleChain::new(vec![Upstream::Drops, Upstream::Answers("new")]);
		let statement_store = statement_store(&chain);

		let result = futures::executor::block_on(statement_store.submit_sso(vec![1], "sso"));

		assert_eq!((result.is_err(), chain.connects(), chain.requests()), (true, 1, 1));
	}

	#[test]
	fn identifies_no_allowance_submit_rejections() {
		assert!(is_no_allowance_rejection(
			r#"statement_submit not accepted: {"reason":"noAllowance","status":"rejected"}"#
		));
		assert!(!is_no_allowance_rejection(
			r#"statement_submit not accepted: {"reason":"badProof","status":"rejected"}"#
		));
	}

	/// A statement store whose subscription connection answers with `frames`.
	fn watching_statement_store(frames: Vec<String>) -> (StatementStoreRpc, Arc<StubPlatform>) {
		let platform = Arc::new(StubPlatform { rpc_responses: frames, ..Default::default() });
		let chain = PeopleChain::new(vec![Upstream::Drops]);
		let statement_store = StatementStoreRpc::new(
			platform.clone(),
			ChainRuntime::new(chain, test_spawner()),
			PEOPLE_GENESIS,
			test_spawner(),
		);
		(statement_store, platform)
	}

	/// The watch subscribes with the filter it is given, and hands over the
	/// stored statements as one batch before each new page.
	#[test]
	fn a_statement_watch_subscribes_with_its_filter_and_batches_the_backlog() {
		let (statement_store, platform) = watching_statement_store(vec![
			subscribe_ack_frame("truapi:1", "watch"),
			statement_page_frame("watch", vec![vec![1]], Some(1)),
			statement_page_frame("watch", vec![vec![2]], Some(0)),
			statement_page_frame("watch", vec![vec![3]], None),
		]);
		let filter = RemoteStatementStoreSubscribeRequest::MatchAll(vec![[7; 32]]);

		let batches = block_on(statement_store.start_statement_watch(filter)).unwrap();
		let batches = block_on_stream(batches).take(2).map(Result::unwrap).collect::<Vec<_>>();

		assert_eq!(batches, vec![vec![vec![1], vec![2]], vec![vec![3]]]);
		let sent = platform.sent_rpc.lock().unwrap();
		let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
		assert_eq!(request["params"][0], json!({ "matchAll": [hex_topic(&[7; 32])] }));
	}

	/// A page the node garbled is a failure the reader sees, not a page that
	/// silently never arrived.
	#[test]
	fn a_malformed_page_ends_the_statement_watch_with_an_error() {
		let malformed = json!({
			"jsonrpc": "2.0",
			"method": "statement_subscribeStatement",
			"params": { "subscription": "watch", "result": { "event": "somethingElse" } },
		});
		let (statement_store, platform) = watching_statement_store(vec![
			subscribe_ack_frame("truapi:1", "watch"),
			malformed.to_string(),
			statement_page_frame("watch", vec![vec![1]], Some(0)),
		]);
		let filter = RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]);

		let batches = block_on(statement_store.start_statement_watch(filter)).unwrap();
		let mut batches = block_on_stream(batches);

		assert!(matches!(batches.next(), Some(Err(StatementWatchError::MalformedPage(_)))));
		assert!(batches.next().is_none());
		let sent = platform.sent_rpc.lock().unwrap();
		let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
		assert_eq!(request["params"][0], json!({ "matchAny": [hex_topic(&[7; 32])] }));
	}

	/// A filter the RPC would refuse fails before any connection opens.
	#[test]
	fn a_statement_watch_with_too_many_topics_does_not_start() {
		let (statement_store, platform) = watching_statement_store(vec![]);
		let topics = vec![[7; 32]; MAX_MATCH_ALL_TOPICS + 1];
		let filter = RemoteStatementStoreSubscribeRequest::MatchAll(topics);

		let started = block_on(statement_store.start_statement_watch(filter));

		assert!(matches!(started, Err(StatementWatchError::NotStarted(_))));
		assert_eq!(platform.chain_connects.lock().unwrap().len(), 0);
	}

	/// A connection to the live People chain, from `TRUAPI_LIVE_PEOPLE_WS` and
	/// `TRUAPI_LIVE_PEOPLE_GENESIS`, or Paseo People Next.
	struct LivePeopleChain {
		provider: Arc<truapi_provider::EmbeddedChainProvider>,
		genesis: [u8; 32],
		host_client: HostRpcClient,
	}

	async fn connect_live_people_chain() -> LivePeopleChain {
		let url = std::env::var("TRUAPI_LIVE_PEOPLE_WS")
			.unwrap_or_else(|_| "wss://paseo-people-next-system-rpc.polkadot.io".into());
		let genesis = std::env::var("TRUAPI_LIVE_PEOPLE_GENESIS").unwrap_or_else(|_| {
			"4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48".into()
		});
		let genesis: [u8; 32] = hex::decode(genesis).unwrap().try_into().unwrap();
		let source = truapi_provider::ChainSource::rpc_node(url.parse().unwrap());
		let provider = Arc::new(
			truapi_provider::EmbeddedChainProvider::builder().chain(genesis, source).build(),
		);
		let connection = truapi_provider::platform::ChainProvider::connect(&*provider, genesis)
			.await
			.unwrap();
		let host_client = HostRpcClient::new(Arc::from(connection), tokio_spawner());
		LivePeopleChain { provider, genesis, host_client }
	}

	/// The live People chain answers a watch of a topic nobody uses with one
	/// empty batch, which is what tells an inbox it caught up. Run with
	/// `cargo test -p truapi --lib live_statement_watch -- --ignored`;
	/// `TRUAPI_LIVE_PEOPLE_WS` and `TRUAPI_LIVE_PEOPLE_GENESIS` pick another
	/// node.
	#[tokio::test(flavor = "multi_thread")]
	#[ignore = "needs the live Paseo People Next chain"]
	async fn live_statement_watch_of_an_unused_topic_starts_with_one_empty_batch() {
		let chain = connect_live_people_chain().await;
		let rpc_client = RpcClient::new(chain.host_client);
		let topic: [u8; 32] = rand::random();

		let mut batches =
			start_statement_watch_over(&rpc_client, TopicFilterKind::MatchAll, &[topic])
				.await
				.unwrap();
		let first = tokio::time::timeout(LIVE_WAIT, batches.next()).await;

		assert_eq!(first.unwrap().map(Result::unwrap), Some(vec![]));
	}

	/// How long a live test waits for the chain to answer.
	const LIVE_WAIT: core::time::Duration = core::time::Duration::from_secs(60);

	/// The live statement store, watched as the host watches it.
	struct LiveStatements(RpcClient);

	#[async_trait]
	impl StatementStoreApi for LiveStatements {
		async fn start_statement_watch(
			&self,
			filter: RemoteStatementStoreSubscribeRequest,
		) -> Result<StatementBatches, StatementWatchError> {
			let (kind, topics) = validated_topic_filter(filter).unwrap();
			start_statement_watch_over(&self.0, kind, &topics).await
		}
	}

	/// The sr25519 key the live tests sign statements with, derived from the
	/// person behind `entropy` so every run reuses one statement-store slot.
	fn live_statement_signer(entropy: &[u8]) -> schnorrkel::Keypair {
		derive_sr25519_hard_path(entropy, &["truapi-live-test", "statement-store"]).unwrap()
	}

	/// Gives `signer` a statement allowance for this period through the People
	/// Lite ring of the person behind `entropy`, unless it already has one.
	async fn allocate_live_statement_allowance(
		chain: &LivePeopleChain,
		entropy: &[u8],
		signer: &schnorrkel::Keypair,
	) {
		let rpc = alloc::rpc::RpcClient::new(RpcClient::new(chain.host_client.clone()));
		let metadata = alloc::fetch_metadata(&rpc).await.unwrap();
		let network_suffix = alloc::slot::read_network_suffix(&rpc).await.unwrap();
		let suffix = String::from_utf8(network_suffix.clone()).unwrap();
		let entropy = derive_lite_person_ring_vrf_entropy(entropy, &suffix);
		let candidates = [alloc::CollectionCandidate { collection: LITE_PEOPLE, entropy }];
		let person = alloc::FixedPersonhoodSigner::new(&candidates).await.unwrap();
		let target = signer.public.to_bytes();
		let period = alloc::slot::current_period(current_unix_secs());
		let memberships =
			alloc::find_including_rings(&rpc, &metadata, &person, &[LITE_PEOPLE], u32::MAX)
				.await
				.unwrap();
		let scan = alloc::CollectionScanParams {
			network_suffix: &network_suffix,
			period,
			target: &target,
			reuse_existing: true,
		};
		let scans = alloc::scan_collections(&rpc, &metadata, &person, &[LITE_PEOPLE], scan)
			.await
			.unwrap();
		let versions = Arc::new(FixedExtensionVersions::one_chain(H256(chain.genesis), 1));
		let extrinsics =
			ExtrinsicService::from_provider(chain.provider.clone(), tokio_spawner(), versions);
		let target_chain =
			alloc::ExtrinsicTarget { extrinsics: &extrinsics, genesis: H256(chain.genesis) };
		let registration = alloc::PooledRegistrationParams {
			target: &target,
			period,
			network_suffix: &network_suffix,
			reuse_existing: true,
			allow_eviction: false,
			protected: &[],
		};
		alloc::register_statement_account_pooled(
			&rpc,
			&metadata,
			target_chain,
			&person,
			&scans,
			&memberships,
			registration,
		)
		.await
		.unwrap();
	}

	/// The statement a sender publishes to put `cheque` under `topic`: the
	/// envelope sealed to the static receive key, signed by `signer`.
	fn signed_inbox_statement(
		signer: &schnorrkel::Keypair,
		topic: v01::StatementTopic,
		cheque: &v01::NftPurseCheque,
	) -> Vec<u8> {
		let plaintext = (0u8, topic, cheque).encode();
		let static_key = X25519PrivateKey::from(TEST_STATIC_SECRET).public_key();
		let sealed = SealedBox::seal(static_key, &plaintext).unwrap();
		let expiry = (current_unix_secs() + 3600) << 32;
		let fields = vec![
			StatementField::Expiry(expiry),
			StatementField::Channel(rand::random()),
			StatementField::Topic1(topic.0),
			StatementField::Data(sealed.to_bytes()),
		];
		let signed =
			sign_statement_fields(signer.secret.to_bytes(), signer.public.to_bytes(), fields);
		signed.unwrap().encode()
	}

	fn current_unix_secs() -> u64 {
		let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
		now.unwrap().as_secs()
	}

	/// A cheque a real sender publishes on the live People chain reaches a
	/// product listening to its inbox. Needs a person in a People Lite ring:
	/// `TRUAPI_LIVE_PERSON_MNEMONIC` names one, and the test skips without it.
	/// The first run in a period takes one of the person's statement-store
	/// slots; later runs reuse it. Run with
	/// `cargo test -p truapi --lib live_payment_inbox -- --ignored`.
	#[tokio::test(flavor = "multi_thread")]
	#[ignore = "needs the live Paseo People Next chain and a person's mnemonic"]
	async fn live_payment_inbox_receives_a_published_cheque() {
		let Ok(mnemonic) = std::env::var("TRUAPI_LIVE_PERSON_MNEMONIC") else {
			eprintln!("skipped: TRUAPI_LIVE_PERSON_MNEMONIC is not set");
			return;
		};
		let entropy = bip39::Mnemonic::parse(mnemonic.trim()).unwrap().to_entropy();
		let chain = connect_live_people_chain().await;
		let signer = live_statement_signer(&entropy);
		allocate_live_statement_allowance(&chain, &entropy, &signer).await;
		let rpc_client = RpcClient::new(chain.host_client.clone());
		let (_dir, db) = open_temporary_core_db();
		let purses = test_purses_over(db, Arc::new(LiveStatements(rpc_client.clone())));
		let product = ProductId::for_test("tickets.dot");
		let inbox = purses.start_payment_inbox_listen(rand::random(), &product, rand::random());
		let mut listen = inbox.await.unwrap();
		let backlog = tokio::time::timeout(LIVE_WAIT, listen.arrivals.next()).await;
		assert_eq!(backlog.unwrap().map(Result::unwrap), Some(vec![]));
		let cheque = v01::NftPurseCheque {
			version: 0,
			id: v01::NftPurseChequeId(rand::random()),
			recipient: v01::NftPurseChequeRecipient::Static,
			items: vec![v01::NftPurseItemKey(rand::random())],
			encrypted_secrets: vec![0x55; 4],
		};
		let statement = signed_inbox_statement(&signer, listen.channel, &cheque);

		submit_sso(&rpc_client, statement, "live payment").await.unwrap();

		let arrived = tokio::time::timeout(LIVE_WAIT, listen.arrivals.next()).await;
		assert_eq!(arrived.unwrap().map(Result::unwrap), Some(vec![cheque]));
	}
}
