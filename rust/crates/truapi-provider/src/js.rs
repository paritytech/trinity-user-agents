//! JavaScript-facing API for browser hosts (`js` feature, wasm32 only).
//!
//! Exposes the provider to JS without a Rust consumer: build a provider from
//! chain registrations, connect per genesis hash, and drive the raw JSON-RPC
//! string pipe. `nextResponse()` is pull-based, mirroring the smoldot npm
//! package's `nextJsonRpcResponse` so existing host code maps 1:1.
//!
//! ```js
//! const builder = new ChainProviderBuilder();
//! builder.addRpcChain("0x3740…", "wss://node.example");
//! const provider = builder.build();
//! const connection = await provider.connect("0x3740…");
//! connection.send('{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}');
//! const response = await connection.nextResponse(); // undefined once closed
//! connection.close();
//! ```
//!
//! Construct one provider per page/worker: connections share the provider's
//! resources, matching the one-provider-per-host-process contract.
//!
//! Warm start is opt-in and the crate stores nothing: hand `setStorage` a client
//! to storage the host owns and the blob for a chain is read before its first connect,
//! then written back on a schedule the provider sets. `saveDatabase` forces a write at
//! a moment the host chooses.
//!
//! Sync progress of a light-client chain is watched with `lifecycle`, once
//! something is connected to the chain:
//!
//! ```js
//! const watch = provider.lifecycle("0x3740…");
//! for (let state; (state = await watch.next()); ) {
//!   if (state.phase.kind === "syncing") showProgress(state.phase.at, state.phase.target);
//! }
//! ```

use std::sync::Arc;

use crate::platform::{ChainProvider as _, JsonRpcConnection};
use futures::{
	lock::Mutex,
	stream::{BoxStream, StreamExt},
};
use wasm_bindgen::prelude::*;

use crate::{
	config::ChainSource,
	provider::{EmbeddedChainProvider, EmbeddedChainProviderBuilder},
};

/// Lock a mutex, recovering the guard if a previous holder panicked.
#[cfg(feature = "smoldot")]
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Route the embedded provider's (and smoldot's) `tracing` output to the
/// browser console at `level` (`error`|`warn`|`info`|`debug`|`trace`; anything
/// else disables it). Installs the console subscriber on the first call and is
/// safe to call again at any time to retune verbosity.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: &str) {
	crate::logging::set_level_from_str(level);
}

/// Collects genesis-hash to chain-source registrations from JS.
///
/// The built provider resolves an unregistered genesis from the stack given to
/// `setStack`. Without one it resolves it from the bundled catalog, unless a
/// remote node was registered with `addRpcChain`.
#[wasm_bindgen]
pub struct ChainProviderBuilder {
	inner: Option<EmbeddedChainProviderBuilder>,
	/// Set by `setStack` and filled by `addLightSpec` until `build` takes it.
	#[cfg(feature = "smoldot")]
	stack: Option<crate::LightStack>,
	/// Cleared by the first `addRpcChain`.
	#[cfg(feature = "networks")]
	catalog: bool,
}

#[wasm_bindgen]
impl ChainProviderBuilder {
	/// Create an empty builder.
	#[wasm_bindgen(constructor)]
	pub fn new() -> Self {
		ChainProviderBuilder {
			inner: Some(EmbeddedChainProviderBuilder::new()),
			#[cfg(feature = "smoldot")]
			stack: None,
			#[cfg(feature = "networks")]
			catalog: true,
		}
	}

