//! Embedded smoldot light-client backend.
//!
//! One smoldot [`Client`] is shared per provider and created lazily on the
//! first light-client connect: the native platform spawns an OS thread pool,
//! while the wasm platform schedules on the JS event loop and dials peers
//! over the browser's `WebSocket`. Each connect adds the chain again: smoldot
//! deduplicates identical chains internally while giving every add its own
//! [`ChainId`], request queue, and response stream, which yields natural
//! per-connection isolation.
//!
//! Readiness: a connection holds back the requests it is sent until smoldot
//! first reports the chain as synced, then forwards them in the order they
//! arrived, except those in [`ANSWERED_BEFORE_SYNC`]. Held requests count against
//! [`MAX_UNDELIVERED_FRAMES`], so a chain that never syncs refuses work
//! instead of growing the queue.
//!
//! Warm-start snapshots: take one with
//! [`EmbeddedChainProvider::snapshot`](crate::EmbeddedChainProvider::snapshot),
//! persist the returned string, and feed it back on a later run through
//! [`EmbeddedChainProviderBuilder::database`](crate::EmbeddedChainProviderBuilder::database),
//! which seeds a chain whether it is connected directly or brought up behind
//! one of its parachains.
//!
//! Observability: on native targets smoldot logs through the `log` crate, and
//! `logging_native` installs a stderr sink for it when `TRUAPI_PROVIDER_LOG`
//! names a level. With that variable unset no logger is installed, leaving a
//! host free to route the output from smoldot into its own `tracing` subscriber with a
//! `log`->`tracing` bridge (e.g. `tracing_log::LogTracer`).

use core::num::{NonZero, NonZeroUsize};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use crate::platform::JsonRpcConnection;
use futures::channel::{mpsc, oneshot};
use futures::future;
use futures::stream::{self, BoxStream, StreamExt};
use smoldot_light::lifecycle_service::{Phase, Subscription};
use smoldot_light::{
    AddChainConfig, AddChainConfigJsonRpc, ChainId, Client, HandleRpcError, JsonRpcResponses,
    StatementProtocolConfig,
};

use crate::config::ChainSource;
use crate::error::{ProviderError, synthetic_error_frame};

/// Lock a mutex, recovering the guard if a previous holder panicked.
///
/// The embedded light client is a single process-wide instance shared by every
/// connection, so one poisoning event must not brick all of them. smoldot's own
/// calls under this lock do not panic; this is defense-in-depth for the shared
/// singleton's blast radius.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A chain spec whose bootnodes this target can actually dial.
///
/// Native smoldot declines `/wss`, so those bootnodes are rewritten to loopback
/// `/ws` tunnels (see [`crate::wss_tunnel`]). On wasm the browser's `WebSocket`
/// already speaks TLS, so the spec is used verbatim — which is why the same spec
/// works unmodified in a page.
fn dialable_spec(specification: &str) -> std::borrow::Cow<'_, str> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::borrow::Cow::Owned(crate::wss_tunnel::tunnel_wss_bootnodes(specification))
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::borrow::Cow::Borrowed(specification)
    }
}

/// The smoldot platform backing this target. Tests wrap it to collapse the
/// sync-mode decision deadline they would otherwise wait out offline; see
/// [`crate::light_platform_test`].
#[cfg(all(not(target_arch = "wasm32"), not(test)))]
type Platform = Arc<smoldot_light::platform::DefaultPlatform>;
#[cfg(all(not(target_arch = "wasm32"), test))]
type Platform = crate::light_platform_test::ShortDeadlinePlatform<
    Arc<smoldot_light::platform::DefaultPlatform>,
>;
#[cfg(target_arch = "wasm32")]
type Platform = crate::light_platform_web::SubxtPlatform;

/// `connection_types` limits the peers the browser platform dials.
#[cfg_attr(
    not(target_arch = "wasm32"),
    expect(unused_variables, reason = "only the browser platform filters peers")
)]
fn new_platform(connection_types: crate::connection_types::ConnectionTypes) -> Platform {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Before the client starts, or its first log lines are dropped.
        crate::logging_native::init_from_env();
        let platform = smoldot_light::platform::DefaultPlatform::new(
            env!("CARGO_PKG_NAME").into(),
            env!("CARGO_PKG_VERSION").into(),
        );
        #[cfg(test)]
        {
            crate::light_platform_test::ShortDeadlinePlatform::new(platform)
        }
        #[cfg(not(test))]
        {
            platform
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::light_platform_web::SubxtPlatform::new(connection_types)
    }
}

/// Statement-store defaults mirroring smoldot's own example configuration.
const STATEMENT_MAX_SEEN: usize = 65_536;
const STATEMENT_FALSE_POSITIVE_RATE: f64 = 0.01;
const STATEMENT_AFFINITY_UPDATE_INTERVAL: Duration = Duration::from_secs(1);

/// JSON-RPC queue budgets declared to smoldot when adding a chain.
///
/// smoldot stores these but does not enforce the pending cap: its request queue
/// is `async_channel::unbounded()` and `queue_rpc_request` only fails when that
/// channel is closed, so `TooManyPendingRequests` never fires for capacity. A
/// caller that sends without draining [`JsonRpcConnection::responses`] therefore
/// grows the queue without limit, because smoldot's response channel is bounded
/// and its service stalls once the consumer stops reading. The bound is applied
/// here instead, by [`MAX_UNDELIVERED_FRAMES`].
const MAX_PENDING_REQUESTS: u32 = 1024;
const MAX_SUBSCRIPTIONS: u32 = 1024;

