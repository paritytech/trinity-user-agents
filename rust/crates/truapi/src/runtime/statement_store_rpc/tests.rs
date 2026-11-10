use std::sync::{
	Arc, Mutex,
	atomic::{AtomicUsize, Ordering},
};

use futures::{
	channel::mpsc,
	stream::{BoxStream, StreamExt},
};
use serde_json::json;

use futures::executor::{block_on, block_on_stream};

use super::{LIVE_WAIT, LivePeopleStatements, StatementStoreRpc, StatementSubmitError};
use crate::{
	chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure, RuntimeFailureKind},
	host_logic::statement_store::{
		MAX_MATCH_ALL_TOPICS, NewStatements, SUBMIT_STATEMENT_METHOD, StatementStoreApi,
		StatementWatchError, TopicFilter, hex_topic,
	},
	platform::JsonRpcConnection,
	test_support::{StubPlatform, statement_page_frame, subscribe_ack_frame, test_spawner},
};

const PEOPLE_GENESIS: [u8; 32] = [0x11; 32];

/// What one People-chain connection does with the requests it receives.
#[derive(Clone, Copy)]
enum Upstream {
	/// Closes as soon as a request arrives.
	Drops,
	/// Answers every submission with this status.
	Answers(&'static str),
	/// Answers every submission with a JSON-RPC error carrying this message.
	Fails(&'static str),
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

#[async_trait::async_trait]
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
		let request: serde_json::Value = serde_json::from_str(&request).unwrap();
		let response = match self.upstream {
			Upstream::Drops => {
				sender.take();
				return;
			},
			Upstream::Answers(status) => json!({
				"jsonrpc": "2.0",
				"id": request["id"],
				"result": { "status": status, "reason": "noAllowance" },
			}),
			Upstream::Fails(message) => json!({
				"jsonrpc": "2.0",
				"id": request["id"],
				"error": { "code": -32602, "message": message },
			}),
		};
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
		statement_store.submit(&[1], "first").await.unwrap();
		statement_store.submit(&[2], "second").await.unwrap();
	});

	assert_eq!(chain.connects(), 1);
}

#[test]
fn a_later_submit_reconnects_after_a_dropped_connection() {
	let chain = PeopleChain::new(vec![Upstream::Drops, Upstream::Answers("new")]);
	let statement_store = statement_store(&chain);

	let (first, second) = futures::executor::block_on(async {
		let first = statement_store.submit(&[1], "first").await;
		let second = statement_store.submit(&[2], "second").await;
		(first, second)
	});

	assert_eq!((first.is_err(), second, chain.connects(), chain.requests()), (true, Ok(()), 2, 2));
}

/// A rejection is the store's answer, not a transport failure. Resending
/// the statement would only repeat the rejection.
#[test]
fn a_rejected_submit_is_not_resent() {
	let chain = PeopleChain::new(vec![Upstream::Answers("rejected")]);
	let statement_store = statement_store(&chain);

	let result = futures::executor::block_on(statement_store.submit(&[1], "submit"));

	assert_eq!((result.is_err(), chain.requests()), (true, 1));
}

#[test]
fn an_sso_submit_reports_a_dropped_connection() {
	let chain = PeopleChain::new(vec![Upstream::Drops, Upstream::Answers("new")]);
	let statement_store = statement_store(&chain);

	let result = futures::executor::block_on(statement_store.submit_sso(&[1], "sso"));

	assert_eq!(
		(result.is_err_and(|error| error.is_unreachable()), chain.connects(), chain.requests()),
		(true, 1, 1)
	);
}

/// A JSON-RPC error is an answer, so it must not read as an unreachable chain.
#[test]
fn a_node_error_is_not_reported_as_unreachable() {
	let chain = PeopleChain::new(vec![Upstream::Fails("invalid statement encoding")]);
	let statement_store = statement_store(&chain);

	let result = futures::executor::block_on(statement_store.submit_sso(&[1], "sso"));

	assert!(matches!(
		result,
		Err(StatementSubmitError::Chain(failure))
			if failure.kind() == RuntimeFailureKind::HostFailure
	));
}

/// A light client reports a statement that reached no peer as a node error.
/// Nothing reached the chain, so a signing host has to send it again.
#[test]
fn a_statement_that_reached_no_peer_is_reported_as_unreachable() {
	let chain = PeopleChain::new(vec![Upstream::Fails(NO_PEERS)]);
	let statement_store = statement_store(&chain);

	let result = futures::executor::block_on(statement_store.submit_sso(&[1], "sso"));

	assert!(result.is_err_and(|error| error.is_unreachable()));
}

