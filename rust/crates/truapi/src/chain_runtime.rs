//! ChainHead v1 state machine used by `ProductRuntimeHost`.
//!
//! [`ChainRuntime`] keeps one [`ChainConnection`] per chain (keyed by genesis
//! hash) on top of the platform-provided [`JsonRpcConnection`]. The generic
//! JSON-RPC mechanics are delegated to [`crate::host_rpc_client`], while
//! `subxt-rpcs` owns the raw `chainHead_v1` method shapes and event parsing.
//! This module keeps the TrUAPI-facing local follow ids and maps subxt DTOs to
//! public v01 [`RemoteChainHeadFollowItem`] values.
//!
//! Each connection also lazily caches one genesis-pinned Subxt
//! [`OnlineClient`], its backend driver, and Subxt's per-client metadata cache.
//! Internal services use that client instead of hand-rolling chainHead
//! orchestration where Subxt fits the boundary.
//!
//! The chain-side traits return [`RuntimeFailure`], a local classification
//! that the [`crate::runtime`] layer maps to [`truapi::CallError`] variants
//! (`Unsupported`, `HostFailure`, ...). This avoids leaking json-rpc plumbing
//! into the public API.

use core::{
	pin::Pin,
	task::{Context, Poll},
};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, SystemTime};
use std::{
	collections::{BTreeMap, HashMap, HashSet, VecDeque},
	sync::{
		Arc, Mutex, OnceLock, Weak,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, SystemTime};

use crate::platform::{ChainProvider, JsonRpcConnection, ProviderError};
use derive_more::{Display, Error};
use futures::{
	FutureExt, Stream, StreamExt,
	channel::mpsc,
	future::{AbortHandle, Abortable, BoxFuture, Shared, join_all},
	pin_mut,
	stream::BoxStream,
};
use parity_scale_codec::{Decode, Error as ScaleError, Input};
use parking_lot::Mutex as ParkingMutex;
use serde::de::{Deserializer, Error as DeError};
use serde_json::Value;
use subxt::{
	OnlineClient,
	backend::{ChainHeadBackend, LegacyBackend},
	config::{
		RpcConfigFor,
		substrate::{SubstrateConfig, SubstrateConfigBuilder},
	},
	utils::H256,
};
use subxt_rpcs::{
	ChainHeadRpcMethods, Error as SubxtRpcError, LegacyRpcMethods, RpcConfig, client::RpcClient,
	methods::chain_head as subxt_chain,
};
use tracing::{instrument, warn};
use truapi::v01::{
	OperationStartedResult, RemoteChainHeadBodyRequest, RemoteChainHeadBodyResponse,
	RemoteChainHeadCallRequest, RemoteChainHeadCallResponse, RemoteChainHeadContinueRequest,
	RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest, RemoteChainHeadHeaderRequest,
	RemoteChainHeadHeaderResponse, RemoteChainHeadStopOperationRequest,
	RemoteChainHeadStorageRequest, RemoteChainHeadStorageResponse, RemoteChainHeadUnpinRequest,
	RemoteChainSpecChainNameResponse, RemoteChainSpecGenesisHashResponse,
	RemoteChainSpecPropertiesResponse, RemoteChainTransactionBroadcastRequest,
	RemoteChainTransactionBroadcastResponse, RemoteChainTransactionStopRequest, RuntimeApi,
	RuntimeSpec, RuntimeType, StorageQueryItem, StorageQueryType, StorageResultItem,
};

use crate::{
	Spawner,
	host_rpc_client::HostRpcClient,
	time::{RealTimer, Timer},
};

mod chain_halt;
mod current_block;
mod follow_retry;

pub use chain_halt::{retry_while_halted, says_chain_halted};
pub use current_block::CurrentBlockError;
use follow_retry::{FollowReport, FollowRetryingRpcClient};

mod runtime_call_cache;
mod runtime_call_client;

use runtime_call_cache::RuntimeCallCache;
use runtime_call_client::RuntimeCallClient;

const FOLLOW_METHOD: &str = "remote_chain_head_follow";
const CONNECT_METHOD: &str = "remote_chain_connect";
/// Method tag for failures building or using the legacy Subxt client.
const LEGACY_CONNECTION: &str = "legacy_connection";

struct TruapiRpcConfig;

impl RpcConfig for TruapiRpcConfig {
	type Header = RawHeader;
	type Hash = H256;
	type AccountId = ();
}

#[derive(Debug, Clone)]
struct RawHeader(Vec<u8>);

impl Decode for RawHeader {
	fn decode<I: Input>(input: &mut I) -> Result<Self, ScaleError> {
		let Some(len) = input.remaining_len()? else {
			return Err("raw header input length is unknown".into());
		};
		let mut bytes = vec![0u8; len];
		input.read(&mut bytes)?;
		Ok(Self(bytes))
	}
}

impl<'de> serde::Deserialize<'de> for RawHeader {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let bytes = subxt_chain::Bytes::deserialize(deserializer).map_err(D::Error::custom)?;
		Ok(Self(bytes.0))
	}
}

/// Shared, single-flight `chainHead_v1_follow` setup keyed by local follow id.
/// Concurrent callers for the same id await one in-flight request rather than
/// each opening (and leaking) a separate remote subscription.
type FollowSetup = Shared<BoxFuture<'static, Result<String, RuntimeFailure>>>;

/// Shared start task published synchronously when a local follow stream is
/// created. Follow-bound operations await it so they cannot overtake setup.
type LocalFollowStart = Shared<BoxFuture<'static, Result<(), RuntimeFailure>>>;
type FollowStartRegistry = Arc<ParkingMutex<HashMap<(Vec<u8>, String), LocalFollowStart>>>;

/// One scope's registered follow ids, keyed by genesis hash.
type ScopeFollowIntents = HashMap<String, HashSet<String>>;

/// One scope's broadcast operations, keyed by the sequence in the id handed out.
type ScopeTransactionOperations = BTreeMap<u64, TransactionOperation>;

/// Shared, single-flight provider connect keyed by genesis hash. Concurrent
/// first connections for the same chain await one in-flight `connect` rather
/// than each opening a connection and orphaning all but the last insert.
type ConnectionSetup = Shared<BoxFuture<'static, Result<Arc<ChainConnection>, RuntimeFailure>>>;

/// Shared, single-flight setup of the connection's cached Subxt client.
/// Concurrent first users await one in-flight build rather than each starting
/// (and leaking) a separate backend driver and follow subscription.
type SubxtConnectionSetup = Shared<BoxFuture<'static, Result<SubxtConnection, RuntimeFailure>>>;

/// Cached Subxt client built over one connection's transport.
/// The backend driver is owned by the setup task that created this value.
#[derive(Clone)]
pub struct SubxtConnection {
	/// Client whose chain config pins the host-configured genesis hash.
	pub client: OnlineClient<SubstrateConfig>,
	/// What the backend's RPC client has learned about its follow.
	follow: Arc<FollowReport>,
	/// The connection the client reads over. Weak, since that connection
	/// caches this bundle.
	connection: Weak<ChainConnection>,
}

impl SubxtConnection {
	/// Whether the connection under this client has closed, or is gone.
	fn connection_closed(&self) -> bool {
		self.connection.upgrade().is_none_or(|connection| connection.is_closed())
	}
}

/// Cached Subxt client over the legacy JSON-RPC methods, which reach any
/// block the node still keeps rather than only blocks pinned by a chainHead
/// follow. It shares its chain config, and so its metadata cache, with the
/// connection's [`SubxtConnection`].
#[derive(Clone)]
pub struct LegacyConnection {
	/// Client for metadata-aware reads at a block, such as events.
	pub client: OnlineClient<SubstrateConfig>,
	/// Backend for block-level reads that need no metadata.
	pub backend: Arc<LegacyBackend<SubstrateConfig>>,
	/// Legacy methods the backend does not expose, such as the best block hash.
	pub methods: LegacyRpcMethods<RpcConfigFor<SubstrateConfig>>,
	/// Raw JSON-RPC client, for legacy subscriptions neither of the above
	/// offers, such as `state_subscribeStorage`.
	pub raw_rpc_client: RpcClient,
}

/// Classification of framework-level chain failures separate from JSON-RPC
/// domain errors. Maps cleanly to [`truapi::CallError`] variants at the
/// `ProductRuntimeHost` boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeFailureKind {
	/// The chain could not be reached: the backend is not wired, the node
	/// could not be dialled, or the connection is gone. A later attempt can
	/// succeed.
	Unavailable,
	/// Backend responded but the payload was malformed or the call failed.
	HostFailure,
}

/// Framework-level chain failure with a diagnostic reason.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
#[display(
    "{method}{}{}",
    if reason.is_some() { ": " } else { "" },
    reason.as_deref().unwrap_or_default()
)]
pub struct RuntimeFailure {
	kind: RuntimeFailureKind,
	method: &'static str,
	reason: Option<String>,
	/// Chain the call was made against, where the failure knows it.
	///
	/// Kept beside the reason rather than inside it, because the reason is what
	/// the product is told and that should not change, while a host reading its
	/// log needs to know which chain went quiet. A failure raised before a
	/// chain is chosen has none.
	chain: Option<Vec<u8>>,
}

impl RuntimeFailure {
	/// Backend refused the call for unavailability reasons (no provider, the
	/// connection died, etc.).
	pub fn unavailable(method: &'static str) -> Self {
		Self { kind: RuntimeFailureKind::Unavailable, method, reason: None, chain: None }
	}

	/// Backend produced a structural error (malformed json-rpc, unexpected
	/// shape, ...).
	pub fn host_failure(method: &'static str, reason: impl Into<String>) -> Self {
		Self {
			kind: RuntimeFailureKind::HostFailure,
			method,
			reason: Some(reason.into()),
			chain: None,
		}
	}

	/// [`Self::unavailable`] carrying the underlying failure text for
	/// diagnostics.
	pub fn unavailable_with_reason(method: &'static str, reason: impl Into<String>) -> Self {
		Self {
			kind: RuntimeFailureKind::Unavailable,
			method,
			reason: Some(reason.into()),
			chain: None,
		}
	}

	/// Failure classification.
	pub fn kind(&self) -> RuntimeFailureKind {
		self.kind
	}

	/// Method tag the failure originated from.
	pub fn method(&self) -> &'static str {
		self.method
	}

	/// Say which chain this failure happened on.
	///
	/// A failure raised before a chain is chosen leaves it unset, so a reader
	/// can tell "this chain went quiet" from "we never got that far".
	#[must_use]
	pub fn on_chain(mut self, genesis_hash: &[u8]) -> Self {
		self.chain = Some(genesis_hash.to_vec());
		self
	}

	/// Chain the call was made against, where the failure knows it.
	pub fn chain(&self) -> Option<&[u8]> {
		self.chain.as_deref()
	}

	/// Diagnostic reason. Always non-empty for `HostFailure`.
	pub fn reason(&self) -> String {
		match &self.reason {
			Some(reason) => format!("{}: {}", self.method, reason),
			None => self.method.to_string(),
		}
	}

	/// Re-tag this failure under `method`, preserving its kind and nesting
	/// the original method and reason so the caller still sees where it
	/// failed.
	fn reclassify(&self, method: &'static str) -> RuntimeFailure {
		RuntimeFailure {
			kind: self.kind,
			method,
			reason: Some(self.reason()),
			chain: self.chain.clone(),
		}
	}
}