	/// Register a remote JSON-RPC node for the chain identified by the
	/// `0x`-prefixed genesis hash. A later registration for the same hash
	/// replaces the earlier one, and the built provider no longer resolves
	/// other genesis hashes from the catalog.
	#[wasm_bindgen(js_name = addRpcChain)]
	pub fn add_rpc_chain(&mut self, genesis_hash: &str, url: &str) -> Result<(), JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		let url =
			url::Url::parse(url).map_err(|err| JsError::new(&format!("invalid URL: {err}")))?;
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		self.inner = Some(builder.chain(genesis, ChainSource::rpc_node(url)));
		#[cfg(feature = "networks")]
		{
			self.catalog = false;
		}
		Ok(())
	}

	/// Register an embedded light-client chain identified by the
	/// `0x`-prefixed genesis hash. Use this for a relay or standalone chain;
	/// parachains are served through the bundled catalog (`addNetwork`), which
	/// supplies their relay wiring and statement-store placement.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = addLightChain)]
	pub fn add_light_chain(
		&mut self,
		genesis_hash: &str,
		specification: String,
	) -> Result<(), JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		let source = ChainSource::light_client(specification);
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		self.inner = Some(builder.chain(genesis, source.build()));
		Ok(())
	}

	/// Serve the chains of a Trinity stack on the light client, instead of the
	/// bundled catalog.
	///
	/// `link` is the stack's link and `descriptor` its descriptor as served,
	/// the pair a host gives the core. Throws when the link is malformed, the
	/// descriptor is refused under it, or a stack was already set. Fetch what
	/// [`light_specs`](Self::light_specs) lists and hand each spec to
	/// [`add_light_spec`](Self::add_light_spec). Build one provider per stack.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = setStack)]
	pub fn set_stack(&mut self, link: &str, descriptor: &[u8]) -> Result<(), JsError> {
		if self.inner.is_none() {
			return Err(JsError::new("builder was already consumed by build()"));
		}
		if self.stack.is_some() {
			return Err(JsError::new("setStack was already called"));
		}
		self.stack = Some(crate::LightStack::open(link, descriptor)?);
		Ok(())
	}

	/// Where to fetch each chain's light spec, in the descriptor's order. A
	/// chain the descriptor names no light spec for is left out. Empty before
	/// [`set_stack`](Self::set_stack).
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = lightSpecs)]
	pub fn light_specs(&self) -> Vec<LightSpecSource> {
		let specs = self.stack.as_ref().map(crate::LightStack::light_specs);
		specs.unwrap_or_default().into_iter().map(LightSpecSource).collect()
	}

	/// Hand over the light spec of the stack's chain with the `0x`-prefixed
	/// genesis hash, as served at its URL.
	///
	/// Throws when the bytes do not match the descriptor's `sha256` or imply
	/// another genesis hash, when the chain is not in the stack, and before
	/// [`set_stack`](Self::set_stack). A chain left without a spec is unknown
	/// to the built provider until `addLightSpec` on the provider supplies it.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = addLightSpec)]
	pub fn add_light_spec(
		&mut self,
		genesis_hash: &str,
		specification: &[u8],
	) -> Result<(), JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		let stack = self
			.stack
			.as_ref()
			.ok_or_else(|| JsError::new("call setStack before addLightSpec"))?;
		Ok(stack.add_light_spec(genesis, specification)?)
	}

	/// Seed the chain with the `0x`-prefixed genesis hash from a stored
	/// database blob, so its light client resumes from that finalized state
	/// instead of syncing from the chain-spec checkpoint.
	///
	/// The blob is the string the `chainHead_unstable_finalizedDatabase` function in smoldot
	/// produces. It seeds this run only, and beats anything
	/// [`set_storage`](Self::set_storage) would load for the same chain.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = setDatabaseContent)]
	pub fn set_database_content(
		&mut self,
		genesis_hash: &str,
		blob: String,
	) -> Result<(), JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		self.inner = Some(builder.database(genesis, blob));
		Ok(())
	}

	/// Keep database blobs in storage the host owns.
	///
	/// The crate stores nothing itself, so without this call every chain syncs
	/// from the chain-spec checkpoint on every run. Pass a JS object with
	/// `load(genesisHash, stack)` resolving to the stored string or `null`, and
	/// `save(genesisHash, blob, stack)`. Both are called with a `0x`-prefixed
	/// hex genesis hash and may return a promise. `stack` identifies the stack
	/// given to [`set_stack`](Self::set_stack) and is `undefined` without one.
	/// Two stacks can share a genesis hash, so keep a blob per stack and
	/// genesis hash. Passing `null` does nothing.
	///
	/// A blob seeded through
	/// [`set_database_content`](Self::set_database_content) beats one loaded
	/// here for the same chain.
	///
	/// A store that cannot answer must reject rather than resolve empty: an
	/// empty read is taken as nothing stored yet, and would let a later
	/// snapshot overwrite good state.
	///
	/// A loaded blob is trusted input: it becomes the finalized state the light
	/// client resumes from, so whatever can write to the store can steer the
	/// view of the chain. Origin-scoped storage satisfies that. A store fed by
	/// another page or a server does not.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = setStorage)]
	pub fn set_storage(&mut self, client: JsValue) -> Result<(), JsError> {
		if client.is_null() || client.is_undefined() {
			return Ok(());
		}
		let store = HostStorageClient::new(client)?;
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		self.inner = Some(builder.storage(std::sync::Arc::new(store)));
		Ok(())
	}

	/// Limit the kinds of connection the light client opens to peers. A field
	/// left out stays allowed; an unknown field or a non-boolean value throws.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = setConnectionTypes)]
	pub fn set_connection_types(
		&mut self,
		#[wasm_bindgen(unchecked_param_type = "ConnectionTypes")] types: JsValue,
	) -> Result<(), JsError> {
		let types = connection_types_from_js(&types)?;
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		self.inner = Some(builder.connection_types(types));
		Ok(())
	}

	/// Register every chain of the bundled network `name` (relay plus system
	/// parachains, with relay wiring and statement-store placement supplied by
	/// the catalog). Returns the network's genesis hashes.
	#[cfg(feature = "networks")]
	#[wasm_bindgen(js_name = addNetwork)]
	pub fn add_network(&mut self, name: &str) -> Result<NetworkChains, JsError> {
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		let (builder, chains) = builder.add_network(name)?;
		self.inner = Some(builder);
		Ok(NetworkChains {
			relay: hex0x(&chains.relay),
			assethub: hex0x(&chains.assethub),
			bulletin: hex0x(&chains.bulletin),
			people: hex0x(&chains.people),
		})
	}

	/// Build the provider, consuming the builder.
	pub fn build(&mut self) -> Result<ChainProviderHandle, JsError> {
		let builder = self
			.inner
			.take()
			.ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
		#[cfg(feature = "networks")]
		let builder = if self.catalog { builder.catalog() } else { builder };
		#[cfg(feature = "smoldot")]
		let builder = match self.stack.take() {
			Some(stack) => builder.stack(stack),
			None => builder,
		};
		Ok(ChainProviderHandle {
			inner: Arc::new(builder.build()),
			#[cfg(feature = "smoldot")]
			persisting: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
		})
	}
}