/// How many response frames may be waiting for the consumer before further
/// requests are refused, mirroring the bounded outbound buffer the WebSocket
/// backend uses.
///
/// Counts frames owed to the consumer, not requests in smoldot: each accepted
/// request and each synthesized error adds one, and every frame the consumer
/// takes removes one. Subscription notifications also decrement without a
/// matching request, so a subscription-heavy connection is allowed more than
/// this many requests in flight. That errs towards accepting traffic, and still
/// bounds the case this exists for, where nothing is drained at all.
const MAX_UNDELIVERED_FRAMES: usize = 1024;

/// Method prefixes smoldot answers before the chain is synced: the chain spec
/// is local, the lifecycle subscription is how a consumer watches the sync, and
/// statements and Bitswap blocks are exchanged with peers without consulting
/// the chain.
const ANSWERED_BEFORE_SYNC: [&str; 4] = [
    "chainSpec_v1_",
    "lifecycle_unstable_",
    "statement_",
    "bitswap_",
];

/// How many connections the shared client may hold at once.
///
/// Every one of them costs an `add_chain` with its own request queue and
/// response stream, a [`MAX_UNDELIVERED_FRAMES`] channel, and, on the FFI path,
/// a pump thread's stack. smoldot shares the sync state behind chains that are
/// identical, but each connect still pays its own copy of all of that, and none
/// of it is visible to the consumer holding the connections.
///
/// This is a backstop against a consumer that leaks them, not a budget to
/// tune. The bundled catalog resolves eight chains, so a consumer that keeps
/// one connection per chain stays well under it.
const MAX_CONNECTIONS: usize = 32;

struct LightInner {
    client: Client<Platform, ()>,
    /// Every chain this client is running, refcounted by what holds it: one
    /// reference per connection, plus one per parachain connection that named
    /// it as a relay. A chain is present here exactly while smoldot is running
    /// it, which decides whether a stored blob can still be consumed and whether
    /// there is anything to snapshot.
    added: HashMap<[u8; 32], AddedChain>,
    /// Live connections, bounded by [`MAX_CONNECTIONS`]. Counted here rather
    /// than summed over `added`, whose refcounts a parachain connection raises
    /// twice: once on its own chain and once on the relay it borrows.
    connections: usize,
}

/// A chain the client is running and how many things hold it.
struct AddedChain {
    /// Set for a relay this crate added behind a parachain, which no connection
    /// owns and so must be removed here when the last holder goes. A chain that
    /// only ever had connections leaves this `None`: each connection removes the
    /// chain it added itself.
    implicit_id: Option<ChainId>,
    /// The ids smoldot gave each live connection to this chain. Any of them,
    /// like `implicit_id`, reports the lifecycle of the chain.
    connection_ids: Vec<ChainId>,
    refcount: usize,
}

/// Lazily-started shared smoldot client owned by a provider.
pub struct LightState {
    inner: OnceLock<Arc<Mutex<LightInner>>>,
    /// Handed to [`new_platform`] when the client starts.
    connection_types: crate::connection_types::ConnectionTypes,
}

impl LightState {
    pub fn new(connection_types: crate::connection_types::ConnectionTypes) -> Self {
        LightState {
            inner: OnceLock::new(),
            connection_types,
        }
    }

    fn inner(&self) -> &Arc<Mutex<LightInner>> {
        self.inner.get_or_init(|| {
            Arc::new(Mutex::new(LightInner {
                client: Client::new(new_platform(self.connection_types)),
                added: HashMap::new(),
                connections: 0,
            }))
        })
    }

    /// Number of implicit relay chains currently held.
    #[cfg(test)]
    pub fn relay_count(&self) -> usize {
        lock(self.inner())
            .added
            .values()
            .filter(|chain| chain.implicit_id.is_some())
            .count()
    }

    /// Whether the client is running the chain with this genesis hash, whether
    /// a connection asked for it or a parachain brought it up as its relay.
    ///
    /// Answered from the bookkeeping the client keeps rather than from a count of
    /// handed-out connections, because a relay behind a parachain has no
    /// connection and is exactly the chain a stored blob most needs to cover.
    pub fn is_added(&self, genesis_hash: [u8; 32]) -> bool {
        self.inner
            .get()
            .is_some_and(|inner| lock(inner).added.contains_key(&genesis_hash))
    }

    /// Subscribe to the lifecycle of the chain with this genesis hash, or
    /// `None` when the client is not running it.
    ///
    /// The subscription outlives the id it was taken through: it ends only
    /// when smoldot stops running the chain.
    pub fn lifecycle(&self, genesis_hash: [u8; 32]) -> Option<Subscription> {
        let guard = lock(self.inner.get()?);
        let chain = guard.added.get(&genesis_hash)?;
        let chain_id = chain
            .implicit_id
            .or_else(|| chain.connection_ids.first().copied())?;
        Some(guard.client.lifecycle_state(chain_id))
    }