/// Provider of `JsonRpcConnection` instances keyed by chain genesis hash.
/// Hosts plug in the platform-side `ChainProvider`.
#[async_trait::async_trait]
pub trait RuntimeChainProvider: Send + Sync {
	/// Open or reuse a JSON-RPC connection for the chain identified by
	/// `genesis_hash`.
	async fn connect(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure>;
}

/// Adapter from a host's [`ChainProvider`] into the [`RuntimeChainProvider`]
/// surface the chain runtime expects.
pub struct HostChainProvider {
	provider: Arc<dyn ChainProvider>,
}

impl HostChainProvider {
	/// Opens connections through `provider`.
	pub fn new(provider: Arc<dyn ChainProvider>) -> Self {
		Self { provider }
	}
}

#[async_trait::async_trait]
impl RuntimeChainProvider for HostChainProvider {
	async fn connect(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
		let genesis_hash: [u8; 32] = genesis_hash.try_into().map_err(|genesis_hash: Vec<u8>| {
			RuntimeFailure::host_failure(
				"remote_chain_connect",
				format!("genesis_hash must be 32 bytes, got {}", genesis_hash.len()),
			)
		})?;
		self.provider
			.connect(genesis_hash)
			.await
			.map(Arc::from)
			.map_err(|err| match err {
				// The node could not be reached, which a later attempt can outlast.
				ProviderError::Handshake { .. } | ProviderError::Transport { .. } => {
					RuntimeFailure::unavailable_with_reason(
						"remote_chain_connect",
						format!("{err:?}"),
					)
				},
				// The provider refused: it does not serve the chain, is not set
				// up to connect, or holds all the connections it allows. Asking
				// again straight away gets the same answer.
				_ => RuntimeFailure::host_failure("remote_chain_connect", format!("{err:?}")),
			})
	}
}

/// Follows a node allows per connection, as [`ConnectionScope`] describes. A
/// connection holding this many refuses the next one, whichever caller it
/// belongs to.
const FOLLOWS_A_NODE_BUDGETS: usize = 4;

/// Which caller a chain connection belongs to.
///
/// A node allows four `chainHead_v1_follow` subscriptions and sixteen concurrent
/// operations per connection, so one connection shared across products makes
/// those budgets collide: four products plus the core's own client is a fifth
/// follow the node refuses. Each product runtime gets its own connection, and
/// the core's internal traffic keeps one of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ConnectionScope {
	/// The core's own traffic: metadata, transaction assembly, internal reads.
	Core,
	/// One product runtime, identified by its `core_instance`.
	///
	/// Per runtime instance rather than per product id, so each connection has
	/// exactly one owner and releasing it is a drop. Keying by product id would
	/// put two concurrent runtimes of the same product on one connection, which
	/// needs refcounting and a liveness story for a runtime that dies holding
	/// it. The cost is that `App`, `Widget` and `Worker` are separate runtimes,
	/// so one product with a card and a chat extension holds three connections.
	Product(u64),
}

/// chainHead-v1 state machine on top of a [`RuntimeChainProvider`].
///
/// Each method maps a typed v01 chain request to one or more json-rpc calls,
/// shares one `chainHead_v1_follow` subscription per (genesis_hash, local
/// follow id) pair, and parses follow events back into typed
/// [`RemoteChainHeadFollowItem`] values.
#[derive(Clone)]
pub struct ChainRuntime {
	/// The way out to a host, which opens the sockets this runtime hands around.
	provider: Arc<dyn RuntimeChainProvider>,
	/// Where response pumps and follow setup run.
	spawner: Spawner,
	/// Which caller this view belongs to. Cheap to re-scope: every other
	/// field is shared, so two scopes see the same maps under different keys.
	scope: ConnectionScope,
	/// Open connections, grouped by scope and then keyed by genesis hash.
	/// A scope's group exists only while that scope is live, so releasing one
	/// takes the whole group with it and a connect still in flight has nowhere
	/// to land. Two callers on one chain are two entries, because a node
	/// budgets follows per connection.
	connections: Arc<Mutex<HashMap<ConnectionScope, HashMap<String, ConnectionEntry>>>>,
	/// Follow setup in flight, keyed by genesis hash and local follow id, so a
	/// follow-bound request can await the setup rather than miss the follow.
	follow_starts: FollowStartRegistry,
	/// Synchronously registered follow intents, grouped by scope and then keyed
	/// by genesis hash, so a scope resolves only its own follows and releasing
	/// it takes them with it. Alias cardinality includes setup tasks that have
	/// not yet been polled.
	follow_intents: Arc<Mutex<HashMap<ConnectionScope, ScopeFollowIntents>>>,
	/// Broadcast operations each scope started, keyed by the sequence in the id
	/// handed to the product. A scope reaches only its own, so an id another
	/// product knows or guesses is unknown here.
	transaction_operations: Arc<Mutex<HashMap<ConnectionScope, ScopeTransactionOperations>>>,
	/// Mints the sequence above. One counter for every scope, so no two
	/// callers can be handed the same operation id.
	next_transaction_operation_id: Arc<AtomicU64>,
	/// Paces follow retries and bounds reads, so tests can control the waits.
	timer: Arc<dyn Timer>,
	/// Runtime call outputs every connection shares, so the products and the
	/// core download the runtime metadata of a chain once between them.
	runtime_calls: RuntimeCallCache,
	/// Follows this runtime issued that have since ended, newest last, grouped
	/// by scope, each with why it ended. A follow does not outlive its
	/// subscription and the id stays in the caller's hands, so without this
	/// the call it makes next is answered as an id this runtime never issued.
	/// Bounded: the oldest is forgotten rather than grown without limit, and
	/// forgetting one only costs the better message.
	ended_follows: EndedFollows,
}

/// Record of ended follows, shared by every scope and connection a runtime
/// owns, written where a follow ends and read where an id no longer resolves.
type EndedFollows = Arc<Mutex<HashMap<ConnectionScope, VecDeque<(String, FollowLoss)>>>>;

/// Why a follow this runtime issued stopped answering. Every reason means the
/// same thing to a caller, that the id is spent and a new follow is needed,
/// and they are distinguished because they are not diagnosed the same way.
#[derive(Clone, Copy)]
enum FollowLoss {
	/// The connection carrying the subscription closed under it.
	ConnectionClosed,
	/// The node ended the subscription with a `stop` event, with the
	/// connection still up.
	StoppedByNode,
	/// The subscription failed on a connection that did not close: an event
	/// that could not be read, or an error the node sent in its place.
	Failed,
}

impl FollowLoss {
	/// The clause naming this loss in the failure the caller reads.
	fn clause(self) -> &'static str {
		match self {
			Self::ConnectionClosed => "ended when its connection closed",
			Self::StoppedByNode => "was stopped by the node",
			Self::Failed => "ended on an error with its connection still open",
		}
	}
}

/// How many lost follows a scope remembers. Generous next to the four a node
/// budgets per connection, so the answer survives a reconnect storm.
const REMEMBERED_LOST_FOLLOWS: usize = 32;

#[derive(Clone)]
struct TransactionOperation {
	genesis_hash: Vec<u8>,
	provider_operation_id: String,
}

/// Prefix of the host-assigned transaction operation ids handed to products.
const TRANSACTION_OPERATION_ID_PREFIX: &str = "truapi-tx-";
/// Cap on tracked broadcast operations per scope; a product that never stops an
/// operation would otherwise grow its map for the whole session. Provider
/// operation ids are short-lived, so evicting the oldest handles is safe.
const MAX_TRACKED_TRANSACTION_OPERATIONS: usize = 256;

fn transaction_operation_sequence(operation_id: &str) -> Option<u64> {
	operation_id.strip_prefix(TRANSACTION_OPERATION_ID_PREFIX)?.parse().ok()
}

fn follow_product_namespace(follow_id: &str) -> Option<&str> {
	let (namespace, _) = follow_id.split_once(':')?;
	let instance = namespace.strip_prefix('c')?;
	(!instance.is_empty() && instance.bytes().all(|byte| byte.is_ascii_digit()))
		.then_some(namespace)
}

impl ChainRuntime {
	/// Build a `ChainRuntime` driven by `provider`. Background tasks (response
	/// pumps, follow setup) are spawned on `spawner`.
	pub fn new(provider: Arc<dyn RuntimeChainProvider>, spawner: Spawner) -> Self {
		Self {
			provider,
			spawner,
			scope: ConnectionScope::Core,
			connections: Arc::new(Mutex::new(HashMap::from([(
				ConnectionScope::Core,
				HashMap::new(),
			)]))),
			follow_starts: Arc::new(ParkingMutex::new(HashMap::new())),
			follow_intents: Arc::new(Mutex::new(HashMap::new())),
			transaction_operations: Arc::new(Mutex::new(HashMap::new())),
			next_transaction_operation_id: Arc::new(AtomicU64::new(1)),
			timer: Arc::new(RealTimer),
			runtime_calls: RuntimeCallCache::default(),
			ended_follows: Arc::new(Mutex::new(HashMap::from([(
				ConnectionScope::Core,
				VecDeque::new(),
			)]))),
		}
	}

	/// This runtime on `timer` instead of wall-clock time. Set it before the
	/// first connection, which keeps the timer it opened with.
	#[cfg(test)]
	pub fn with_timer(mut self, timer: Arc<dyn Timer>) -> Self {
		self.timer = timer;
		self
	}

	/// This runtime seen as `scope`. Shares every map with the original, so a
	/// product's connections live beside the core's rather than replacing them.
	pub fn for_product(&self, core_instance: u64) -> Self {
		let scope = ConnectionScope::Product(core_instance);
		self.connections.lock().unwrap().entry(scope).or_default();
		self.ended_follows.lock().unwrap().entry(scope).or_default();
		Self { scope, ..self.clone() }
	}

	/// Key for this scope's connection to `genesis_hash`, within this scope's
	/// own map. The scope is the outer key, so the chain alone identifies the
	/// connection here and two scopes on one chain are still two connections.
	fn connection_key(&self, genesis_hash: &[u8]) -> String {
		encode_hex(genesis_hash)
	}

	/// Drop everything this scope holds, on every chain, and tear it down.
	///
	/// Removing the map's handle is not enough on its own. Each live follow
	/// runs a pump holding a strong handle to the connection, so until those
	/// are aborted the connection cannot drop, both `HostRpcClient` handles
	/// stay alive, and the host's socket is never closed. Removing the entry
	/// first would also destroy the route `cleanup_follow` uses to find the
	/// connection, so a later teardown could no longer abort anything.
	///
	/// Pending connections occupy an entry too; completion cannot restore an
	/// entry removed by teardown.
	pub fn release_scope(&self) {
		let dropped: Vec<Arc<ChainConnection>> = self
			.connections
			.lock()
			.unwrap()
			.remove(&self.scope)
			.into_iter()
			.flat_map(|connections| connections.into_values())
			.filter_map(|entry| entry.ready().cloned())
			.collect();

		// `follow_intents` already holds exactly this scope's follow ids, so
		// sweeping `follow_starts` against them needs no assumption about how
		// an id is shaped and covers host-internal follows too.
		let released_follow_ids: HashSet<String> = self
			.follow_intents
			.lock()
			.unwrap()
			.remove(&self.scope)
			.into_iter()
			.flat_map(|chains| chains.into_values().flatten())
			.collect();
		self.transaction_operations.lock().unwrap().remove(&self.scope);
		self.ended_follows.lock().unwrap().remove(&self.scope);
		self.follow_starts
			.lock()
			.retain(|(_, follow_id), _| !released_follow_ids.contains(follow_id));

		for connection in dropped {
			connection.abort_all_follows();
		}
	}

	/// Start (or attach to an existing) `chainHead_v1_follow` subscription.
	/// Returns a stream of typed follow items that ends with `Err` when setup
	/// or the remote subscription fails, and without one when the remote sends
	/// `stop`.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.follow"))]
	pub fn remote_chain_head_follow(
		&self,
		follow_subscription_id: String,
		request: RemoteChainHeadFollowRequest,
	) -> BoxStream<'static, Result<RemoteChainHeadFollowItem, RuntimeFailure>> {
		// Product SDKs assign operation aliases independently of the outer
		// subscription frame id. Publish the intent synchronously so the next
		// product frame can resolve its alias before async setup is polled.
		self.register_follow_intent(&request.genesis_hash, &follow_subscription_id);
		let (tx, rx) = mpsc::unbounded();
		let setup_tx = tx.clone();
		let runtime = self.clone();
		let cleanup_runtime = self.clone();
		let cleanup_genesis_hash = request.genesis_hash.clone();
		let cleanup_follow_id = follow_subscription_id.clone();
		let setup_genesis_hash = cleanup_genesis_hash.clone();
		let setup_follow_id = cleanup_follow_id.clone();
		let follow_start_key = (request.genesis_hash.clone(), follow_subscription_id.clone());
		let cancelled = Arc::new(AtomicBool::new(false));
		let setup_cancelled = cancelled.clone();
		let cleanup_cancelled = cancelled.clone();