impl Default for ChainProviderBuilder {
	/// Same as [`ChainProviderBuilder::new`]: an empty builder.
	fn default() -> Self {
		Self::new()
	}
}

/// How long a chain runs before its first snapshot. A chain that has finalized
/// nothing yet has nothing worth storing, and asking costs a round trip.
#[cfg(feature = "smoldot")]
const FIRST_SNAPSHOT_DELAY: core::time::Duration = core::time::Duration::from_secs(30);

/// Gap between the first snapshots. Doubles up to [`SNAPSHOT_INTERVAL_MAX`],
/// because the finalized state of a chain stops being newsworthy long before the
/// snapshot stops costing a round trip and a multi-megabyte write.
#[cfg(feature = "smoldot")]
const SNAPSHOT_INTERVAL: core::time::Duration = core::time::Duration::from_secs(60);

/// Longest gap the backoff reaches.
#[cfg(feature = "smoldot")]
const SNAPSHOT_INTERVAL_MAX: core::time::Duration = core::time::Duration::from_secs(600);

/// How long storage gets to answer before a chain gives up and syncs
/// from the chain-spec checkpoint. A host-supplied store is arbitrary JS whose
/// promise may never settle, and a stored blob must not be able to wedge a
/// connection that would otherwise work.
#[cfg(feature = "smoldot")]
const STORAGE_DEADLINE: core::time::Duration = core::time::Duration::from_secs(5);