    /// Add `source` to the shared client as a [`JsonRpcConnection`]. For a
    /// parachain, `relay` carries its relay's genesis hash and resolved source.
    ///
    /// Specs pass through [`dialable_spec`] first, so a `/wss`-only bootnode set
    /// is reachable on native targets.
    pub async fn connect(
        &self,
        genesis_hash: [u8; 32],
        source: &ChainSource,
        relay: Option<([u8; 32], ChainSource)>,
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        // `ChainSource` collapses to a single variant when only the smoldot
        // backend is enabled (e.g. the iOS build), making this match irrefutable.
        #[allow(irrefutable_let_patterns)]
        let ChainSource::LightClient {
            specification,
            database_content,
            statement_protocol,
        } = source
        else {
            return Err(ProviderError::Transport {
                reason: "light backend invoked with a non-light chain source".to_owned(),
            });
        };

        // Before the lock: this parses the whole spec and needs no shared state,
        // so holding the client mutex across it would stall every other connect.
        let specification = dialable_spec(specification);

        let inner = Arc::clone(self.inner());
        let mut guard = lock(&inner);

        // Ahead of the relay add below, so a refusal takes no chain reference
        // to unwind.
        if guard.connections >= MAX_CONNECTIONS {
            return Err(ProviderError::TooManyConnections {
                limit: MAX_CONNECTIONS,
            });
        }

        let relay_genesis = relay.as_ref().map(|(genesis, _)| *genesis);
        let relay_id = match &relay {
            None => None,
            Some((genesis, relay_source)) => Some(add_relay(&mut guard, *genesis, relay_source)?),
        };

        let added = guard
            .client
            .add_chain(AddChainConfig {
                user_data: (),
                specification: specification.as_ref(),
                database_content: database_content.as_deref().unwrap_or(""),
                potential_relay_chains: relay_id.into_iter(),
                json_rpc: AddChainConfigJsonRpc::Enabled {
                    max_pending_requests: NonZero::new(MAX_PENDING_REQUESTS)
                        .expect("budget is non-zero"),
                    max_subscriptions: MAX_SUBSCRIPTIONS,
                },
                statement_protocol_config: statement_protocol.then(statement_protocol_config),
            })
            .map_err(|err| ProviderError::AddChain {
                reason: err.to_string(),
            });

        // No connection is constructed on the error path, so nothing would ever
        // release the reference taken on the relay above (smoldot rejects the
        // add when the spec's relay does not match the one supplied).
        let success = match added {
            Ok(success) => success,
            Err(error) => {
                if let Some(genesis) = relay_genesis {
                    release_chain(&mut guard, genesis);
                }
                return Err(error);
            }
        };

        // Counted only once the add succeeded, so a failed connect leaves no
        // chain behind that `is_added` would report as running.
        let entry = guard.added.entry(genesis_hash).or_insert(AddedChain {
            implicit_id: None,
            connection_ids: Vec::new(),
            refcount: 0,
        });
        entry.connection_ids.push(success.chain_id);
        entry.refcount += 1;
        guard.connections += 1;

        let responses = success
            .json_rpc_responses
            .expect("JSON-RPC was enabled for this chain");
        let lifecycle = guard.client.lifecycle_state(success.chain_id);
        drop(guard);

        // `send` synthesizes an error onto this channel when a request is refused,
        // so a full queue fails the caller fast instead of hanging. Bounded by the
        // same budget as the responses it stands in for: an unbounded one would
        // just move the growth it exists to prevent into the refusal path.
        let (errors_tx, errors_rx) = mpsc::channel(MAX_UNDELIVERED_FRAMES);
        let (closing_tx, closing_rx) = oneshot::channel();

        Ok(Box::new(LightConnection {
            pipe: Arc::new(Pipe {
                inner,
                chain_id: success.chain_id,
                errors_tx: Mutex::new(errors_tx),
                undelivered: AtomicUsize::new(0),
                held: Mutex::new(Some(Vec::new())),
                closing: Mutex::new(Some(closing_tx)),
            }),
            genesis_hash,
            relay: relay_genesis,
            responses: Mutex::new(Some(ResponseSources {
                responses,
                errors: errors_rx,
                lifecycle,
                closing: closing_rx,
            })),
        }))
    }
}

/// Add the relay chain for a parachain entry, reusing an already-added one and
/// taking a reference on it for the calling connection.
///
/// The relay is added with JSON-RPC disabled: it exists only so the parachain
/// can sync, and a direct connection to the relay genesis goes through its own
/// registry entry.
fn add_relay(
    guard: &mut LightInner,
    relay_genesis: [u8; 32],
    relay_source: &ChainSource,
) -> Result<ChainId, ProviderError> {
    // Only an implicit add can be handed to a parachain: a direct connection to
    // the same genesis is its own chain, and removing it would strand the
    // parachain that borrowed its id.
    if let Some(existing) = guard.added.get_mut(&relay_genesis)
        && let Some(chain_id) = existing.implicit_id
    {
        existing.refcount += 1;
        return Ok(chain_id);
    }

    // `ChainSource` collapses to a single variant when only the smoldot backend
    // is enabled, making this match irrefutable.
    #[allow(irrefutable_let_patterns)]
    let ChainSource::LightClient {
        specification,
        database_content,
        statement_protocol,
        ..
    } = relay_source
    else {
        return Err(ProviderError::UnknownRelay {
            relay: relay_genesis,
        });
    };

    let specification = dialable_spec(specification);
    let success = guard
        .client
        .add_chain(AddChainConfig {
            user_data: (),
            specification: specification.as_ref(),
            database_content: database_content.as_deref().unwrap_or(""),
            potential_relay_chains: core::iter::empty(),
            json_rpc: AddChainConfigJsonRpc::Disabled,
            statement_protocol_config: statement_protocol.then(statement_protocol_config),
        })
        .map_err(|err| ProviderError::AddChain {
            reason: err.to_string(),
        })?;

    let entry = guard.added.entry(relay_genesis).or_insert(AddedChain {
        implicit_id: None,
        connection_ids: Vec::new(),
        refcount: 0,
    });
    entry.implicit_id = Some(success.chain_id);
    entry.refcount += 1;
    Ok(success.chain_id)
}