		// Publish the shared start before spawning it. A follow-bound request
		// may arrive in the next product frame before the executor first polls
		// this task; it must await this exact setup rather than reject the
		// provider-synthetic local follow id as unknown.
		let start: LocalFollowStart = async move {
			let result =
				runtime.start_follow(follow_subscription_id, request, tx, setup_cancelled).await;
			if let Err(failure) = &result {
				// Cleanup drops the sender the connection holds, which on its
				// own reads as a clean end. Report the failure on this
				// caller's own handle first.
				let _ = setup_tx.unbounded_send(Err(failure.clone()));
				runtime.cleanup_follow(&setup_genesis_hash, &setup_follow_id);
			}
			result
		}
		.boxed()
		.shared();
		self.record_follow_start(follow_start_key, start.clone());
		self.spawner.spawn(async move {
			let _ = start.await;
		});

		ManagedSubscription::new(
			rx.boxed(),
			Some(Box::new(move || {
				cleanup_cancelled.store(true, Ordering::SeqCst);
				cleanup_runtime.cleanup_follow(&cleanup_genesis_hash, &cleanup_follow_id);
			})),
		)
		.boxed()
	}

	/// [`Self::remote_chain_head_follow`] for host-internal callers that read
	/// a follow themselves. They wait for the events one operation needs and
	/// time out on their own, so a failure ends the stream rather than
	/// becoming an item every one of them has to match.
	pub fn remote_chain_head_follow_items(
		&self,
		follow_subscription_id: String,
		request: RemoteChainHeadFollowRequest,
	) -> BoxStream<'static, RemoteChainHeadFollowItem> {
		self.remote_chain_head_follow(follow_subscription_id, request)
			.scan((), |_, item| core::future::ready(item.ok()))
			.boxed()
	}

	/// Fetch a block header.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.header"))]
	pub async fn remote_chain_head_header(
		&self,
		request: RemoteChainHeadHeaderRequest,
	) -> Result<RemoteChainHeadHeaderResponse, RuntimeFailure> {
		let method = "remote_chain_head_header";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;

		let hash = hash_from_bytes(method, &request.hash)?;
		let header = connection
			.methods
			.chainhead_v1_header(&remote_follow_id, hash)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))?
			.map(|header| header.0);
		Ok(RemoteChainHeadHeaderResponse { header })
	}

	/// Start a chainHead_v1_body operation.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.body"))]
	pub async fn remote_chain_head_body(
		&self,
		request: RemoteChainHeadBodyRequest,
	) -> Result<RemoteChainHeadBodyResponse, RuntimeFailure> {
		let method = "remote_chain_head_body";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;

		let operation = connection
			.methods
			.chainhead_v1_body(&remote_follow_id, hash_from_bytes(method, &request.hash)?)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
			.and_then(operation_started_result)?;
		Ok(RemoteChainHeadBodyResponse { operation })
	}

	/// Start a chainHead_v1_storage operation.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.storage"))]
	pub async fn remote_chain_head_storage(
		&self,
		request: RemoteChainHeadStorageRequest,
	) -> Result<RemoteChainHeadStorageResponse, RuntimeFailure> {
		let method = "remote_chain_head_storage";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;

		let items = request.items.iter().map(map_storage_query_item).collect::<Vec<_>>();

		let operation = connection
			.methods
			.chainhead_v1_storage(
				&remote_follow_id,
				hash_from_bytes(method, &request.hash)?,
				items,
				request.child_trie.as_deref(),
			)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
			.and_then(operation_started_result)?;
		Ok(RemoteChainHeadStorageResponse { operation })
	}

	/// Start a chainHead_v1_call operation.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.call"))]
	pub async fn remote_chain_head_call(
		&self,
		request: RemoteChainHeadCallRequest,
	) -> Result<RemoteChainHeadCallResponse, RuntimeFailure> {
		let method = "remote_chain_head_call";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, true)
			.await?;

		let operation = connection
			.methods
			.chainhead_v1_call(
				&remote_follow_id,
				hash_from_bytes(method, &request.hash)?,
				&request.function,
				&request.call_parameters,
			)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
			.and_then(operation_started_result)?;
		Ok(RemoteChainHeadCallResponse { operation })
	}

	/// Starts calls concurrently and returns their outcomes in request order.
	///
	/// Every request must belong to the supplied follow. The timeout starts
	/// after all startup responses arrive and applies only to unfinished calls.
	/// A stopped follow fails unfinished calls while retaining completed results.
	pub async fn chain_head_call_batch(
		&self,
		follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
		requests: Vec<RemoteChainHeadCallRequest>,
		label: &'static str,
		timeout: Duration,
		timer: &dyn Timer,
	) -> Vec<Result<Vec<u8>, String>> {
		let operations =
			join_all(requests.into_iter().map(|request| self.remote_chain_head_call(request)))
				.await;
		let mut results = vec![None; operations.len()];
		let mut pending = HashMap::new();
		for (index, response) in operations.into_iter().enumerate() {
			match response.map(|response| response.operation) {
				Ok(OperationStartedResult::Started { operation_id }) => {
					pending.insert(operation_id, index);
				},
				Ok(OperationStartedResult::LimitReached) => {
					results[index] = Some(Err(format!("{label} operation limit reached")));
				},
				Err(failure) => results[index] = Some(Err(failure.reason())),
			}
		}
		let timeout = timer.sleep(timeout).fuse();
		pin_mut!(timeout);
		let failure = loop {
			if pending.is_empty() {
				break None;
			}
			let next = follow.next().fuse();
			pin_mut!(next);
			let item = futures::select! {
				item = next => item,
				() = timeout => break Some(format!("{label} runtime call timed out")),
			};
			let (operation_id, result) = match item {
				Some(RemoteChainHeadFollowItem::OperationCallDone { operation_id, output }) => {
					(operation_id, Ok(output))
				},
				Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id }) => {
					(operation_id, Err(format!("{label} runtime call was inaccessible")))
				},
				Some(RemoteChainHeadFollowItem::OperationError { operation_id, error }) => {
					(operation_id, Err(error))
				},
				Some(RemoteChainHeadFollowItem::Stop) | None => {
					break Some(format!("{label} follow stopped during runtime call"));
				},
				_ => continue,
			};
			if let Some(index) = pending.remove(&operation_id) {
				results[index] = Some(result);
			}
		};
		if let Some(error) = failure {
			for index in pending.into_values() {
				results[index] = Some(Err(error.clone()));
			}
		}
		results
			.into_iter()
			.map(|result| result.expect("every call has an outcome"))
			.collect()
	}

	/// Release pinned blocks.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.unpin"))]
	pub async fn remote_chain_head_unpin(
		&self,
		request: RemoteChainHeadUnpinRequest,
	) -> Result<(), RuntimeFailure> {
		let method = "remote_chain_head_unpin";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;
		let hashes = request
			.hashes
			.iter()
			.map(|hash| hash_from_bytes(method, hash))
			.collect::<Result<Vec<_>, _>>()?;
		for hash in hashes {
			connection
				.methods
				.chainhead_v1_unpin(&remote_follow_id, hash)
				.await
				.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))?;
		}
		Ok(())
	}

	/// Continue a paused operation.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.continue"))]
	pub async fn remote_chain_head_continue(
		&self,
		request: RemoteChainHeadContinueRequest,
	) -> Result<(), RuntimeFailure> {
		let method = "remote_chain_head_continue";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;
		connection
			.methods
			.chainhead_v1_continue(&remote_follow_id, &request.operation_id)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
	}

	/// Stop a chain-head operation.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.stop_operation"))]
	pub async fn remote_chain_head_stop_operation(
		&self,
		request: RemoteChainHeadStopOperationRequest,
	) -> Result<(), RuntimeFailure> {
		let method = "remote_chain_head_stop_operation";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let remote_follow_id = self
			.ensure_follow_context(method, &connection, request.follow_subscription_id, false)
			.await?;
		connection
			.methods
			.chainhead_v1_stop_operation(&remote_follow_id, &request.operation_id)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
	}

	/// Echo back the chain genesis hash via chainSpec_v1_genesisHash.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_genesis_hash"))]
	pub async fn remote_chain_spec_genesis_hash(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<RemoteChainSpecGenesisHashResponse, RuntimeFailure> {
		let method = "remote_chain_spec_genesis_hash";
		let connection = self.connection_for(method, &genesis_hash).await?;
		let genesis_hash = connection
			.methods
			.chainspec_v1_genesis_hash()
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))
			.map(hash_to_bytes)?;
		Ok(RemoteChainSpecGenesisHashResponse { genesis_hash })
	}

	/// Fetch the chain display name via chainSpec_v1_chainName.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_chain_name"))]
	pub async fn remote_chain_spec_chain_name(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<RemoteChainSpecChainNameResponse, RuntimeFailure> {
		let method = "remote_chain_spec_chain_name";
		let connection = self.connection_for(method, &genesis_hash).await?;
		let chain_name = connection
			.methods
			.chainspec_v1_chain_name()
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))?;
		Ok(RemoteChainSpecChainNameResponse { chain_name })
	}

	/// Fetch the chain JSON properties via chainSpec_v1_properties.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_properties"))]
	pub async fn remote_chain_spec_properties(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<RemoteChainSpecPropertiesResponse, RuntimeFailure> {
		let method = "remote_chain_spec_properties";
		let connection = self.connection_for(method, &genesis_hash).await?;
		let value = connection
			.methods
			.chainspec_v1_properties::<Value>()
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))?;
		let properties = serde_json::to_string(&value)
			.map_err(|err| RuntimeFailure::host_failure(method, err.to_string()))?;
		Ok(RemoteChainSpecPropertiesResponse { properties })
	}

	/// Broadcast a signed transaction via transaction_v1_broadcast.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.transaction_broadcast"))]
	pub async fn remote_chain_transaction_broadcast(
		&self,
		request: RemoteChainTransactionBroadcastRequest,
	) -> Result<RemoteChainTransactionBroadcastResponse, RuntimeFailure> {
		let method = "remote_chain_transaction_broadcast";
		let connection = self.connection_for(method, &request.genesis_hash).await?;
		let provider_operation_id = connection
			.methods
			.transaction_v1_broadcast(&request.transaction)
			.await
			.map_err(|err| rpc_failure(method, err).on_chain(&connection.genesis_hash))?;
		let operation_id = provider_operation_id.map(|provider_operation_id| {
			let sequence = self.next_transaction_operation_id.fetch_add(1, Ordering::Relaxed);
			self.with_transaction_operations(|operations| {
				operations.insert(
					sequence,
					TransactionOperation {
						genesis_hash: request.genesis_hash,
						provider_operation_id,
					},
				);
				while operations.len() > MAX_TRACKED_TRANSACTION_OPERATIONS {
					operations.pop_first();
				}
			});
			format!("{TRANSACTION_OPERATION_ID_PREFIX}{sequence}")
		});
		Ok(RemoteChainTransactionBroadcastResponse { operation_id })
	}

	/// Stop a transaction broadcast via transaction_v1_stop.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.transaction_stop"))]
	pub async fn remote_chain_transaction_stop(
		&self,
		request: RemoteChainTransactionStopRequest,
	) -> Result<(), RuntimeFailure> {
		let method = "remote_chain_transaction_stop";
		let sequence = transaction_operation_sequence(&request.operation_id).ok_or_else(|| {
			RuntimeFailure::host_failure(method, "unknown transaction operation id")
		})?;
		let operation = self
			.with_transaction_operations(|operations| operations.remove(&sequence))
			.ok_or_else(|| {
				RuntimeFailure::host_failure(method, "unknown transaction operation id")
			})?;
		if operation.genesis_hash != request.genesis_hash {
			self.with_transaction_operations(|operations| operations.insert(sequence, operation));
			return Err(RuntimeFailure::host_failure(
				method,
				"transaction operation belongs to a different chain",
			));
		}
		let connection = match self.connection_for(method, &request.genesis_hash).await {
			Ok(connection) => connection,
			Err(err) => {
				self.with_transaction_operations(|operations| {
					operations.insert(sequence, operation)
				});
				return Err(err);
			},
		};
		match connection.methods.transaction_v1_stop(&operation.provider_operation_id).await {
			Ok(()) => Ok(()),
			Err(err) if transaction_operation_already_finished(&err) => Ok(()),
			Err(err) => {
				self.with_transaction_operations(|operations| {
					operations.insert(sequence, operation)
				});
				Err(rpc_failure(method, err))
			},
		}
	}

	/// Runs `f` on the broadcast operations this scope started, and no other scope's.
	fn with_transaction_operations<R>(
		&self,
		f: impl FnOnce(&mut ScopeTransactionOperations) -> R,
	) -> R {
		self.with_scope_group(&self.transaction_operations, BTreeMap::is_empty, f)
	}

	/// Runs `f` on the follow intents this scope registered, and no other scope's.
	fn with_follow_intents<R>(&self, f: impl FnOnce(&mut ScopeFollowIntents) -> R) -> R {
		self.with_scope_group(&self.follow_intents, HashMap::is_empty, f)
	}

	/// Runs `f` on this scope's group in `groups`, dropping the group once it is empty.
	///
	/// A released scope gets a throwaway group, so a call that finishes after
	/// `release_scope` cannot bring the scope's entries back. The connections
	/// lock is held throughout, and `release_scope` removes the scope from
	/// `connections` before it touches `groups`, so the check cannot race it.
	fn with_scope_group<Group: Default, R>(
		&self,
		groups: &Mutex<HashMap<ConnectionScope, Group>>,
		is_empty: impl Fn(&Group) -> bool,
		f: impl FnOnce(&mut Group) -> R,
	) -> R {
		let connections = self.connections.lock().unwrap();
		if !connections.contains_key(&self.scope) {
			return f(&mut Group::default());
		}
		let mut groups = groups.lock().unwrap();
		let group = groups.entry(self.scope).or_default();
		let result = f(group);
		if is_empty(group) {
			groups.remove(&self.scope);
		}
		result
	}

	/// Records a follow's shared setup unless this scope has already been
	/// released.
	///
	/// Holds the connections lock across the check and the insert for the same
	/// reason [`Self::with_scope_group`] does: `release_scope` leaves
	/// `connections` before it sweeps `follow_starts`, so a writer that sees
	/// the scope cannot have its entry swept afterwards, and one that does not
	/// see it never inserts. Without this the setup outlives the scope, and
	/// `core_instance` never repeats, so nothing asks for that key again.
	fn record_follow_start(&self, key: (Vec<u8>, String), start: LocalFollowStart) {
		let connections = self.connections.lock().unwrap();
		if !connections.contains_key(&self.scope) {
			return;
		}
		self.follow_starts.lock().insert(key, start);
	}

	/// Genesis-pinned Subxt client for the chain identified by `genesis_hash`.
	/// The cached unit is the underlying Subxt connection bundle, not just
	/// the cheap client handle.
	///
	/// Core scope only. The Subxt backend spawns a driver task holding a
	/// `HostRpcClient` handle that nothing aborts, so a connection carrying one
	/// never reaches a zero handle count and `release_scope` cannot close its
	/// socket. The product-facing capability uses `remote_chain_*` instead,
	/// which goes through `connection.methods` and spawns no such task.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.online_client"))]
	pub async fn online_client(
		&self,
		genesis_hash: &[u8],
	) -> Result<OnlineClient<SubstrateConfig>, RuntimeFailure> {
		Ok(self.subxt_connection(genesis_hash).await?.client)
	}

	/// Raw JSON-RPC client for the chain identified by `genesis_hash`.
	pub async fn rpc_client(
		&self,
		method: &'static str,
		genesis_hash: &[u8],
	) -> Result<HostRpcClient, RuntimeFailure> {
		Ok(self.connection_for(method, genesis_hash).await?.rpc_client.clone())
	}

	/// Legacy-RPC Subxt client for the chain identified by `genesis_hash`,
	/// for reads at blocks a chainHead follow no longer pins.
	///
	/// Core scope only, for the same reason as [`Self::online_client`].
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.legacy_connection"))]
	pub async fn legacy_connection(
		&self,
		genesis_hash: &[u8],
	) -> Result<LegacyConnection, RuntimeFailure> {
		self.connection_for(LEGACY_CONNECTION, genesis_hash)
			.await?
			.legacy_connection()
			.await
	}

	async fn subxt_connection(
		&self,
		genesis_hash: &[u8],
	) -> Result<SubxtConnection, RuntimeFailure> {
		debug_assert!(
			matches!(self.scope, ConnectionScope::Core),
			"the Subxt backend's driver task defeats release_scope on a product scope",
		);
		let connection = self.connection_for("subxt_connection", genesis_hash).await?;
		connection.subxt_connection().await
	}

	#[instrument(skip_all, fields(runtime.method = "chain_runtime.connection_for", method = method))]
	async fn connection_for(
		&self,
		method: &'static str,
		genesis_hash: &[u8],
	) -> Result<Arc<ChainConnection>, RuntimeFailure> {
		let key = self.connection_key(genesis_hash);
		let setup = {
			let mut scopes = self.connections.lock().unwrap();
			let connections = scopes.get_mut(&self.scope).ok_or_else(|| scope_released(method))?;
			match connections.get(&key) {
				Some(ConnectionEntry::Ready(connection)) if !connection.is_closed() => {
					return Ok(connection.clone());
				},
				Some(ConnectionEntry::Connecting { setup, .. }) => setup.clone(),
				_ => {
					let provider = self.provider.clone();
					let spawner = self.spawner.clone();
					let timer = self.timer.clone();
					let registry = Arc::downgrade(&self.connections);
					let scope = self.scope;
					let ended_follows = self.ended_follows.clone();
					let setup_key = key.clone();
					let genesis_hash = genesis_hash.to_owned();
					let runtime_calls = self.runtime_calls.clone();
					let identity = Arc::new(());
					let setup_identity = identity.clone();
					let setup: ConnectionSetup = async move {
						let result = provider.connect(genesis_hash.clone()).await.map(|rpc| {
							ChainConnection::new(
								rpc,
								spawner,
								timer,
								genesis_hash,
								runtime_calls,
								scope,
								ended_follows,
							)
						});
						let registry =
							registry.upgrade().ok_or_else(|| scope_released(CONNECT_METHOD))?;
						let mut scopes = registry.lock().unwrap();
						let connections =
							scopes.get_mut(&scope).ok_or_else(|| scope_released(CONNECT_METHOD))?;
						let Some(ConnectionEntry::Connecting { identity, .. }) =
							connections.get(&setup_key)
						else {
							return Err(connection_superseded());
						};
						if !Arc::ptr_eq(identity, &setup_identity) {
							return Err(connection_superseded());
						}
						match &result {
							Ok(connection) => {
								connections
									.insert(setup_key, ConnectionEntry::Ready(connection.clone()));
							},
							Err(_) => {
								connections.remove(&setup_key);
							},
						}
						result
					}
					.boxed()
					.shared();
					connections.insert(
						key,
						ConnectionEntry::Connecting { identity, setup: setup.clone() },
					);
					setup
				},
			}
		};

		setup.await.map_err(|failure| failure.reclassify(method))
	}

	#[instrument(skip_all, fields(runtime.method = "chain_runtime.start_follow"))]
	async fn start_follow(
		&self,
		local_follow_id: String,
		request: RemoteChainHeadFollowRequest,
		sender: mpsc::UnboundedSender<Result<RemoteChainHeadFollowItem, RuntimeFailure>>,
		cancelled: Arc<AtomicBool>,
	) -> Result<(), RuntimeFailure> {
		if cancelled.load(Ordering::SeqCst) {
			return Err(follow_closed());
		}
		let connection = self.connection_for(FOLLOW_METHOD, &request.genesis_hash).await?;
		if cancelled.load(Ordering::SeqCst) {
			return Err(follow_closed());
		}
		// Record this subscriber's sender before kicking off (or joining) the
		// single-flight setup so events route to it regardless of which caller
		// wins the setup.
		connection.register_follow_intent(
			&local_follow_id,
			request.with_runtime,
			sender,
			cancelled,
		);
		if connection.follow_cancelled_or_missing(&local_follow_id) {
			connection.unfollow(&local_follow_id);
			return Err(follow_closed());
		}
		connection.ensure_remote_follow(local_follow_id, request.with_runtime).await?;
		Ok(())
	}

	#[instrument(skip_all, fields(runtime.method = "chain_runtime.ensure_follow_context", method = method))]
	async fn ensure_follow_context(
		&self,
		method: &'static str,
		connection: &Arc<ChainConnection>,
		local_follow_id: String,
		with_runtime: bool,
	) -> Result<String, RuntimeFailure> {
		// The connection owns the sticky alias-to-transport binding, so it
		// resolves first and always sees the alias the caller asked for. It can
		// only answer once the follow exists, so a miss falls back to this
		// chain's pending intent, waits for that setup, and lets the connection
		// bind the alias afterwards.
		let local_follow_id = match connection.resolve_local_follow_id(&local_follow_id) {
			Some(resolved) => resolved,
			None => {
				let pending_id = self.resolve_local_follow_id(
					method,
					&connection.genesis_hash,
					&local_follow_id,
				)?;
				let follow_start = self
					.follow_starts
					.lock()
					.get(&(connection.genesis_hash.clone(), pending_id.clone()))
					.cloned();
				if let Some(start) = follow_start {
					start.await.map_err(|failure| failure.reclassify(method))?;
				}
				connection
					.resolve_local_follow_id(&local_follow_id)
					.ok_or_else(|| self.unknown_or_lost_follow(method, &local_follow_id))?
			},
		};
		let remote_follow_id =
			connection.require_remote_follow(method, local_follow_id.clone()).await?;
		connection.note_follow_call(&local_follow_id);
		if with_runtime && !connection.follow_with_runtime(&local_follow_id) {
			return Err(RuntimeFailure::host_failure(
				method,
				"follow subscription was created without runtime metadata",
			));
		}
		Ok(remote_follow_id)
	}

	/// Resolve an exact follow id or the Product SDK's provider-local alias.
	///
	/// Alias candidates stay inside the requesting `cN:` product namespace.
	/// A unique pending intent is eligible; multiple candidates fail closed.
	fn resolve_local_follow_id(
		&self,
		method: &'static str,
		genesis_hash: &[u8],
		requested_follow_id: &str,
	) -> Result<String, RuntimeFailure> {
		let key = self.connection_key(genesis_hash);
		let intents = self.follow_intents.lock().unwrap();
		let intents = intents.get(&self.scope).and_then(|chains| chains.get(&key));
		if intents.is_some_and(|intents| intents.contains(requested_follow_id)) {
			return Ok(requested_follow_id.to_string());
		}

		let requested_namespace = follow_product_namespace(requested_follow_id);
		if let Some(intents) = intents &&
			(requested_namespace.is_some() || !requested_follow_id.contains(':'))
		{
			let mut candidates = intents.iter().filter(|intent| {
				requested_namespace
					.is_none_or(|namespace| follow_product_namespace(intent) == Some(namespace))
			});
			if let Some(candidate) = candidates.next() &&
				candidates.next().is_none()
			{
				return Ok(candidate.clone());
			}
		}
		Err(self.unknown_or_lost_follow(method, requested_follow_id))
	}

	/// What to tell a caller whose follow id no live follow owns: why it ended
	/// if this runtime still remembers it, and otherwise that it was never
	/// issued. The two are worth separating because only the first is
	/// actionable, and they are indistinguishable without the record.
	fn unknown_or_lost_follow(
		&self,
		method: &'static str,
		requested_follow_id: &str,
	) -> RuntimeFailure {
		match lookup_lost_follow(&self.ended_follows, self.scope, requested_follow_id) {
			Some(loss) => follow_lost(method, requested_follow_id, loss),
			None => RuntimeFailure::host_failure(
				method,
				format!("unknown follow subscription id {requested_follow_id:?}"),
			),
		}
	}

	fn register_follow_intent(&self, genesis_hash: &[u8], local_follow_id: &str) {
		let key = self.connection_key(genesis_hash);
		self.with_follow_intents(|chains| {
			chains.entry(key).or_default().insert(local_follow_id.to_string());
		});
	}

	fn remove_follow_intent(&self, genesis_hash: &[u8], local_follow_id: &str) {
		let key = self.connection_key(genesis_hash);
		self.with_follow_intents(|chains| {
			let emptied = chains.get_mut(&key).is_some_and(|chain_intents| {
				chain_intents.remove(local_follow_id);
				chain_intents.is_empty()
			});
			if emptied {
				chains.remove(&key);
			}
		});
	}

	#[instrument(skip_all, fields(runtime.method = "chain_runtime.cleanup_follow"))]
	fn cleanup_follow(&self, genesis_hash: &[u8], local_follow_id: &str) {
		self.remove_follow_intent(genesis_hash, local_follow_id);
		self.follow_starts
			.lock()
			.remove(&(genesis_hash.to_vec(), local_follow_id.to_string()));
		let key = self.connection_key(genesis_hash);
		let Some(connection) = self
			.connections
			.lock()
			.unwrap()
			.get(&self.scope)
			.and_then(|connections| connections.get(&key))
			.and_then(ConnectionEntry::ready)
			.cloned()
		else {
			return;
		};
		connection.unfollow(local_follow_id);
	}
}