/// A built provider; hand out one per page/worker.
#[wasm_bindgen]
pub struct ChainProviderHandle {
	inner: Arc<EmbeddedChainProvider>,
	/// Chains already being snapshotted, so repeated connects to one chain do
	/// not each start their own loop.
	#[cfg(feature = "smoldot")]
	persisting: Arc<std::sync::Mutex<std::collections::HashSet<[u8; 32]>>>,
}

#[cfg(feature = "smoldot")]
impl ChainProviderHandle {
	/// Keep the finalized state for `genesis` in storage from now on, once per
	/// chain. Does nothing when the provider was built without a store.
	///
	/// The loop holds a weak reference and drops it before each wait, so it
	/// stops on its own once JS releases the provider rather than keeping it
	/// alive for the life of the page.
	fn start_persistence(&self, genesis: [u8; 32]) {
		if !self.inner.has_storage() {
			return;
		}
		if !lock(&self.persisting).insert(genesis) {
			return;
		}

		let provider = Arc::downgrade(&self.inner);
		let persisting = Arc::clone(&self.persisting);
		wasm_bindgen_futures::spawn_local(async move {
			futures_timer::Delay::new(FIRST_SNAPSHOT_DELAY).await;
			let mut interval = SNAPSHOT_INTERVAL;
			loop {
				let Some(strong) = provider.upgrade() else {
					break;
				};
				// Snapshotting a chain nobody holds open would start one just
				// to photograph it, every tick, for the life of the provider.
				if !strong.is_connected(genesis) {
					break;
				}
				if let Err(error) = strong.save_database(genesis).await {
					tracing::warn!(
						genesis = %hex0x(&genesis),
						reason = %error,
						"could not store finalized state"
					);
				}
				// Before the wait, so an idle loop does not keep the provider
				// alive after JS has let go of it.
				drop(strong);
				futures_timer::Delay::new(interval).await;
				interval = (interval * 2).min(SNAPSHOT_INTERVAL_MAX);
			}
			// Held for the whole life of the loop and released only here, so a
			// connect arriving mid-sleep finds the chain taken instead of
			// starting a second loop that doubles the snapshot cost and resets
			// the backoff. The break paths above reach this too, which is what
			// lets a later connect restart persistence once the chain is gone.
			lock(&persisting).remove(&genesis);
		});
	}
}