/// Drop one reference on the implicit relay `relay_genesis`, removing it from
/// the client once the last parachain connection using it is gone.
///
/// Callers must have already removed (or never added) the parachain chain that
/// held the reference, so no live chain still depends on the relay.
fn release_chain(guard: &mut LightInner, genesis_hash: [u8; 32]) {
    let Some(entry) = guard.added.get_mut(&genesis_hash) else {
        return;
    };
    entry.refcount -= 1;
    if entry.refcount > 0 {
        return;
    }
    let implicit_id = entry.implicit_id;
    guard.added.remove(&genesis_hash);
    // A connection removes the chain it added itself. Only an implicit relay
    // has no owner to do that for it.
    if let Some(relay_id) = implicit_id {
        let _: () = guard.client.remove_chain(relay_id);
    }
}

fn statement_protocol_config() -> StatementProtocolConfig {
    StatementProtocolConfig::new(
        NonZeroUsize::new(STATEMENT_MAX_SEEN).expect("budget is non-zero"),
        STATEMENT_FALSE_POSITIVE_RATE,
        statement_seed(),
        STATEMENT_AFFINITY_UPDATE_INTERVAL,
    )
}

/// Random bloom-filter seed from the target's entropy source.
fn statement_seed() -> u128 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rand::random()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).expect("the browser provides entropy");
        u128::from_le_bytes(bytes)
    }
}

/// One added smoldot chain exposed as a raw JSON-RPC pipe.
struct LightConnection {
    pipe: Arc<Pipe>,
    /// Genesis of the chain this connection is for. Its reference is released
    /// on close, which is what stops reporting the chain as running.
    genesis_hash: [u8; 32],
    /// Genesis of the implicit relay this connection holds a reference on, if
    /// it is a parachain; released on close.
    relay: Option<[u8; 32]>,
    /// Taken once by `responses()`.
    responses: Mutex<Option<ResponseSources>>,
}

/// What `responses()` merges into the stream it hands out.
struct ResponseSources {
    responses: JsonRpcResponses<Platform>,
    /// Receiver for [`Pipe::errors_tx`].
    errors: mpsc::Receiver<String>,
    /// Watched until the chain first syncs, which releases the held requests.
    lifecycle: Subscription,
    /// Resolves when the connection closes.
    closing: oneshot::Receiver<()>,
}

/// The request side of a connection, shared with the stream that releases
/// held requests once the chain syncs.
struct Pipe {
    inner: Arc<Mutex<LightInner>>,
    chain_id: ChainId,
    /// Synthetic JSON-RPC error frames for requests smoldot rejected, merged
    /// into the response stream so the caller fails fast.
    errors_tx: Mutex<mpsc::Sender<String>>,
    /// Frames owed to the consumer, bounded by [`MAX_UNDELIVERED_FRAMES`].
    undelivered: AtomicUsize,
    /// Requests sent before the chain first synced, in send order. `None`
    /// once they are released, after which requests go straight to smoldot.
    held: Mutex<Option<Vec<String>>>,
    /// Taken by `close`, which ends the wait for the chain to sync. `None`
    /// means the connection is closed.
    closing: Mutex<Option<oneshot::Sender<()>>>,
}