enum ConnectionEntry {
	Connecting { identity: Arc<()>, setup: ConnectionSetup },
	Ready(Arc<ChainConnection>),
}

impl ConnectionEntry {
	fn ready(&self) -> Option<&Arc<ChainConnection>> {
		match self {
			Self::Connecting { .. } => None,
			Self::Ready(connection) => Some(connection),
		}
	}
}

struct ChainConnection {
	/// The handle of this connection on the host socket. Every clone of it
	/// counts, including the ones inside `runtime_call_client` and `methods`,
	/// so the socket closes once every holder is gone.
	rpc_client: HostRpcClient,
	/// The same client, with the runtime calls every connection to the chain
	/// makes alike answered from the shared cache.
	runtime_call_client: RuntimeCallClient,
	/// Typed `chainHead_v1` calls over `runtime_call_client`.
	methods: ChainHeadRpcMethods<TruapiRpcConfig>,
	spawner: Spawner,
	/// Paces the Subxt backend's follow retries.
	timer: Arc<dyn Timer>,
	/// Host-configured genesis hash this connection was opened for; pins the
	/// cached Subxt bundle's chain config.
	genesis_hash: Vec<u8>,
	/// Live follows on this connection, keyed by local follow id. Entries are
	/// recorded on intent and gain a remote id once the node acknowledges one,
	/// so a count of these is not a count of follows the node knows about.
	follows: Mutex<HashMap<String, FollowState>>,
	/// Follow setup in flight, so concurrent callers for one local id await a
	/// single remote subscription rather than each opening one.
	follow_setups: Mutex<HashMap<String, FollowSetup>>,
	/// Cached Subxt bundle setup tagged with its generation, so invalidation
	/// (on setup failure or backend-driver exit) can never evict a newer
	/// rebuild.
	subxt_connection_setup: Mutex<Option<(u64, SubxtConnectionSetup)>>,
	subxt_connection_generation: AtomicU64,
	/// Config shared by every Subxt client on this connection, so they share
	/// one metadata cache.
	chain_config: OnceLock<SubstrateConfig>,
	legacy_connection: Mutex<Option<LegacyConnection>>,
	/// Whether this connection has already reported itself at the budget.
	/// dotNS opens and drops a follow per lookup, so a connection sitting at
	/// the budget would otherwise repeat the line on every lookup, into a
	/// device log the level filter cannot quiet.
	budget_reported: AtomicBool,
	/// Scope this connection was opened for, so a follow it loses is recorded
	/// against the same scope that will come looking for it.
	scope: ConnectionScope,
	/// The runtime's record of ended follows. Written here, where a follow
	/// actually ends, so a loss is known before anyone asks about it.
	ended_follows: EndedFollows,
}