#[wasm_bindgen]
impl ChainProviderHandle {
	/// Open a connection to the chain identified by the `0x`-prefixed genesis
	/// hash. Rejects when the chain is not registered, the transport fails, or
	/// the light client already holds as many connections as it allows.
	pub async fn connect(&self, genesis_hash: &str) -> Result<Connection, JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		// Stored state is an optimisation, so storage that cannot answer leaves
		// the chain to sync from the checkpoint rather than failing the connect.
		// A host that supplied no storage is not asked at all, and neither is
		// a chain that is already up, whose blob smoldot would discard anyway.
		#[cfg(feature = "smoldot")]
		if self.inner.has_storage() && !self.inner.is_connected(genesis) {
			let deadline = futures_timer::Delay::new(STORAGE_DEADLINE);
			futures::pin_mut!(deadline);
			match futures::future::select(
				core::pin::pin!(self.inner.load_database(genesis)),
				deadline,
			)
			.await
			{
				futures::future::Either::Left((
					Err(error @ crate::ProviderError::UnknownGenesis { .. }),
					_,
				)) => return Err(error.into()),
				futures::future::Either::Left((Err(error), _)) => {
					tracing::warn!(
						reason = %error,
						"storage unavailable, syncing from the chain-spec checkpoint"
					)
				},
				futures::future::Either::Left((Ok(_), _)) => {},
				futures::future::Either::Right(((), _)) => {
					tracing::warn!(
						"storage did not answer in {}s, syncing from the chain-spec checkpoint",
						STORAGE_DEADLINE.as_secs()
					)
				},
			}
		}
		let connection = self.inner.connect(genesis).await?;
		#[cfg(feature = "smoldot")]
		{
			// The relay a parachain syncs through is the chain that actually
			// warp syncs, so it is kept warm alongside it.
			if let Some(relay) = self.inner.relay_of(genesis) {
				self.start_persistence(relay);
			}
			self.start_persistence(genesis);
		}
		let responses = connection.responses();
		Ok(Connection { inner: Arc::from(connection), responses: Arc::new(Mutex::new(responses)) })
	}

	/// Read the stored blob for the `0x`-prefixed genesis hash into this
	/// provider, so the next [`connect`](ChainProviderHandle::connect) to that
	/// chain resumes from it. Resolves with whether a blob is now in hand.
	///
	/// Call this before connecting. The store is read at most once per chain,
	/// because only the first add of a chain consumes a blob. Rejects for a
	/// chain this provider cannot connect to, as `connect` does.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = loadDatabase)]
	pub async fn load_database(&self, genesis_hash: &str) -> Result<bool, JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		Ok(self.inner.load_database(genesis).await?)
	}

	/// `addLightSpec` on the builder, for a spec that could not be fetched
	/// before `build`. Also throws on a provider built without a stack. A
	/// chain that is already running keeps the spec it started with.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = addLightSpec)]
	pub fn add_light_spec(&self, genesis_hash: &str, specification: &[u8]) -> Result<(), JsError> {
		Ok(self.inner.add_light_spec(parse_genesis(genesis_hash)?, specification)?)
	}

	/// Watch what the light client is doing on the chain identified by the
	/// `0x`-prefixed genesis hash. Throws when nothing is connected to it.
	///
	/// A parachain goes from `connecting` straight to `ready`; watch its relay
	/// for warp sync progress.
	#[cfg(feature = "smoldot")]
	pub fn lifecycle(&self, genesis_hash: &str) -> Result<LifecycleWatch, JsError> {
		let states = self.inner.lifecycle(parse_genesis(genesis_hash)?)?;
		let (states, stop) = futures::stream::abortable(states);
		Ok(LifecycleWatch { states: Arc::new(Mutex::new(states.boxed())), stop })
	}

	/// Snapshot the finalized state of the chain and write it to storage.
	/// Resolves with whether a blob was stored.
	///
	/// A chain that has finalized nothing yet is skipped rather than stored,
	/// since that blob would be discarded on the next run. This is a full round
	/// trip against the light client, so drive it while the page or worker is
	/// alive. Neither `pagehide` nor a hidden tab is guaranteed to stay
	/// scheduled long enough to finish one, and a Worker sees neither event.
	#[cfg(feature = "smoldot")]
	#[wasm_bindgen(js_name = saveDatabase)]
	pub async fn save_database(&self, genesis_hash: &str) -> Result<bool, JsError> {
		let genesis = parse_genesis(genesis_hash)?;
		Ok(self.inner.save_database(genesis).await?)
	}
}

/// A live JSON-RPC connection: a raw string pipe.
#[wasm_bindgen]
pub struct Connection {
	inner: Arc<dyn JsonRpcConnection>,
	responses: Arc<Mutex<BoxStream<'static, String>>>,
}

#[wasm_bindgen]
impl Connection {
	/// Queue a JSON-RPC request string.
	pub fn send(&self, request: String) {
		self.inner.send(request);
	}