/// Decrement `counter` unless it is already zero.
///
/// Subscription notifications arrive without a matching request, so a plain
/// `fetch_sub` would wrap past zero and, being a `usize`, land on a value that
/// refuses every later request. Written as a compare-exchange loop because the
/// saturating helper on atomics is nightly-only.
fn decrement_saturating(counter: &AtomicUsize) {
    let mut current = counter.load(Ordering::Acquire);
    while current > 0 {
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

/// Whether smoldot can only answer `request` once the chain has synced.
///
/// A request without a readable method is forwarded, since smoldot rejects it
/// without consulting the chain.
fn waits_for_sync(request: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(request) else {
        return false;
    };
    value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|method| {
            !ANSWERED_BEFORE_SYNC
                .iter()
                .any(|prefix| method.starts_with(prefix))
        })
}

/// Resolve once the chain first reports [`Phase::Ready`], or once it is
/// removed, which only follows this connection closing.
async fn first_ready(mut lifecycle: Subscription) {
    while let Some(state) = lifecycle.next().await {
        if state.phase == Phase::Ready {
            return;
        }
    }
}

impl Pipe {
    fn is_closed(&self) -> bool {
        lock(&self.closing).is_none()
    }

    fn send(&self, request: String) {
        // Parsed before taking the client lock, which every connection shares.
        let holding = lock(&self.held).is_some();
        let needs_sync = holding && waits_for_sync(&request);

        // The chain-removal check and the request must happen under the same
        // lock: json_rpc_request panics on a removed ChainId.
        let mut guard = lock(&self.inner);
        if self.is_closed() {
            return;
        }

        // smoldot would queue this without limit, so the backpressure is ours to
        // apply: a consumer that stops reading stops being sent work.
        if self.undelivered.load(Ordering::Acquire) >= MAX_UNDELIVERED_FRAMES {
            drop(guard);
            self.refuse(&request, "light client response queue full");
            return;
        }

        self.undelivered.fetch_add(1, Ordering::AcqRel);
        if needs_sync && let Some(held) = lock(&self.held).as_mut() {
            held.push(request);
            return;
        }
        if let Err(rejected) = self.forward(&mut guard, request) {
            drop(guard);
            self.reject(&rejected);
        }
    }

    /// Hand `request`, already counted in `undelivered`, to smoldot, returning
    /// it if smoldot refuses it.
    fn forward(&self, guard: &mut LightInner, request: String) -> Result<(), String> {
        guard
            .client
            .json_rpc_request(request, self.chain_id)
            .map_err(|HandleRpcError::TooManyPendingRequests { json_rpc_request }| json_rpc_request)
    }

    /// Forward the requests held while the chain was syncing, in send order.
    fn release_held(&self) {
        let mut guard = lock(&self.inner);
        if self.is_closed() {
            return;
        }
        let Some(held) = lock(&self.held).take() else {
            return;
        };
        for request in held {
            if let Err(rejected) = self.forward(&mut guard, request) {
                self.reject(&rejected);
            }
        }
    }

    /// Answer a request smoldot refused. Not reachable while the chain is live
    /// (smoldot's queue is unbounded), but it owes the caller a frame either way.
    fn reject(&self, request: &str) {
        self.undelivered.fetch_sub(1, Ordering::AcqRel);
        self.refuse(request, "light client request queue full");
    }

    /// Refuse `request` with a synthetic error frame, keeping the connection
    /// alive. The consumer correlates by id, so a dropped request without a
    /// frame for its id would leave it waiting forever.
    fn refuse(&self, request: &str, reason: &str) {
        tracing::warn!(reason, "refusing a light-client request");
        let Some(frame) = synthetic_error_frame(request, reason) else {
            return;
        };
        // A full error channel means the consumer is not draining even the
        // refusals, so the frame is dropped rather than queued behind them.
        if lock(&self.errors_tx).try_send(frame).is_ok() {
            self.undelivered.fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl JsonRpcConnection for LightConnection {
    fn send(&self, request: String) {
        self.pipe.send(request);
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let Some(sources) = lock(&self.responses).take() else {
            return stream::empty().boxed();
        };
        let responses = stream::unfold(sources.responses, |mut responses| async move {
            responses.next().await.map(|item| (item, responses))
        });
        let pipe = Arc::clone(&self.pipe);
        // Yields nothing: it ends once the chain syncs, releasing what was
        // held, or once the connection closes.
        let release = stream::once(async move {
            let ready = core::pin::pin!(first_ready(sources.lifecycle));
            future::select(ready, sources.closing).await;
            pipe.release_held();
        })
        .filter_map(|()| future::ready(None));
        let pipe = Arc::clone(&self.pipe);
        stream::select(stream::select(responses, sources.errors), release)
            .inspect(move |_| decrement_saturating(&pipe.undelivered))
            .boxed()
    }

    fn close(&self) {
        let mut guard = lock(&self.pipe.inner);
        if lock(&self.pipe.closing).take().is_none() {
            return;
        }
        guard.connections -= 1;
        // Removal makes JsonRpcResponses::next() return None; dropping `closing`
        // above and closing the error channel end the other two parts of the
        // merged stream, so `responses()` terminates cleanly.
        let _: () = guard.client.remove_chain(self.pipe.chain_id);
        if let Some(chain) = guard.added.get_mut(&self.genesis_hash) {
            chain.connection_ids.retain(|id| *id != self.pipe.chain_id);
        }
        release_chain(&mut guard, self.genesis_hash);

        // This connection's own chain is already removed above, so no live chain
        // still depends on the relay it held a reference on.
        if let Some(relay_genesis) = self.relay {
            release_chain(&mut guard, relay_genesis);
        }

        lock(&self.pipe.errors_tx).close_channel();
        // An untaken response stream keeps smoldot's JSON-RPC service for the
        // chain alive, and with it the chain's lifecycle.
        lock(&self.responses).take();
    }
}

impl Drop for LightConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;
    use std::sync::Arc;

    use futures::FutureExt;
    use futures::executor::block_on;
    use futures::future::{self, Either};
    use futures::stream::StreamExt;

    use crate::platform::ChainProvider;
    use crate::{ChainSource, EmbeddedChainProvider};

    /// Real relay-chain spec (checkpoint included) vendored from smoldot's
    /// demo specs: `add_chain` and spec-local JSON-RPC queries succeed without
    /// any network access.
    const RELAY_SPEC: &str = include_str!("../tests/fixtures/paseo.json");

    /// Parachain of [`RELAY_SPEC`], used to exercise the relay-add path.
    const PARACHAIN_SPEC: &str = include_str!("../tests/fixtures/paseo_people.json");

    const RELAY_GENESIS: [u8; 32] = [1; 32];
    const PARACHAIN_GENESIS: [u8; 32] = [2; 32];

    fn frame_id(frame: &str) -> serde_json::Value {
        let frame: serde_json::Value = serde_json::from_str(frame).expect("valid JSON");
        frame["id"].clone()
    }

    fn offline_provider() -> EmbeddedChainProvider {
        EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .build()
    }

    /// The blocker this file exists to prevent. The relay under a parachain has no
    /// connection of its own, so anything counting handed-out connections
    /// cannot see it, and nothing would ever store the one chain that
    /// actually warp syncs.
    #[test]
    fn a_relay_behind_a_parachain_counts_as_added() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();

        let connection =
            block_on(provider.connect(PARACHAIN_GENESIS)).expect("offline add_chain succeeds");
        assert!(
            provider.is_connected(PARACHAIN_GENESIS),
            "the chain that was asked for is running"
        );
        assert!(
            provider.is_connected(RELAY_GENESIS),
            "so is the relay it was brought up behind, which nothing connected to"
        );

        connection.close();
        assert!(!provider.is_connected(PARACHAIN_GENESIS));
        assert!(
            !provider.is_connected(RELAY_GENESIS),
            "the relay goes when its last parachain does"
        );
    }

    /// A chain is running only while something holds it, and a second close
    /// must not underflow the count.
    #[test]
    fn a_chain_is_added_until_its_last_connection_closes() {
        let provider = offline_provider();
        let first = block_on(provider.connect(RELAY_GENESIS)).expect("first connect");
        let second = block_on(provider.connect(RELAY_GENESIS)).expect("second connect");
        assert!(provider.is_connected(RELAY_GENESIS));

        first.close();
        assert!(
            provider.is_connected(RELAY_GENESIS),
            "one closed connection does not stop the chain"
        );

        second.close();
        assert!(!provider.is_connected(RELAY_GENESIS));
        second.close();
        assert!(!provider.is_connected(RELAY_GENESIS), "close is idempotent");
    }

    /// The ceiling is there for a consumer that never closes what it opens, so
    /// what matters is both halves: that the connect over it is refused, and
    /// that closing one hands the slot back instead of spending it for the
    /// life of the process.
    #[test]
    fn connections_are_capped_and_closing_one_frees_a_slot() {
        let provider = offline_provider();
        let mut held = Vec::new();
        for _ in 0..super::MAX_CONNECTIONS {
            held.push(block_on(provider.connect(RELAY_GENESIS)).expect("connect under the cap"));
        }

        let error = block_on(provider.connect(RELAY_GENESIS))
            .err()
            .expect("a connect over the cap must fail");
        assert_eq!(
            error.to_string(),
            format!(
                "the light client already holds {} connections",
                super::MAX_CONNECTIONS
            )
        );

        held.pop().expect("one to close").close();
        block_on(provider.connect(RELAY_GENESIS)).expect("the closed connection freed its slot");
    }

    /// Dropping a handle without closing it is exactly what the consumer the
    /// ceiling exists for does, and `Drop` is the only thing that frees the
    /// slot on that path. Without it the ceiling stops being a bound on live
    /// connections and becomes a budget of [`MAX_CONNECTIONS`] for the life of
    /// the process.
    #[test]
    fn dropping_a_handle_frees_its_slot() {
        let provider = offline_provider();
        let mut held = Vec::new();
        for _ in 0..super::MAX_CONNECTIONS {
            held.push(block_on(provider.connect(RELAY_GENESIS)).expect("connect under the cap"));
        }

        drop(held.pop().expect("one to drop"));
        block_on(provider.connect(RELAY_GENESIS)).expect("the dropped handle freed its slot");
    }

    /// Why the count is its own field rather than a sum over the chain
    /// refcounts: a parachain raises two of those, its own and the relay it
    /// borrows, so summing them would spend the ceiling twice as fast as
    /// connections are actually handed out.
    #[test]
    fn a_parachain_connection_spends_one_slot() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();

        let mut held = Vec::new();
        for _ in 0..super::MAX_CONNECTIONS {
            held.push(
                block_on(provider.connect(PARACHAIN_GENESIS)).expect("connect under the cap"),
            );
        }

        let error = block_on(provider.connect(PARACHAIN_GENESIS))
            .err()
            .expect("a connect over the cap must fail");
        assert_eq!(
            error.to_string(),
            format!(
                "the light client already holds {} connections",
                super::MAX_CONNECTIONS
            )
        );
    }

    /// A chain that was never connected is not running, so there is nothing to
    /// snapshot and a stored blob can still be consumed.
    #[test]
    fn an_unconnected_chain_is_not_added() {
        let provider = offline_provider();
        assert!(!provider.is_connected(RELAY_GENESIS));
    }

    #[test]
    fn garbage_chain_spec_is_an_error() {
        let provider = EmbeddedChainProvider::builder()
            .chain(
                [1; 32],
                ChainSource::light_client("not a chain spec").build(),
            )
            .build();
        let error = block_on(provider.connect([1; 32]))
            .err()
            .expect("a malformed chain spec must fail to connect");
        assert!(error.to_string().contains("failed to add a chain"));
    }

    #[test]
    fn unknown_relay_is_an_error() {
        let provider = EmbeddedChainProvider::builder()
            .parachain(
                RELAY_GENESIS,
                ChainSource::light_client(RELAY_SPEC).build(),
                [9; 32],
            )
            .build();
        let error = block_on(provider.connect(RELAY_GENESIS))
            .err()
            .expect("an unregistered relay must fail to connect");
        assert!(
            error
                .to_string()
                .contains("not a registered light-client chain")
        );
    }

    #[test]
    fn chain_name_round_trips_without_a_network() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        connection.send(
            r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
        let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
        assert!(
            response.contains("Paseo Testnet"),
            "unexpected response: {response}"
        );
    }

    #[test]
    fn parachain_reuses_its_registered_relay() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();
        // Two connects: the second must reuse the cached relay ChainId.
        for _ in 0..2 {
            let connection = block_on(provider.connect(PARACHAIN_GENESIS))
                .expect("parachain add_chain succeeds with its relay registered");
            let mut responses = connection.responses();
            connection.send(
                r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#
                    .to_owned(),
            );
            let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
            assert!(
                response.contains("Paseo People"),
                "unexpected response: {response}"
            );
        }
    }

    #[test]
    fn relay_is_reclaimed_when_the_last_parachain_closes() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();
        let first = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
        let second = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
        assert_eq!(
            provider.relay_count(),
            1,
            "both connections share one relay"
        );
        first.close();
        assert_eq!(
            provider.relay_count(),
            1,
            "the relay stays while a parachain connection is live"
        );
        second.close();
        assert_eq!(
            provider.relay_count(),
            0,
            "the relay is reclaimed after the last parachain connection closes"
        );
    }

    #[test]
    fn a_failed_parachain_add_releases_its_relay() {
        // The relay entry is a chain whose spec id is not the one the parachain
        // declares, so smoldot refuses the parachain with NoRelayChainFound
        // after the relay has already been added and referenced.
        let provider = EmbeddedChainProvider::builder()
            .chain(
                RELAY_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
            )
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();
        block_on(provider.connect(PARACHAIN_GENESIS))
            .err()
            .expect("a parachain whose relay does not match must fail to connect");
        assert_eq!(
            provider.relay_count(),
            0,
            "the relay must not leak when the parachain add fails"
        );
    }

    /// A consumer that never reads must stop being accepted work.
    ///
    /// smoldot would queue these without limit: its request channel is unbounded
    /// and its service stalls once the bounded response channel backs up, so the
    /// only thing standing between a runaway `send()` loop and unbounded memory
    /// is the frame budget this asserts.
    #[test]
    fn sending_without_draining_is_refused_once_the_budget_is_spent() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        // Held but deliberately never polled, which is what makes frames pile up.
        let mut responses = connection.responses();

        let overshoot = 64;
        for id in 0..super::MAX_UNDELIVERED_FRAMES + overshoot {
            connection.send(format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"chainSpec_v1_chainName","params":[]}}"#
            ));
        }

        // Drain what is buffered and count the refusals among it.
        let mut refusals = 0;
        while let Some(Some(frame)) = block_on(async { Some(responses.next().await) }) {
            if frame.contains("response queue full") {
                refusals += 1;
            }
            if refusals >= overshoot {
                break;
            }
        }
        assert_eq!(
            refusals, overshoot,
            "every request past the budget must come back as an error frame"
        );
    }

    /// `rpc_methods` stands in for a request that needs the synced chain.
    /// smoldot answers it at once, so only the hold explains it coming back
    /// after the chain-spec query sent behind it. Offline, the test platform
    /// lets the chain settle as synced within a fraction of a second, which is
    /// what releases the hold.
    #[test]
    fn requests_wait_for_the_chain_to_sync_and_keep_their_order() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        for id in 1..=2 {
            connection.send(format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"rpc_methods","params":[]}}"#
            ));
        }
        connection.send(
            r#"{"jsonrpc":"2.0","id":3,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );

        let mut next_id =
            || frame_id(&block_on(responses.next()).expect("the connection stays alive"));
        assert_eq!(next_id(), 3, "the chain-spec query is not held");
        assert_eq!(
            [next_id(), next_id()],
            [1, 2],
            "held requests go out in send order once the chain syncs"
        );
    }

    /// Statements are gossiped with peers and never consult the chain, so a
    /// statement subscription opened during a warp sync must not wait it out.
    /// It is answered while the `rpc_methods` sent ahead of it is still held.
    #[test]
    fn statement_requests_are_not_held() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        connection
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"rpc_methods","params":[]}"#.to_owned());
        connection.send(
            r#"{"jsonrpc":"2.0","id":2,"method":"statement_subscribeStatement","params":["any"]}"#
                .to_owned(),
        );

        let frame = block_on(responses.next()).expect("the connection stays alive");
        assert_eq!(
            frame_id(&frame),
            2,
            "the statement subscription is not held"
        );
    }

    /// A chain that has not synced must not hold requests without limit: the
    /// ones past the budget are refused, and the ones within it are answered
    /// once the chain syncs.
    #[test]
    fn held_requests_count_against_the_budget() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();

        let overshoot = 64;
        let sent = super::MAX_UNDELIVERED_FRAMES + overshoot;
        for id in 0..sent {
            connection.send(format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"rpc_methods","params":[]}}"#
            ));
        }

        let refusals = (0..sent)
            .map(|_| block_on(responses.next()).expect("every request is answered"))
            .filter(|frame| frame.contains("response queue full"))
            .count();
        assert_eq!(refusals, overshoot);
    }

    /// The hold ends on the first `Ready` and on nothing before it: a chain
    /// that is connecting or warp syncing still answers from a state that has
    /// not caught up.
    #[test]
    fn the_hold_ends_on_the_first_ready_and_no_earlier() {
        use smoldot_light::lifecycle_service::{LifecycleService, Phase};

        let service = LifecycleService::new();
        let mut ready = Box::pin(super::first_ready(service.subscribe()));
        assert!(ready.as_mut().now_or_never().is_none(), "connecting holds");

        block_on(service.update(|state| state.phase = Phase::Syncing { at: 1, target: 9 }));
        assert!(ready.as_mut().now_or_never().is_none(), "syncing holds");

        block_on(service.update(|state| state.phase = Phase::Ready));
        assert!(ready.now_or_never().is_some(), "ready releases");
    }

    /// A host that polls a finished watch again, or twice at once, must get
    /// the end again rather than a panic from polling an ended stream.
    #[test]
    fn a_lifecycle_keeps_reporting_its_end() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut lifecycle = provider
            .lifecycle(RELAY_GENESIS)
            .expect("a connected chain has a lifecycle");
        connection.close();
        while block_on(lifecycle.next()).is_some() {}
        assert_eq!(block_on(lifecycle.next()), None);
    }

    /// A host shows sync progress through this stream, so it must start with
    /// where the chain is now and end once nothing runs the chain, rather than
    /// leave the host waiting on a chain that is gone.
    #[test]
    fn a_lifecycle_starts_with_the_current_state_and_ends_with_the_chain() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut lifecycle = provider
            .lifecycle(RELAY_GENESIS)
            .expect("a connected chain has a lifecycle");
        block_on(lifecycle.next()).expect("the current state comes first");

        connection.close();
        let deadline = futures_timer::Delay::new(Duration::from_secs(10));
        let ended = block_on(future::select(lifecycle.count(), deadline));
        assert!(
            matches!(ended, Either::Left(_)),
            "the stream ends once the chain stops running"
        );
    }

    /// The relay behind a parachain has no connection of its own, and is the
    /// only chain in the pair that reports warp sync progress.
    #[test]
    fn the_relay_behind_a_parachain_has_a_lifecycle() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .parachain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).build(),
                RELAY_GENESIS,
            )
            .build();
        let _connection =
            block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
        let mut lifecycle = provider
            .lifecycle(RELAY_GENESIS)
            .expect("the implicit relay is running");
        block_on(lifecycle.next()).expect("the relay reports its state");
    }

    /// A second connection keeps the chain running after the first closes, so
    /// the lifecycle must still be reachable through the one that is left.
    #[test]
    fn a_lifecycle_is_reachable_while_any_connection_holds_the_chain() {
        let provider = offline_provider();
        let first = block_on(provider.connect(RELAY_GENESIS)).expect("first connect");
        let _second = block_on(provider.connect(RELAY_GENESIS)).expect("second connect");
        first.close();
        let mut lifecycle = provider
            .lifecycle(RELAY_GENESIS)
            .expect("the chain is still running");
        block_on(lifecycle.next()).expect("the remaining connection reports its state");
    }

    #[test]
    fn an_unconnected_chain_has_no_lifecycle() {
        let provider = offline_provider();
        let error = provider
            .lifecycle(RELAY_GENESIS)
            .err()
            .expect("nothing runs the chain");
        assert!(
            error.to_string().contains("connect to it first"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn close_is_idempotent_and_ends_the_stream() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        connection.close();
        connection.close();
        assert_eq!(block_on(responses.next()), None);
        // A late send must not panic on the removed chain.
        connection.send(
            r#"{"jsonrpc":"2.0","id":2,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
    }

    #[test]
    fn concurrent_parachain_connects_share_and_reclaim_the_relay() {
        // Many threads race the lazy client init and the shared relay refcount;
        // once every connection closes the relay must be fully reclaimed, with
        // no panic and no leak.
        let provider = Arc::new(
            EmbeddedChainProvider::builder()
                .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
                .parachain(
                    PARACHAIN_GENESIS,
                    ChainSource::light_client(PARACHAIN_SPEC).build(),
                    RELAY_GENESIS,
                )
                .build(),
        );
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let provider = Arc::clone(&provider);
                std::thread::spawn(move || {
                    let connection =
                        block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
                    connection.close();
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("connect thread does not panic");
        }
        assert_eq!(provider.relay_count(), 0, "no relay leaks under contention");
    }

    #[test]
    fn connections_to_the_same_chain_are_isolated() {
        let provider = offline_provider();
        let first = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let second = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut second_responses = second.responses();
        first.close();
        second.send(
            r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
        let response =
            block_on(second_responses.next()).expect("the second connection stays alive");
        assert!(
            response.contains("Paseo Testnet"),
            "unexpected response: {response}"
        );
    }
}