impl ChainConnection {
	fn new(
		rpc: Arc<dyn JsonRpcConnection>,
		spawner: Spawner,
		timer: Arc<dyn Timer>,
		genesis_hash: Vec<u8>,
		runtime_calls: RuntimeCallCache,
		scope: ConnectionScope,
		ended_follows: EndedFollows,
	) -> Arc<Self> {
		let rpc_client = HostRpcClient::new(rpc, spawner.clone());
		let runtime_call_client =
			RuntimeCallClient::new(rpc_client.clone(), genesis_hash.clone(), runtime_calls);
		let methods = ChainHeadRpcMethods::new(RpcClient::new(runtime_call_client.clone()));
		Arc::new(Self {
			rpc_client,
			runtime_call_client,
			methods,
			spawner,
			timer,
			genesis_hash,
			follows: Mutex::new(HashMap::new()),
			follow_setups: Mutex::new(HashMap::new()),
			subxt_connection_setup: Mutex::new(None),
			subxt_connection_generation: AtomicU64::new(0),
			chain_config: OnceLock::new(),
			legacy_connection: Mutex::new(None),
			budget_reported: AtomicBool::new(false),
			scope,
			ended_follows,
		})
	}

	fn is_closed(&self) -> bool {
		self.rpc_client.is_closed()
	}

	/// Lazily build (single-flight) and cache the Subxt bundle over this
	/// connection's transport. The chain config pins the host-configured
	/// genesis hash, so Subxt never reads a provider-echoed one, and the
	/// backend follow started here is shared by every user of this connection.
	async fn subxt_connection(self: &Arc<Self>) -> Result<SubxtConnection, RuntimeFailure> {
		let (generation, setup) = {
			let mut slot = self.subxt_connection_setup.lock().unwrap();
			if let Some(existing) = slot.clone() {
				existing
			} else {
				let generation =
					self.subxt_connection_generation.fetch_add(1, Ordering::Relaxed) + 1;
				let connection = self.clone();
				let setup: SubxtConnectionSetup =
					async move { connection.build_subxt_connection(generation).await }
						.boxed()
						.shared();
				*slot = Some((generation, setup.clone()));
				(generation, setup)
			}
		};
		let result = setup.await;
		// On failure, drop the cached setup so a later call can retry.
		if result.is_err() {
			self.invalidate_subxt_connection(generation);
		}
		result
	}

	/// Drop the cached Subxt setup if it still belongs to `generation`;
	/// stale invalidations (an old driver exiting after a rebuild) are
	/// ignored.
	fn invalidate_subxt_connection(&self, generation: u64) {
		let mut slot = self.subxt_connection_setup.lock().unwrap();
		if slot.as_ref().is_some_and(|(cached, _)| *cached == generation) {
			*slot = None;
		}
	}

	/// Body of the single-flight Subxt setup: start the chainHead backend,
	/// drive it on the connection's spawner, and build the client with the
	/// config-pinned genesis hash.
	async fn build_subxt_connection(
		self: Arc<Self>,
		generation: u64,
	) -> Result<SubxtConnection, RuntimeFailure> {
		const METHOD: &str = "subxt_connection";
		let config = self.chain_config(METHOD)?;
		// Subxt waits only for a call to finish, never retrying an inaccessible
		// one, so it must not wait on a download another caller may give up.
		let rpc = FollowRetryingRpcClient::new(
			self.runtime_call_client.without_waiting(),
			self.timer.clone(),
		);
		let follow = rpc.report();
		let (backend, mut driver) =
			ChainHeadBackend::<SubstrateConfig>::builder().build(RpcClient::new(rpc));
		// The pump holds only a weak handle so a torn-down connection is not
		// kept alive by its own driver task.
		let pump_connection = Arc::downgrade(&self);
		self.spawner.spawn(async move {
			while let Some(result) = driver.next().await {
				if let Err(error) = result {
					warn!(target: "subxt", "chainHead backend error={error}");
				}
			}
			// The backend can make no further progress; drop the cached
			// Subxt bundle so the next caller rebuilds instead of hitting
			// a permanently dead backend.
			if let Some(connection) = pump_connection.upgrade() {
				connection.invalidate_subxt_connection(generation);
			}
		});
		let backend = Arc::new(backend);
		let client = OnlineClient::from_backend_with_config(config, backend.clone())
			.await
			.map_err(|error| RuntimeFailure::host_failure(METHOD, error.to_string()))?;
		Ok(SubxtConnection { client, follow, connection: Arc::downgrade(&self) })
	}

	/// The config every Subxt client on this connection is built with. It
	/// pins the host-configured genesis hash, so Subxt never reads a
	/// provider-echoed one.
	fn chain_config(&self, method: &'static str) -> Result<SubstrateConfig, RuntimeFailure> {
		if let Some(config) = self.chain_config.get() {
			return Ok(config.clone());
		}
		let genesis_hash: [u8; 32] = self.genesis_hash.as_slice().try_into().map_err(|_| {
			RuntimeFailure::host_failure(
				method,
				format!("expected 32-byte genesis hash, got {}", self.genesis_hash.len()),
			)
		})?;
		Ok(self
			.chain_config
			.get_or_init(|| {
				SubstrateConfigBuilder::new().set_genesis_hash(H256(genesis_hash)).build()
			})
			.clone())
	}

	/// Lazily build and cache the legacy Subxt client. Building it sends no
	/// request and starts no background task, so concurrent first callers
	/// may each build one; the first one stored is kept.
	async fn legacy_connection(&self) -> Result<LegacyConnection, RuntimeFailure> {
		if let Some(existing) = self.legacy_connection.lock().unwrap().clone() {
			return Ok(existing);
		}
		let config = self.chain_config(LEGACY_CONNECTION)?;
		let rpc = RpcClient::new(self.rpc_client.clone());
		let backend = Arc::new(LegacyBackend::builder().build(rpc.clone()));
		let client = OnlineClient::from_backend_with_config(config, backend.clone())
			.await
			.map_err(|error| RuntimeFailure::host_failure(LEGACY_CONNECTION, error.to_string()))?;
		let methods = LegacyRpcMethods::new(rpc.clone());
		let built = LegacyConnection { client, backend, methods, raw_rpc_client: rpc };
		Ok(self.legacy_connection.lock().unwrap().get_or_insert(built).clone())
	}

	/// Whether the already-resolved follow was registered with runtime metadata.
	fn follow_with_runtime(&self, local_follow_id: &str) -> bool {
		self.follows
			.lock()
			.unwrap()
			.get(local_follow_id)
			.is_some_and(|follow| follow.with_runtime)
	}

	/// Resolve the product-visible follow id to the transport-owned follow.
	///
	/// Product adapters assign their own ids (for example `follow_0`) after
	/// starting a subscription, while the dispatcher keys that subscription
	/// by its transport request id. The first follow-bound request claims the
	/// sole unaliased follow for this chain; later requests reuse that alias.
	fn resolve_local_follow_id(&self, requested_id: &str) -> Option<String> {
		let mut follows = self.follows.lock().unwrap();

		if follows.contains_key(requested_id) {
			return Some(requested_id.to_string());
		}

		if let Some((local_follow_id, _)) = follows
			.iter()
			.find(|(_, follow)| follow.client_subscription_id.as_deref() == Some(requested_id))
		{
			return Some(local_follow_id.clone());
		}

		// What follows is speculative: it adopts the sole unaliased follow for
		// an id this connection has not seen, which is how a product-minted
		// alias first binds. An id the runtime knows has ended is not an
		// unseen id, so leave it to the caller's fail-closed path rather than
		// silently answering it from an unrelated follow.
		if self.follow_is_known_lost(requested_id) {
			return None;
		}

		let unaliased: Vec<_> = follows
			.iter()
			.filter(|(_, follow)| follow.client_subscription_id.is_none())
			.map(|(local_follow_id, _)| local_follow_id.clone())
			.take(2)
			.collect();

		let [local_follow_id] = unaliased.as_slice() else {
			return None;
		};

		follows
			.get_mut(local_follow_id)
			.expect("unaliased follow still exists")
			.client_subscription_id = Some(requested_id.to_string());

		Some(local_follow_id.clone())
	}