#[test]
fn identifies_no_allowance_submit_rejections() {
	let rejected =
		|reason| StatementSubmitError::Rejected(json!({ "status": "rejected", "reason": reason }));
	assert_eq!(
		(rejected("noAllowance").is_no_allowance(), rejected("badProof").is_no_allowance()),
		(true, false)
	);
}

/// smoldot's two messages for a statement that reached no peer.
const NO_PEERS: &str = "Statement store error: No connected peers to broadcast the statement to";
const NO_PEER_ACCEPTED: &str =
	"Statement store error: none of the 3 connected peers accepted the statement";

/// Only a statement that reached no peer is resent. Other rejections,
/// such as a bad proof, would fail again.
#[test]
fn only_a_statement_that_reached_no_peer_is_unsent() {
	let node_error = |reason: &str| {
		StatementSubmitError::Chain(RuntimeFailure::host_failure(SUBMIT_STATEMENT_METHOD, reason))
	};
	let failures = [
		node_error(NO_PEERS),
		node_error(NO_PEER_ACCEPTED),
		StatementSubmitError::Rejected(json!({ "status": "rejected", "reason": "badProof" })),
	];

	let unsent: Vec<bool> = failures.iter().map(StatementSubmitError::reached_no_peer).collect();

	assert_eq!(unsent, vec![true, true, false]);
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
	let filter = TopicFilter::MatchAll(vec![[7; 32]]);

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
	let filter = TopicFilter::MatchAny(vec![[7; 32]]);

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
	let filter = TopicFilter::MatchAll(topics);

	let started = block_on(statement_store.start_statement_watch(filter));

	assert!(matches!(started, Err(StatementWatchError::NotStarted(_))));
	assert_eq!(platform.chain_connects.lock().unwrap().len(), 0);
}

/// The live People chain answers a watch of a topic nobody uses with one
/// empty batch, which is what tells an inbox it caught up. Run with
/// `cargo test -p truapi --lib live_statement_watch -- --ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Paseo People Next chain"]
async fn live_statement_watch_of_an_unused_topic_starts_with_one_empty_batch() {
	let statements = LivePeopleStatements::connect().await;
	let filter = TopicFilter::MatchAll(vec![rand::random()]);

	let mut batches = statements.start_statement_watch(filter).await.unwrap();
	let first = tokio::time::timeout(LIVE_WAIT, batches.next()).await;

	assert_eq!(first.unwrap().map(Result::unwrap), Some(vec![]));
}

fn statement_page(statements: &[u8], remaining: Option<u64>) -> Result<NewStatements, String> {
	let statements = statements.iter().map(|statement| vec![*statement]).collect();
	Ok(NewStatements { remote_subscription_id: "sub".to_string(), statements, remaining })
}

fn batches_of(pages: Vec<Result<NewStatements, String>>) -> Vec<Result<Vec<Vec<u8>>, String>> {
	block_on_stream(super::backlog_then_live_batches(futures::stream::iter(pages))).collect()
}

/// A product reading an inbox gets everything already there in one item,
/// then each new page as it arrives.
#[test]
fn the_backlog_is_one_batch_and_each_later_page_its_own() {
	let pages = vec![
		statement_page(&[1, 2], Some(1)),
		statement_page(&[3], Some(0)),
		statement_page(&[4], None),
		statement_page(&[5], None),
	];

	let expected = vec![Ok(vec![vec![1], vec![2], vec![3]]), Ok(vec![vec![4]]), Ok(vec![vec![5]])];
	assert_eq!(batches_of(pages), expected);
}

/// An empty topic still reports that the backlog is complete.
#[test]
fn an_empty_backlog_is_one_empty_batch() {
	let pages = vec![statement_page(&[], Some(0))];

	assert_eq!(batches_of(pages), vec![Ok(vec![])]);
}

/// The node may interleave new statements, which carry no `remaining`,
/// with the backlog pages.
#[test]
fn a_page_without_remaining_before_the_backlog_ends_joins_the_backlog() {
	let pages = vec![
		statement_page(&[1], Some(1)),
		statement_page(&[2], None),
		statement_page(&[3], Some(0)),
	];

	assert_eq!(batches_of(pages), vec![Ok(vec![vec![1], vec![2], vec![3]])]);
}

/// A failure must reach the reader, wherever it happens.
#[test]
fn errors_pass_on_before_and_after_the_backlog() {
	let failure = "transport".to_string();
	let pages = vec![
		statement_page(&[1], Some(1)),
		Err(failure.clone()),
		statement_page(&[2], Some(0)),
		Err(failure.clone()),
	];

	let expected = vec![Err(failure.clone()), Ok(vec![vec![1], vec![2]]), Err(failure)];
	assert_eq!(batches_of(pages), expected);
}