	/// Resolve with the next JSON-RPC response or notification, or
	/// `undefined` once the connection is closed or dead.
	///
	/// Calling this in a loop is not optional: it is the only thing that drains
	/// the connection. Frames queue until they are taken, and once the backlog
	/// reaches the budget for the connection further [`send`](Connection::send) calls
	/// come back as JSON-RPC errors instead of being queued, so a caller that
	/// sends without reading eventually gets nothing but errors.
	#[wasm_bindgen(js_name = nextResponse)]
	pub async fn next_response(&self) -> Option<String> {
		let responses = Arc::clone(&self.responses);
		let mut responses = responses.lock().await;
		responses.next().await
	}

	/// Close the connection; pending `nextResponse()` calls resolve to
	/// `undefined`.
	pub fn close(&self) {
		self.inner.close();
	}
}

#[cfg(feature = "smoldot")]
#[wasm_bindgen(typescript_custom_section)]
const CONNECTION_TYPES_TS: &str = r#"
/** Which kinds of connection the light client opens to peers; each defaults to `true`. */
export interface ConnectionTypes {
  /** Secure `wss://` WebSocket. */
  secure?: boolean;
  /** Plain `ws://` WebSocket to a localhost peer. */
  localhost?: boolean;
  /** Plain `ws://` WebSocket to any other peer. */
  unsecure?: boolean;
}
"#;

/// Read the object [`CONNECTION_TYPES_TS`] declares.
#[cfg(feature = "smoldot")]
fn connection_types_from_js(value: &JsValue) -> Result<crate::ConnectionTypes, JsError> {
	let object = value
		.dyn_ref::<js_sys::Object>()
		.ok_or_else(|| JsError::new("connection types must be an object"))?;
	let mut types = crate::ConnectionTypes::default();
	for entry in js_sys::Object::entries(object).iter() {
		let entry = js_sys::Array::from(&entry);
		let key = entry.get(0).as_string().unwrap_or_default();
		let field = match key.as_str() {
			"secure" => &mut types.secure,
			"localhost" => &mut types.localhost,
			"unsecure" => &mut types.unsecure,
			_ => return Err(JsError::new(&format!("unknown connection type `{key}`"))),
		};
		*field = entry
			.get(1)
			.as_bool()
			.ok_or_else(|| JsError::new(&format!("connection type `{key}` must be a boolean")))?;
	}
	Ok(types)
}

#[cfg(feature = "smoldot")]
#[wasm_bindgen(typescript_custom_section)]
const CHAIN_LIFECYCLE_TS: &str = r#"
/** What the light client is doing on one chain. */
export interface ChainLifecycle {
  phase:
    | { kind: "connecting" }
    | { kind: "syncing"; at: number; target: number }
    | { kind: "ready" };
  peers: number;
  health: { kind: "ok" } | { kind: "stalled"; reason: "noPeers" | "noProgress" };
}
"#;

/// A live watch on one chain's lifecycle, from
/// [`ChainProviderHandle::lifecycle`].
#[cfg(feature = "smoldot")]
#[wasm_bindgen]
pub struct LifecycleWatch {
	states: Arc<Mutex<BoxStream<'static, crate::ChainLifecycle>>>,
	stop: futures::stream::AbortHandle,
}

#[cfg(feature = "smoldot")]
#[wasm_bindgen]
impl LifecycleWatch {
	/// Resolve with the current state on the first call and with the next
	/// change after that, or with `undefined` once the watch is closed or the
	/// chain stops running.
	#[wasm_bindgen(unchecked_return_type = "ChainLifecycle | undefined")]
	pub async fn next(&self) -> JsValue {
		let states = Arc::clone(&self.states);
		let mut states = states.lock().await;
		states.next().await.map_or(JsValue::UNDEFINED, lifecycle_to_js)
	}

	/// Stop the watch; pending `next()` calls resolve to `undefined`.
	pub fn close(&self) {
		self.stop.abort();
	}
}