	/// Record a chainHead call made on `local_follow_id`, for the budget line.
	fn note_follow_call(&self, local_follow_id: &str) {
		if let Some(follow) = self.follows.lock().unwrap().get_mut(local_follow_id) {
			follow.activity.saw_call();
		}
	}

	fn remote_follow_id(&self, local_follow_id: &str) -> Option<String> {
		self.follows
			.lock()
			.unwrap()
			.get(local_follow_id)
			.and_then(|follow| follow.remote_subscription_id.clone())
	}

	/// Record intent to follow `local_follow_id`, attaching `sender` for a
	/// follow subscriber. Idempotent: an existing follow keeps its
	/// `with_runtime` flag and remote id; only the sender is (re)attached.
	fn register_follow_intent(
		&self,
		local_follow_id: &str,
		with_runtime: bool,
		sender: mpsc::UnboundedSender<Result<RemoteChainHeadFollowItem, RuntimeFailure>>,
		cancelled: Arc<AtomicBool>,
	) {
		let mut follows = self.follows.lock().unwrap();
		match follows.get_mut(local_follow_id) {
			Some(follow) => {
				follow.sender = sender;
				follow.cancelled = cancelled;
			},
			None => {
				follows.insert(
					local_follow_id.to_string(),
					FollowState {
						with_runtime,
						client_subscription_id: None,
						remote_subscription_id: None,
						abort: None,
						sender,
						cancelled,
						activity: FollowActivity::new(),
					},
				);
			},
		}
	}

	fn follow_cancelled_or_missing(&self, local_follow_id: &str) -> bool {
		match self.follows.lock().unwrap().get(local_follow_id) {
			Some(follow) => follow.cancelled.load(Ordering::SeqCst),
			None => true,
		}
	}

	/// Issue `chainHead_v1_follow` exactly once per local follow id and return
	/// the remote subscription id. Concurrent callers for the same id share
	/// one in-flight setup instead of each opening a duplicate remote
	/// subscription that would then leak.
	#[instrument(skip_all, fields(runtime.method = "chain_connection.ensure_remote_follow"))]
	async fn ensure_remote_follow(
		self: &Arc<Self>,
		local_follow_id: String,
		with_runtime: bool,
	) -> Result<String, RuntimeFailure> {
		if self.follow_cancelled_or_missing(&local_follow_id) {
			return Err(follow_closed());
		}
		if let Some(remote_follow_id) = self.remote_follow_id(&local_follow_id) {
			return Ok(remote_follow_id);
		}

		let setup = {
			let mut setups = self.follow_setups.lock().unwrap();
			if let Some(existing) = setups.get(&local_follow_id) {
				existing.clone()
			} else {
				let connection = self.clone();
				let id = local_follow_id.clone();
				let setup: FollowSetup =
					async move { connection.run_follow_setup(id, with_runtime).await }
						.boxed()
						.shared();
				setups.insert(local_follow_id.clone(), setup.clone());
				setup
			}
		};

		let result = setup.await;
		// On failure, drop the cached setup so a later re-subscribe can retry.
		// On success the established follow short-circuits the fast path above,
		// and `remove_follow` clears the entry at teardown.
		if result.is_err() {
			self.follow_setups.lock().unwrap().remove(&local_follow_id);
		}
		result
	}

	/// Return the remote follow id for an already-created local follow.
	///
	/// Follow-bound request methods must not create remote follows themselves:
	/// the local follow stream owns cleanup, so only `follow_head_subscribe`
	/// may establish the remote subscription.
	#[instrument(skip_all, fields(runtime.method = "chain_connection.require_remote_follow"))]
	async fn require_remote_follow(
		self: &Arc<Self>,
		method: &'static str,
		local_follow_id: String,
	) -> Result<String, RuntimeFailure> {
		if let Some(remote_follow_id) = self.remote_follow_id(&local_follow_id) {
			return Ok(remote_follow_id);
		}

		let setup = self.follow_setups.lock().unwrap().get(&local_follow_id).cloned();

		match setup {
			Some(setup) => setup.await.map_err(|failure| failure.reclassify(method)),
			None => Err(RuntimeFailure::host_failure(
				method,
				format!("follow subscription {local_follow_id:?} is not established"),
			)),
		}
	}

	/// Body of the single-flight follow setup: ensure the `FollowState`
	/// exists, issue `chainHead_v1_follow`, and record the remote id.
	#[instrument(skip_all, fields(runtime.method = "chain_connection.run_follow_setup"))]
	async fn run_follow_setup(
		self: Arc<Self>,
		local_follow_id: String,
		with_runtime: bool,
	) -> Result<String, RuntimeFailure> {
		if self.follow_cancelled_or_missing(&local_follow_id) {
			self.remove_follow(&local_follow_id);
			return Err(follow_closed());
		}
		let mut follow = self.methods.chainhead_v1_follow(with_runtime).await.map_err(|err| {
			self.remove_follow(&local_follow_id);
			rpc_failure(FOLLOW_METHOD, err).on_chain(&self.genesis_hash)
		})?;
		let remote_follow_id = follow
			.subscription_id()
			.ok_or_else(|| {
				self.remove_follow(&local_follow_id);
				RuntimeFailure::host_failure(FOLLOW_METHOD, "missing follow subscription id")
			})?
			.to_string();
		if self.follow_cancelled_or_missing(&local_follow_id) {
			self.remove_follow(&local_follow_id);
			drop(follow);
			return Err(follow_closed());
		}

		let (abort, abort_registration) = AbortHandle::new_pair();
		let connection = self.clone();
		let pump_follow_id = local_follow_id.clone();
		let pump = async move {
			while let Some(item) = follow.next().await {
				match item {
					Ok(event) => match map_follow_event(event) {
						Ok(item) => {
							let is_stop = matches!(item, RemoteChainHeadFollowItem::Stop);
							connection.deliver_follow_event(&pump_follow_id, item);
							if is_stop {
								break;
							}
						},
						Err(failure) => {
							connection.interrupt_follow(
								&pump_follow_id,
								failure,
								FollowLoss::Failed,
							);
							break;
						},
					},
					Err(error) => {
						let loss = if is_transport_loss(&error) {
							FollowLoss::ConnectionClosed
						} else {
							FollowLoss::Failed
						};
						connection.interrupt_follow(
							&pump_follow_id,
							rpc_failure(FOLLOW_METHOD, error).on_chain(&connection.genesis_hash),
							loss,
						);
						break;
					},
				}
			}
			connection.remove_follow_without_abort(&pump_follow_id);
		};

		if !self.attach_remote_follow(&local_follow_id, remote_follow_id.clone(), abort) {
			return Err(follow_closed());
		}

		self.spawner.spawn(Abortable::new(pump, abort_registration).map(|_| ()));
		Ok(remote_follow_id)
	}

	fn attach_remote_follow(
		&self,
		local_follow_id: &str,
		remote_follow_id: String,
		abort: AbortHandle,
	) -> bool {
		let mut follows = self.follows.lock().unwrap();
		let Some(follow) = follows.get_mut(local_follow_id) else {
			return false;
		};
		follow.remote_subscription_id = Some(remote_follow_id);
		follow.abort = Some(abort);
		let holding = Self::follows_holding_a_slot(&follows);
		let at_budget = holding.len() >= FOLLOWS_A_NODE_BUDGETS;
		// Reported on the way into the budget and again only after the
		// connection has come back out of it, so a connection that sits there
		// says so once rather than on every follow that renews the condition.
		let already_reported = self.budget_reported.swap(at_budget, Ordering::Relaxed);
		let report = at_budget && !already_reported;
		let line = report.then(|| {
			Self::budget_line(
				&encode_hex(&self.genesis_hash),
				&holding,
				&follows,
				SystemTime::now(),
			)
		});
		drop(follows);
		if let Some(line) = line {
			// The refusal this predicts surfaces far from here, on whichever
			// call happens to open the next follow, and says nothing about
			// follows when it does. The target addresses it to the host, so it
			// arrives whatever the level is, and hosts run at `ERROR`.
			warn!(target: crate::markers::CHAIN_FOLLOW_BUDGET_REACHED.target, "{line}");
		}
		true
	}

	/// What the budget marker carries: the chain, how many follows hold a slot,
	/// and each of those follows with its activity.
	///
	/// Every follow is named, because a leak shows as one follow open far longer
	/// than the rest, and a line that named only some of them could leave that
	/// one out. The follows read in the order [`Self::follows_holding_a_slot`]
	/// gives them, which is by id rather than by age.
	fn budget_line(
		genesis: &str,
		holding: &[String],
		follows: &HashMap<String, FollowState>,
		now: SystemTime,
	) -> String {
		let held = holding.len();
		let head = format!(
			"this connection to chain {genesis} holds {held} of {FOLLOWS_A_NODE_BUDGETS} follows, the next is refused: "
		);
		let entries = holding
			.iter()
			.map(|follow_id| {
				let activity = follows
					.get(follow_id)
					.map(|follow| follow.describe_activity(now))
					.unwrap_or_default();
				(Self::occupant_name(follow_id), activity)
			})
			.collect::<Vec<_>>();
		let room = crate::logging::HOST_SINK_DETAIL_MAX_CHARS
			.saturating_sub(head.chars().count() + held.saturating_sub(1) * 2);
		let occupants = Self::fit_entries(&entries, room).join(", ");
		format!("{head}{occupants}")
	}

	/// Each `(name, activity)` as `name activity`, all of them inside `room`
	/// characters between them.
	///
	/// Nothing is cut when everything fits. Otherwise the shortest entries
	/// are kept whole and the longest share what is left equally. A cut
	/// entry loses the end of its name and keeps its activity, which is what
	/// says whether a follow is stale or its receiver gone; only an entry
	/// whose activity alone outruns its share is cut at its end.
	fn fit_entries(entries: &[(String, String)], room: usize) -> Vec<String> {
		let whole = entries
			.iter()
			.map(|(name, activity)| format!("{name} {activity}"))
			.collect::<Vec<_>>();
		let mut by_length = (0..whole.len()).collect::<Vec<_>>();
		by_length.sort_by_key(|&index| whole[index].chars().count());
		let mut fitted = whole.clone();
		let mut left = room;
		for (placed, &index) in by_length.iter().enumerate() {
			let share = left / (whole.len() - placed);
			let length = whole[index].chars().count();
			if length <= share {
				left = left.saturating_sub(length);
				continue;
			}
			let (name, activity) = &entries[index];
			let name_room = share.saturating_sub(activity.chars().count() + 1);
			fitted[index] = if name_room >= 2 {
				format!("{} {activity}", Self::bound_entry(name, name_room))
			} else {
				Self::bound_entry(&whole[index], share)
			};
			left = left.saturating_sub(fitted[index].chars().count());
		}
		fitted
	}

	/// `entry` within `max_chars`, marking a cut so a name read short is not
	/// mistaken for a name that is short.
	fn bound_entry(entry: &str, max_chars: usize) -> String {
		if entry.chars().count() <= max_chars {
			return entry.to_string();
		}
		let kept = crate::logging::truncate_to_chars(entry, max_chars.saturating_sub(1));
		format!("{kept}\u{2026}")
	}

	/// Local ids of the follows holding a slot in this connection's budget.
	///
	/// A follow is recorded on intent and gains a remote id once the node
	/// acknowledges one, so only the acknowledged ones occupy the node's
	/// budget. Counting intents instead would report a connection as full
	/// while the node still had room.
	fn follows_holding_a_slot(follows: &HashMap<String, FollowState>) -> Vec<String> {
		let mut holding: Vec<String> = follows
			.iter()
			.filter(|(_, follow)| follow.remote_subscription_id.is_some())
			.map(|(local_follow_id, _)| local_follow_id.clone())
			.collect();
		holding.sort();
		holding
	}

	/// Name one follow holding a slot, for a line that leaves the device.
	///
	/// The core's own follow ids are its own words, so one is named by what it
	/// says less the prefix every core follow shares, and the line's own budget
	/// is what bounds its length. A product's follow id is built from the
	/// request id it chose, which is arbitrary text off the wire, so only the
	/// runtime it belongs to is named.
	/// Which products hold how many is the diagnostic; what a product called
	/// its request is not, and is not ours to repeat.
	fn occupant_name(local_follow_id: &str) -> String {
		if let Some(core_follow) = local_follow_id.strip_prefix("truapi:") {
			return core_follow.to_string();
		}
		match local_follow_id.split_once(':') {
			Some((runtime, _)) => format!("{runtime}:product"),
			None => "product".to_string(),
		}
	}

	/// Abort every follow pump on this connection.
	///
	/// A pump holds a strong handle to the connection, so while one runs the
	/// connection cannot drop and the host's socket stays open.
	fn abort_all_follows(&self) {
		let ids: Vec<String> = self.follows.lock().unwrap().keys().cloned().collect();
		for id in ids {
			self.remove_follow(&id);
		}
	}

	fn remove_follow(&self, local_follow_id: &str) {
		self.follow_setups.lock().unwrap().remove(local_follow_id);
		if let Some(mut follow) = self.follows.lock().unwrap().remove(local_follow_id) &&
			let Some(abort) = follow.abort.take()
		{
			abort.abort();
		}
	}

	fn remove_follow_without_abort(&self, local_follow_id: &str) {
		self.follow_setups.lock().unwrap().remove(local_follow_id);
		self.follows.lock().unwrap().remove(local_follow_id);
	}

	fn unfollow(&self, local_follow_id: &str) {
		self.remove_follow(local_follow_id);
	}

	/// Deliver one follow event to the local subscriber; a `Stop` event also
	/// tears the follow down, ending the local stream via sender drop, and is
	/// recorded so a caller still holding the id learns the node ended it.
	/// Cleanup never aborts: the only caller is the pump itself, which the
	/// stored abort handle targets.
	fn deliver_follow_event(&self, local_follow_id: &str, event: RemoteChainHeadFollowItem) {
		let sender = self.follows.lock().unwrap().get_mut(local_follow_id).map(|follow| {
			follow.activity.saw_event();
			follow.sender.clone()
		});
		let is_stop = matches!(event, RemoteChainHeadFollowItem::Stop);
		// Recorded before the subscriber sees the stop, so a caller that reacts
		// to it by calling again already finds the reason.
		if is_stop {
			self.remember_lost_follow(local_follow_id, FollowLoss::StoppedByNode);
		}
		if let Some(sender) = sender {
			let _ = sender.unbounded_send(Ok(event));
		}
		if is_stop {
			self.remove_follow_without_abort(local_follow_id);
		}
	}

	/// End the local follow stream on an abnormal close, delivering `failure`
	/// as its last item before tearing the follow down, and recording `loss`
	/// as why. Cleanup never aborts, same as [`Self::deliver_follow_event`].
	fn interrupt_follow(&self, local_follow_id: &str, failure: RuntimeFailure, loss: FollowLoss) {
		warn!(%failure, follow = local_follow_id, "chain follow interrupted");
		// Recorded before the follow is forgotten, so the runtime can still
		// tell a caller why the id it holds stopped working.
		self.remember_lost_follow(local_follow_id, loss);
		if let Some(sender) = self.follow_sender(local_follow_id) {
			let _ = sender.unbounded_send(Err(failure));
		}
		self.remove_follow_without_abort(local_follow_id);
	}

	/// Record that `local_follow_id` has ended, against this connection's own
	/// scope, so the runtime can answer for the id after the follow is gone.
	/// The alias a product bound to it is recorded as well, since that is the
	/// id such a product goes on calling with.
	fn remember_lost_follow(&self, local_follow_id: &str, loss: FollowLoss) {
		let alias = self
			.follows
			.lock()
			.unwrap()
			.get(local_follow_id)
			.and_then(|follow| follow.client_subscription_id.clone());
		remember_lost_follow(&self.ended_follows, self.scope, local_follow_id, loss);
		if let Some(alias) = alias {
			remember_lost_follow(&self.ended_follows, self.scope, &alias, loss);
		}
	}

	/// Whether this is an id the runtime knows has ended.
	fn follow_is_known_lost(&self, requested_id: &str) -> bool {
		lookup_lost_follow(&self.ended_follows, self.scope, requested_id).is_some()
	}

	/// Handle on the local subscriber of `local_follow_id`, cloned so the
	/// `follows` lock is released before anything is sent through it.
	fn follow_sender(
		&self,
		local_follow_id: &str,
	) -> Option<mpsc::UnboundedSender<Result<RemoteChainHeadFollowItem, RuntimeFailure>>> {
		self.follows
			.lock()
			.unwrap()
			.get(local_follow_id)
			.map(|follow| follow.sender.clone())
	}
}

struct FollowState {
	with_runtime: bool,
	client_subscription_id: Option<String>,
	remote_subscription_id: Option<String>,
	abort: Option<AbortHandle>,
	/// Local subscriber; dropping it (with the follow state) is what ends the
	/// local follow stream.
	sender: mpsc::UnboundedSender<Result<RemoteChainHeadFollowItem, RuntimeFailure>>,
	cancelled: Arc<AtomicBool>,
	activity: FollowActivity,
}

impl FollowState {
	/// What this follow has been doing, for the budget line.
	///
	/// The node sends new blocks to every live follow, so a recent event only
	/// says the follow is alive. The calls made on it say whether anyone still
	/// uses it, so a follow open for minutes with no recent call is the one to
	/// suspect, though a reader that only watches blocks makes no calls
	/// either. A dropped receiver on a follow still here means the cleanup
	/// that should have ended it missed this connection.
	fn describe_activity(&self, now: SystemTime) -> String {
		let FollowActivity { opened_at, last_event_at, calls, last_call_at } = &self.activity;
		let elapsed = |at: &SystemTime| now.duration_since(*at).unwrap_or_default().as_secs();
		let since = |at: &Option<SystemTime>| match at {
			Some(at) => format!("{}s", elapsed(at)),
			None => "never".to_string(),
		};
		let open = elapsed(opened_at);
		let last_event = since(last_event_at);
		let last_call = since(last_call_at);
		// A live receiver is the ordinary case, so only a dropped one is named.
		let receiver = if self.sender.is_closed() { ", dropped" } else { "" };
		format!("open {open}s, event {last_event}, calls {calls}, call {last_call}{receiver}")
	}
}

/// When a follow was opened, when it last delivered an event, and how often
/// and how recently a chainHead call, an unpin included, was made on it.
/// Wall-clock times, since a phone's monotonic clock stops while it sleeps and
/// a follow left open across a screen-off would read as fresh.
struct FollowActivity {
	opened_at: SystemTime,
	last_event_at: Option<SystemTime>,
	calls: u64,
	last_call_at: Option<SystemTime>,
}

impl FollowActivity {
	fn new() -> Self {
		Self { opened_at: SystemTime::now(), last_event_at: None, calls: 0, last_call_at: None }
	}

	fn saw_event(&mut self) {
		self.last_event_at = Some(SystemTime::now());
	}

	fn saw_call(&mut self) {
		self.calls += 1;
		self.last_call_at = Some(SystemTime::now());
	}
}

/// Subscription wrapper that runs an `on_drop` cleanup when the stream is
/// dropped. Used by `remote_chain_head_follow` to send `chainHead_v1_unfollow`
/// when the local follow stream is dropped.
struct ManagedSubscription<T> {
	inner: BoxStream<'static, T>,
	on_drop: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl<T> ManagedSubscription<T> {
	fn new(inner: BoxStream<'static, T>, on_drop: Option<Box<dyn FnOnce() + Send + Sync>>) -> Self {
		Self { inner, on_drop }
	}
}

impl<T> Drop for ManagedSubscription<T> {
	fn drop(&mut self) {
		if let Some(on_drop) = self.on_drop.take() {
			on_drop();
		}
	}
}

impl<T> Stream for ManagedSubscription<T> {
	type Item = T;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.get_mut();
		this.inner.as_mut().poll_next(cx)
	}
}

fn operation_started_result(
	response: subxt_chain::MethodResponse,
) -> Result<OperationStartedResult, RuntimeFailure> {
	match response {
		subxt_chain::MethodResponse::Started(started) => {
			Ok(OperationStartedResult::Started { operation_id: started.operation_id })
		},
		subxt_chain::MethodResponse::LimitReached => Ok(OperationStartedResult::LimitReached),
	}
}

fn map_follow_event(
	event: subxt_chain::FollowEvent<H256>,
) -> Result<RemoteChainHeadFollowItem, RuntimeFailure> {
	match event {
		subxt_chain::FollowEvent::Initialized(event) => {
			Ok(RemoteChainHeadFollowItem::Initialized {
				finalized_block_hashes: event
					.finalized_block_hashes
					.into_iter()
					.map(hash_to_bytes)
					.collect(),
				finalized_block_runtime: event
					.finalized_block_runtime
					.map(map_runtime_event)
					.transpose()?,
			})
		},
		subxt_chain::FollowEvent::NewBlock(event) => Ok(RemoteChainHeadFollowItem::NewBlock {
			block_hash: hash_to_bytes(event.block_hash),
			parent_block_hash: hash_to_bytes(event.parent_block_hash),
			new_runtime: event.new_runtime.map(map_runtime_event).transpose()?,
		}),
		subxt_chain::FollowEvent::BestBlockChanged(event) => {
			Ok(RemoteChainHeadFollowItem::BestBlockChanged {
				best_block_hash: hash_to_bytes(event.best_block_hash),
			})
		},
		subxt_chain::FollowEvent::Finalized(event) => Ok(RemoteChainHeadFollowItem::Finalized {
			finalized_block_hashes: event
				.finalized_block_hashes
				.into_iter()
				.map(hash_to_bytes)
				.collect(),
			pruned_block_hashes: event.pruned_block_hashes.into_iter().map(hash_to_bytes).collect(),
		}),
		subxt_chain::FollowEvent::OperationBodyDone(event) => {
			Ok(RemoteChainHeadFollowItem::OperationBodyDone {
				operation_id: event.operation_id,
				value: event.value.into_iter().map(|bytes| bytes.0).collect(),
			})
		},
		subxt_chain::FollowEvent::OperationCallDone(event) => {
			Ok(RemoteChainHeadFollowItem::OperationCallDone {
				operation_id: event.operation_id,
				output: event.output.0,
			})
		},
		subxt_chain::FollowEvent::OperationStorageItems(event) => {
			Ok(RemoteChainHeadFollowItem::OperationStorageItems {
				operation_id: event.operation_id,
				items: event
					.items
					.into_iter()
					.map(map_storage_result)
					.collect::<Result<Vec<_>, _>>()?,
			})
		},
		subxt_chain::FollowEvent::OperationStorageDone(event) => {
			Ok(RemoteChainHeadFollowItem::OperationStorageDone { operation_id: event.operation_id })
		},
		subxt_chain::FollowEvent::OperationWaitingForContinue(event) => {
			Ok(RemoteChainHeadFollowItem::OperationWaitingForContinue {
				operation_id: event.operation_id,
			})
		},
		subxt_chain::FollowEvent::OperationInaccessible(event) => {
			Ok(RemoteChainHeadFollowItem::OperationInaccessible {
				operation_id: event.operation_id,
			})
		},
		subxt_chain::FollowEvent::OperationError(event) => {
			Ok(RemoteChainHeadFollowItem::OperationError {
				operation_id: event.operation_id,
				error: event.error,
			})
		},
		subxt_chain::FollowEvent::Stop => Ok(RemoteChainHeadFollowItem::Stop),
	}
}