#[cfg(feature = "smoldot")]
impl Drop for LifecycleWatch {
	fn drop(&mut self) {
		self.stop.abort();
	}
}

/// The `ChainLifecycle` object the TypeScript declaration above describes.
#[cfg(feature = "smoldot")]
fn lifecycle_to_js(state: crate::ChainLifecycle) -> JsValue {
	use crate::{ChainHealth, ChainPhase, StallReason};

	let phase = match state.phase {
		ChainPhase::Connecting => serde_json::json!({ "kind": "connecting" }),
		ChainPhase::Syncing { at, target } => {
			serde_json::json!({ "kind": "syncing", "at": at, "target": target })
		},
		ChainPhase::Ready => serde_json::json!({ "kind": "ready" }),
	};
	let health = match state.health {
		ChainHealth::Ok => serde_json::json!({ "kind": "ok" }),
		ChainHealth::Stalled { reason } => {
			serde_json::json!({
				"kind": "stalled",
				"reason": match reason {
					StallReason::NoPeers => "noPeers",
					StallReason::NoProgress => "noProgress",
				},
			})
		},
	};
	let object = serde_json::json!({ "phase": phase, "peers": state.peers, "health": health });
	js_sys::JSON::parse(&object.to_string()).expect("serde_json emits valid JSON")
}

/// Where a chain's light spec is published.
#[cfg(feature = "smoldot")]
#[wasm_bindgen]
pub struct LightSpecSource(crate::LightSpecSource);

#[cfg(feature = "smoldot")]
#[wasm_bindgen]
impl LightSpecSource {
	/// The chain's `0x`-prefixed genesis hash.
	#[wasm_bindgen(getter, js_name = genesisHash)]
	pub fn genesis_hash(&self) -> String {
		hex0x(&self.0.genesis_hash)
	}

	/// The chain's role in the descriptor.
	#[wasm_bindgen(getter)]
	pub fn role(&self) -> String {
		self.0.role.clone()
	}

	/// The URL the descriptor names for the spec.
	#[wasm_bindgen(getter)]
	pub fn url(&self) -> String {
		self.0.url.clone()
	}

	/// The `0x`-prefixed SHA-256 the spec's bytes are checked against.
	#[wasm_bindgen(getter)]
	pub fn sha256(&self) -> String {
		self.0.sha256.clone()
	}
}

/// The genesis hashes of a network registered via
/// [`ChainProviderBuilder::add_network`], as `0x`-prefixed hex strings.
#[cfg(feature = "networks")]
#[wasm_bindgen]
pub struct NetworkChains {
	relay: String,
	assethub: String,
	bulletin: String,
	people: String,
}

#[cfg(feature = "networks")]
#[wasm_bindgen]
impl NetworkChains {
	/// Relay-chain genesis hash.
	#[wasm_bindgen(getter)]
	pub fn relay(&self) -> String {
		self.relay.clone()
	}

	/// Asset Hub genesis hash.
	#[wasm_bindgen(getter)]
	pub fn assethub(&self) -> String {
		self.assethub.clone()
	}

	/// Bulletin-chain genesis hash.
	#[wasm_bindgen(getter)]
	pub fn bulletin(&self) -> String {
		self.bulletin.clone()
	}

	/// People-chain genesis hash.
	#[wasm_bindgen(getter)]
	pub fn people(&self) -> String {
		self.people.clone()
	}
}

#[cfg(any(feature = "networks", feature = "smoldot"))]
fn hex0x(bytes: &[u8; 32]) -> String {
	format!("0x{}", hex::encode(bytes))
}

/// A [`StorageClient`](crate::storage::StorageClient) over the `load` and
/// `save` methods.
///
/// The JS values are held in a `SendWrapper` because the trait is `Send`, and
/// every call crosses back to Rust through a oneshot channel rather than
/// awaiting the promise in place: a `JsFuture` is not `Send` and could not be
/// held across the await point in the trait method.
#[cfg(feature = "smoldot")]
struct HostStorageClient {
	inner: send_wrapper::SendWrapper<HostStorageMethods>,
}