fn map_runtime_event(event: subxt_chain::RuntimeEvent) -> Result<RuntimeType, RuntimeFailure> {
	match event {
		subxt_chain::RuntimeEvent::Valid(event) => {
			let mut apis = event
				.spec
				.apis
				.into_iter()
				.map(|(name, version)| RuntimeApi { name, version })
				.collect::<Vec<_>>();
			apis.sort_by(|left, right| left.name.cmp(&right.name));
			Ok(RuntimeType::Valid(RuntimeSpec {
				spec_name: event.spec.spec_name,
				impl_name: event.spec.impl_name,
				spec_version: event.spec.spec_version,
				impl_version: event.spec.impl_version,
				transaction_version: Some(event.spec.transaction_version),
				apis,
			}))
		},
		subxt_chain::RuntimeEvent::Invalid(event) => {
			Ok(RuntimeType::Invalid { error: event.error })
		},
	}
}

fn map_storage_query_item(item: &StorageQueryItem) -> subxt_chain::StorageQuery<&[u8]> {
	subxt_chain::StorageQuery {
		key: item.key.as_slice(),
		query_type: match item.query_type {
			StorageQueryType::Value => subxt_chain::StorageQueryType::Value,
			StorageQueryType::Hash => subxt_chain::StorageQueryType::Hash,
			StorageQueryType::ClosestDescendantMerkleValue => {
				subxt_chain::StorageQueryType::ClosestDescendantMerkleValue
			},
			StorageQueryType::DescendantsValues => subxt_chain::StorageQueryType::DescendantsValues,
			StorageQueryType::DescendantsHashes => subxt_chain::StorageQueryType::DescendantsHashes,
		},
	}
}

fn map_storage_result(
	item: subxt_chain::StorageResult,
) -> Result<StorageResultItem, RuntimeFailure> {
	let mut result = StorageResultItem {
		key: item.key.0,
		value: None,
		hash: None,
		closest_descendant_merkle_value: None,
	};
	match item.result {
		subxt_chain::StorageResultType::Value(value) => result.value = Some(value.0),
		subxt_chain::StorageResultType::Hash(hash) => result.hash = Some(hash.0),
		subxt_chain::StorageResultType::ClosestDescendantMerkleValue(value) => {
			result.closest_descendant_merkle_value = Some(value.0);
		},
	}
	Ok(result)
}

fn hash_from_bytes(method: &'static str, bytes: &[u8]) -> Result<H256, RuntimeFailure> {
	if bytes.len() != 32 {
		return Err(RuntimeFailure::host_failure(
			method,
			format!("expected 32-byte hash, got {}", bytes.len()),
		));
	}
	Ok(H256::from_slice(bytes))
}

fn hash_to_bytes(hash: H256) -> Vec<u8> {
	hash.as_bytes().to_vec()
}

fn transaction_operation_already_finished(error: &SubxtRpcError) -> bool {
	let reason = error.to_string();
	reason.contains("Invalid operation id") && reason.contains("-32602")
}

fn scope_released(method: &'static str) -> RuntimeFailure {
	RuntimeFailure::unavailable_with_reason(method, "chain connections were released")
}

fn connection_superseded() -> RuntimeFailure {
	RuntimeFailure::unavailable_with_reason(CONNECT_METHOD, "connection setup was superseded")
}

fn follow_closed() -> RuntimeFailure {
	RuntimeFailure::unavailable_with_reason(FOLLOW_METHOD, "follow subscription was closed")
}

/// A follow this scope opened that has since ended, named with why. The id is
/// spent and the caller has to open a new follow. Classed `Unavailable` for the
/// runtime's own failure log; a product reads only the reason.
fn follow_lost(method: &'static str, local_follow_id: &str, loss: FollowLoss) -> RuntimeFailure {
	RuntimeFailure::unavailable_with_reason(
		method,
		format!("follow subscription {local_follow_id:?} {}", loss.clause()),
	)
}

/// Record that a live scope lost a follow, dropping the oldest past the cap.
/// An id recorded again replaces its earlier entry, so the cap counts distinct
/// ids and the newest reason is the one reported.
fn remember_lost_follow(
	ended: &Mutex<HashMap<ConnectionScope, VecDeque<(String, FollowLoss)>>>,
	scope: ConnectionScope,
	local_follow_id: &str,
	loss: FollowLoss,
) {
	let mut ended = ended.lock().unwrap();
	// A released scope has no group, and a pump finishing after the release
	// must not create one: nothing would ever read or remove it again.
	let Some(remembered) = ended.get_mut(&scope) else {
		return;
	};
	remembered.retain(|(remembered, _)| remembered != local_follow_id);
	remembered.push_back((local_follow_id.to_string(), loss));
	while remembered.len() > REMEMBERED_LOST_FOLLOWS {
		remembered.pop_front();
	}
}

/// Why a scope's follow ended, if this runtime still remembers it.
fn lookup_lost_follow(
	ended: &Mutex<HashMap<ConnectionScope, VecDeque<(String, FollowLoss)>>>,
	scope: ConnectionScope,
	requested_id: &str,
) -> Option<FollowLoss> {
	ended
		.lock()
		.unwrap()
		.get(&scope)
		.and_then(|ended| ended.iter().find(|(id, _)| id == requested_id).map(|(_, loss)| *loss))
}

/// Whether a follow's stream error means its connection went away, as opposed
/// to the node or the event itself failing on a connection still open. The
/// same split [`rpc_failure`] makes between `Unavailable` and `HostFailure`.
fn is_transport_loss(error: &SubxtRpcError) -> bool {
	matches!(error, SubxtRpcError::Client(_) | SubxtRpcError::DisconnectedWillReconnect(_))
}

/// A failed or closed transport is `Unavailable`; any other error is a
/// `HostFailure`.
pub fn rpc_failure(method: &'static str, error: SubxtRpcError) -> RuntimeFailure {
	match error {
		SubxtRpcError::Client(_) | SubxtRpcError::DisconnectedWillReconnect(_) => {
			RuntimeFailure::unavailable_with_reason(method, error.to_string())
		},
		error => RuntimeFailure::host_failure(method, error.to_string()),
	}
}

/// Encode a byte slice as a `0x`-prefixed lowercase hex string.
pub fn encode_hex(value: &[u8]) -> String {
	format!("0x{}", hex::encode(value))
}

/// Wait for a usable best block hash from a `chainHead_v1_follow` stream.
pub async fn wait_for_chain_head_best_hash(
	follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
	label: &'static str,
	initialization_timeout: Duration,
	best_hash_timeout: Duration,
) -> Result<Vec<u8>, String> {
	let timeout = futures_timer::Delay::new(initialization_timeout).fuse();
	pin_mut!(timeout);
	loop {
		let next = follow.next().fuse();
		pin_mut!(next);
		futures::select! {
			item = next => match item {
				Some(RemoteChainHeadFollowItem::Initialized { finalized_block_hashes, .. }) => {
					let fallback = finalized_block_hashes.last().cloned();
					return wait_for_chain_head_best_hash_after_initialization(
						follow,
						label,
						fallback,
						best_hash_timeout,
					)
					.await;
				}
				Some(RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash }) => {
					return Ok(best_block_hash);
				}
				Some(RemoteChainHeadFollowItem::Stop) | None => {
					return Err(format!("{label} follow stopped before initialization"));
				}
				_ => {}
			},
			() = timeout => return Err(format!("{label} follow initialization timed out")),
		}
	}
}

async fn wait_for_chain_head_best_hash_after_initialization(
	follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
	label: &'static str,
	fallback: Option<Vec<u8>>,
	timeout: Duration,
) -> Result<Vec<u8>, String> {
	let timeout = futures_timer::Delay::new(timeout).fuse();
	pin_mut!(timeout);
	loop {
		let next = follow.next().fuse();
		pin_mut!(next);
		futures::select! {
			item = next => match item {
				Some(RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash }) => {
					return Ok(best_block_hash);
				}
				Some(RemoteChainHeadFollowItem::Stop) | None => {
					return Err(format!("{label} follow stopped before best block"));
				}
				_ => {}
			},
			() = timeout => {
				return fallback.clone().ok_or_else(|| {
					format!("{label} follow best block timed out")
				});
			},
		}
	}
}

/// Context for one storage operation observed on a `chainHead_v1_follow` stream.
pub struct ChainHeadStorageValueLookup<'a> {
	pub chain: &'a ChainRuntime,
	pub genesis_hash: &'a [u8],
	pub follow_subscription_id: &'a str,
	pub operation_id: &'a str,
	pub key: &'a [u8],
	pub label: &'static str,
	pub timeout: Duration,
}

/// Result of one value query observed on a `chainHead_v1_follow` stream.
pub enum ChainHeadStorageValue {
	Found(Vec<u8>),
	Missing,
	Inaccessible,
}

/// Wait for one storage operation's value from a `chainHead_v1_follow` stream.
pub async fn wait_for_chain_head_storage_value(
	follow: &mut BoxStream<'static, RemoteChainHeadFollowItem>,
	lookup: ChainHeadStorageValueLookup<'_>,
) -> Result<ChainHeadStorageValue, String> {
	let timeout = futures_timer::Delay::new(lookup.timeout).fuse();
	pin_mut!(timeout);
	let mut value = None;
	loop {
		let next = follow.next().fuse();
		pin_mut!(next);
		futures::select! {
			item = next => match item {
				Some(RemoteChainHeadFollowItem::OperationStorageItems { operation_id: item_operation_id, items })
					if item_operation_id == lookup.operation_id =>
				{
					for item in items {
						if item.key == lookup.key {
							value = item.value;
						}
					}
				}
				Some(RemoteChainHeadFollowItem::OperationStorageDone { operation_id: item_operation_id })
					if item_operation_id == lookup.operation_id =>
				{
					return Ok(match value {
						Some(value) => ChainHeadStorageValue::Found(value),
						None => ChainHeadStorageValue::Missing,
					});
				}
				Some(RemoteChainHeadFollowItem::OperationWaitingForContinue { operation_id: item_operation_id })
					if item_operation_id == lookup.operation_id =>
				{
					lookup
						.chain
						.remote_chain_head_continue(RemoteChainHeadContinueRequest {
							genesis_hash: lookup.genesis_hash.to_vec(),
							follow_subscription_id: lookup.follow_subscription_id.to_string(),
							operation_id: lookup.operation_id.to_string(),
						})
						.await
						.map_err(|failure| failure.reason())?;
				}
				Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id: item_operation_id })
					if item_operation_id == lookup.operation_id =>
				{
					return Ok(ChainHeadStorageValue::Inaccessible);
				}
				Some(RemoteChainHeadFollowItem::OperationError { operation_id: item_operation_id, error })
					if item_operation_id == lookup.operation_id =>
				{
					return Err(error);
				}
				Some(RemoteChainHeadFollowItem::Stop) | None => {
					return Err(format!("{} follow stopped during storage lookup", lookup.label));
				}
				_ => {}
			},
			() = timeout => return Err(format!("{} storage lookup timed out", lookup.label)),
		}
	}
}

#[cfg(test)]
fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
	hex::decode(value.strip_prefix("0x").unwrap_or(value)).map_err(|_| "invalid hex".to_string())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod occupant_name_tests {
	use super::ChainConnection;

	/// A product mints its follow id from the request id it chose, which is
	/// arbitrary text off the wire. The line naming the occupants leaves the
	/// device, so it names the runtime and not what the product called itself.
	#[test]
	fn a_product_follow_is_named_without_the_id_it_chose() {
		assert_eq!(ChainConnection::occupant_name("c7:p:41"), "c7:product");
		assert_eq!(
			ChainConnection::occupant_name("c7:tarik@example.invalid"),
			"c7:product",
			"a request id the product chose reached the line",
		);
		assert_eq!(ChainConnection::occupant_name("anything"), "product");
	}

	/// The core's own ids are its own words, and say which read holds the slot.
	/// The prefix they all share says nothing one from another, and the line
	/// they go into has room for four of them.
	#[test]
	fn a_core_follow_is_named_by_what_its_id_says() {
		assert_eq!(ChainConnection::occupant_name("truapi:dotns:3:identity"), "dotns:3:identity");
	}
}