/// The JS object and the two functions taken from it at registration time.
#[cfg(feature = "smoldot")]
struct HostStorageMethods {
	store: JsValue,
	load: js_sys::Function,
	save: js_sys::Function,
}

#[cfg(feature = "smoldot")]
impl HostStorageClient {
	/// Take `load` and `save` off `store`, failing if either is missing.
	fn new(store: JsValue) -> Result<Self, JsError> {
		let load = Self::method(&store, "load")?;
		let save = Self::method(&store, "save")?;
		Ok(Self { inner: send_wrapper::SendWrapper::new(HostStorageMethods { store, load, save }) })
	}

	/// Read one function property off the store object.
	fn method(store: &JsValue, name: &str) -> Result<js_sys::Function, JsError> {
		js_sys::Reflect::get(store, &JsValue::from_str(name))
			.map_err(|_| JsError::new(&format!("the storage client has no `{name}`")))?
			.dyn_into::<js_sys::Function>()
			.map_err(|_| JsError::new(&format!("the storage client `{name}` is not a function")))
	}
}

/// Await the result of `call` on the JS event loop, reporting it through a `Send`
/// channel the caller can hold across its own await point.
#[cfg(feature = "smoldot")]
fn await_js(
	call: Result<JsValue, JsValue>,
) -> futures::channel::oneshot::Receiver<Result<JsValue, String>> {
	let (sender, receiver) = futures::channel::oneshot::channel();
	match call {
		Ok(value) => {
			let promise = js_sys::Promise::resolve(&value);
			wasm_bindgen_futures::spawn_local(async move {
				let outcome = wasm_bindgen_futures::JsFuture::from(promise)
					.await
					.map_err(|error| describe_js(&error));
				let _ = sender.send(outcome);
			});
		},
		Err(error) => {
			let _ = sender.send(Err(describe_js(&error)));
		},
	}
	receiver
}

/// Render a thrown JS value for an error message.
#[cfg(feature = "smoldot")]
fn describe_js(error: &JsValue) -> String {
	error
		.as_string()
		.unwrap_or_else(|| format!("{:?}", js_sys::Object::from(error.clone())))
}

#[cfg(feature = "smoldot")]
#[async_trait::async_trait]
impl crate::storage::StorageClient for HostStorageClient {
	async fn load(
		&self,
		genesis_hash: [u8; 32],
		stack: Option<String>,
	) -> Result<Option<String>, crate::storage::StorageClientError> {
		let receiver = {
			let methods = &*self.inner;
			await_js(methods.load.call2(
				&methods.store,
				&JsValue::from_str(&hex0x(&genesis_hash)),
				&JsValue::from(stack),
			))
		};
		let value = receiver.await.map_err(|_| {
			crate::storage::StorageClientError::new("the warm store never answered")
		})??;
		Ok(value.as_string())
	}

	async fn save(
		&self,
		genesis_hash: [u8; 32],
		blob: String,
		stack: Option<String>,
	) -> Result<(), crate::storage::StorageClientError> {
		let receiver = {
			let methods = &*self.inner;
			await_js(methods.save.call3(
				&methods.store,
				&JsValue::from_str(&hex0x(&genesis_hash)),
				&JsValue::from_str(&blob),
				&JsValue::from(stack),
			))
		};
		receiver.await.map_err(|_| {
			crate::storage::StorageClientError::new("the warm store never answered")
		})??;
		Ok(())
	}
}

fn parse_genesis(hex_str: &str) -> Result<[u8; 32], JsError> {
	hex::decode(hex_str.trim_start_matches("0x"))
		.map_err(|err| JsError::new(&format!("invalid genesis hash hex: {err}")))?
		.try_into()
		.map_err(|_| JsError::new("genesis hashes are 32 bytes"))
}

#[cfg(test)]
mod tests;
