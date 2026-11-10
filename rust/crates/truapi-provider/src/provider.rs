//! Genesis-hash registry dispatching each connection to its backend.
//!
//! A parachain's relay is provider topology (see `relays`), not part of
//! [`ChainSource`]; the light backend brings the relay up behind the parachain.

use std::collections::HashMap;
#[cfg(feature = "smoldot")]
use std::sync::Mutex;

use crate::platform::{ChainProvider, JsonRpcConnection};

use crate::{
	config::{ChainSource, ResolvedChain},
	error::ProviderError,
};

/// Lock a mutex, recovering the guard if a previous holder panicked.
#[cfg(feature = "smoldot")]
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Builder collecting genesis-hash to [`ChainSource`] registrations.
#[derive(Default)]
pub struct EmbeddedChainProviderBuilder {
	chains: HashMap<[u8; 32], ChainSource>,
	/// The relay each parachain syncs through, keyed by parachain genesis hash
	/// (exactly one per parachain).
	#[cfg(feature = "smoldot")]
	relays: HashMap<[u8; 32], [u8; 32]>,
	/// Warm-start database blobs keyed by genesis hash, applied to a
	/// light-client chain at connect time if it has no explicit blob.
	#[cfg(feature = "smoldot")]
	seeded_databases: HashMap<[u8; 32], String>,
	/// Where warm-start blobs are read from and written back to.
	#[cfg(feature = "smoldot")]
	storage: Option<std::sync::Arc<dyn crate::storage::StorageClient>>,
	/// The peer connections the browser light client opens.
	#[cfg(feature = "smoldot")]
	connection_types: crate::connection_types::ConnectionTypes,
	/// Set by [`keep_chains_running`](Self::keep_chains_running).
	#[cfg(feature = "smoldot")]
	keep_chains_running: bool,
	/// Set by [`stack`](Self::stack).
	#[cfg(feature = "smoldot")]
	stack: Option<crate::stack::LightStack>,
	/// Set by [`catalog`](Self::catalog).
	#[cfg(feature = "networks")]
	catalog: bool,
}

impl core::fmt::Debug for EmbeddedChainProviderBuilder {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		let mut builder = f.debug_struct("EmbeddedChainProviderBuilder");
		builder.field("chains", &self.chains);
		#[cfg(feature = "smoldot")]
		builder
			.field("relays", &self.relays)
			.field("seeded_databases", &self.seeded_databases)
			.field("storage", &self.storage.is_some())
			.field("connection_types", &self.connection_types)
			.field("keep_chains_running", &self.keep_chains_running)
			.field("stack", &self.stack);
		#[cfg(feature = "networks")]
		builder.field("catalog", &self.catalog);
		builder.finish()
	}
}

impl EmbeddedChainProviderBuilder {
	/// Create an empty builder.
	pub fn new() -> Self {
		Self::default()
	}

	/// Register `source` as the backend for the chain identified by
	/// `genesis_hash`. A later registration for the same hash replaces the
	/// earlier one.
	pub fn chain(mut self, genesis_hash: [u8; 32], source: ChainSource) -> Self {
		self.chains.insert(genesis_hash, source);
		self
	}

	/// Register `source` as a parachain syncing through the relay registered
	/// under `relay_genesis`. A later registration for `genesis_hash` wins.
	#[cfg(feature = "smoldot")]
	pub fn parachain(
		mut self,
		genesis_hash: [u8; 32],
		source: ChainSource,
		relay_genesis: [u8; 32],
	) -> Self {
		self.chains.insert(genesis_hash, source);
		self.relays.insert(genesis_hash, relay_genesis);
		self
	}

	/// Seed a warm-start database blob (previously produced by
	/// [`EmbeddedChainProvider::snapshot`]) for `genesis_hash`, so its light
	/// client resumes from that finalized state instead of syncing from the
	/// chain-spec checkpoint. Applies to catalog-resolved chains too, and is
	/// ignored for a chain that already carries an explicit blob.
	#[cfg(feature = "smoldot")]
	pub fn database(mut self, genesis_hash: [u8; 32], blob: String) -> Self {
		self.seeded_databases.insert(genesis_hash, blob);
		self
	}

	/// Keep warm-start blobs in `store`, so
	/// [`load_database`](EmbeddedChainProvider::load_database) and
	/// [`save_database`](EmbeddedChainProvider::save_database) have somewhere to read from
	/// and write to. A blob registered with [`database`](Self::database) still
	/// wins over a stored one.
	#[cfg(feature = "smoldot")]
	pub fn storage(mut self, store: std::sync::Arc<dyn crate::storage::StorageClient>) -> Self {
		self.storage = Some(store);
		self
	}

	/// Limit the kinds of connection the browser light client opens to peers.
	#[cfg(all(feature = "smoldot", target_arch = "wasm32"))]
	pub fn connection_types(mut self, connection_types: crate::ConnectionTypes) -> Self {
		self.connection_types = connection_types;
		self
	}

	/// Keep each light-client chain running from its first connect until the
	/// provider is dropped, so it keeps its sync and peers between connections.
	#[cfg(feature = "smoldot")]
	pub fn keep_chains_running(mut self) -> Self {
		self.keep_chains_running = true;
		self
	}

	/// Serve the chains of `stack` on the light client; see
	/// [`LightStack`](crate::LightStack).
	///
	/// A chain registered with [`chain`](Self::chain) still wins, and the
	/// bundled catalog is not consulted.
	#[cfg(feature = "smoldot")]
	pub fn stack(mut self, stack: crate::stack::LightStack) -> Self {
		self.stack = Some(stack);
		self
	}

	/// Resolve a genesis nobody registered from the bundled catalog. Without it, a
	/// provider answers only for the chains registered on it, so one for remote
	/// nodes never starts the light client. Ignored on a provider built with a
	/// [`stack`](Self::stack).
	#[cfg(feature = "networks")]
	pub fn catalog(mut self) -> Self {
		self.catalog = true;
		self
	}

	/// Build the provider. Light-client resources start lazily on the first
	/// light-client connect.
	pub fn build(self) -> EmbeddedChainProvider {
		EmbeddedChainProvider {
			chains: self.chains,
			#[cfg(feature = "smoldot")]
			relays: self.relays,
			#[cfg(feature = "smoldot")]
			seeded_databases: Mutex::new(self.seeded_databases),
			#[cfg(feature = "smoldot")]
			storage: self.storage,
			#[cfg(feature = "smoldot")]
			stored_quality: Mutex::new(HashMap::new()),
			#[cfg(feature = "smoldot")]
			light: crate::light::LightState::new(self.connection_types, self.keep_chains_running),
			#[cfg(feature = "networks")]
			catalog: self.catalog && self.stack.is_none(),
			#[cfg(feature = "smoldot")]
			stack: self.stack,
		}
	}
}

/// In-process [`ChainProvider`] whose per-chain backend is a remote WebSocket
/// JSON-RPC node (all targets) or an embedded smoldot light client (native
/// targets).
///
/// Construct **one provider per host process** and share it (behind an `Arc`)
/// with every consumer: the provider owns the single light-client instance,
/// so host-internal flows (domain resolution, statement store) and product
/// connections share sync, peers, and warm state, while each connection keeps
/// its own isolated JSON-RPC queue and response stream.
///
/// The `responses()` stream of returned connections is take-once: the first
/// call yields the live stream, later calls yield an ended stream.
///
/// A genesis nobody registered is resolved from the stack the provider was
/// built with, or from the bundled catalog on a provider built with
/// `catalog()`, which the `networks` feature adds.
pub struct EmbeddedChainProvider {
	chains: HashMap<[u8; 32], ChainSource>,
	/// The relay each explicitly-registered parachain syncs through; catalog
	/// parachains carry theirs in the catalog entry.
	#[cfg(feature = "smoldot")]
	relays: HashMap<[u8; 32], [u8; 32]>,
	/// Database contents waiting to seed a chain, whether registered explicitly
	/// or read back from the warm store. Behind a lock because `load_database` fills
	/// it after the provider is built, and only useful until the first
	/// add: smoldot ignores the blob on every add after that.
	#[cfg(feature = "smoldot")]
	seeded_databases: Mutex<HashMap<[u8; 32], String>>,
	#[cfg(feature = "smoldot")]
	storage: Option<std::sync::Arc<dyn crate::storage::StorageClient>>,
	/// Whether the blob this provider last wrote for a chain carried the runtime
	/// code. Saves reading an 8 MB blob back on every snapshot, at the cost of
	/// assuming nothing else writes the same key.
	#[cfg(feature = "smoldot")]
	stored_quality: Mutex<HashMap<[u8; 32], bool>>,
	#[cfg(feature = "smoldot")]
	light: crate::light::LightState,
	#[cfg(feature = "networks")]
	catalog: bool,
	#[cfg(feature = "smoldot")]
	stack: Option<crate::stack::LightStack>,
}

impl EmbeddedChainProvider {
	/// Start building a provider.
	pub fn builder() -> EmbeddedChainProviderBuilder {
		EmbeddedChainProviderBuilder::new()
	}

	/// The chains of the bundled network `genesis_hash` belongs to and the relay
	/// it syncs through, on a provider built with
	/// [`catalog`](EmbeddedChainProviderBuilder::catalog).
	#[cfg(feature = "networks")]
	fn catalog_network(&self, genesis_hash: [u8; 32]) -> Option<crate::networks::ResolvedNetwork> {
		self.catalog
			.then(|| crate::networks::catalog_network_chains(genesis_hash))
			.flatten()
	}

	/// Whether [`connect`](ChainProvider::connect) can reach `genesis_hash`.
	#[cfg(feature = "smoldot")]
	fn serves(&self, genesis_hash: [u8; 32]) -> bool {
		#[cfg(feature = "networks")]
		if self.catalog && crate::networks::catalog_service(genesis_hash).is_some() {
			return true;
		}
		self.chains.contains_key(&genesis_hash) ||
			self.stack.as_ref().is_some_and(|stack| stack.has_light_spec(genesis_hash))
	}

	/// What a connect to `genesis_hash` goes through: the chain registered for
	/// it, then the provider's stack, then the catalog.
	fn resolve(&self, genesis_hash: [u8; 32]) -> Result<ResolvedChain, ProviderError> {
		if let Some(source) = self.chains.get(&genesis_hash) {
			return Ok(ResolvedChain {
				source: source.clone(),
				#[cfg(feature = "smoldot")]
				relay: relay_source(&self.chains, source, self.relays.get(&genesis_hash).copied())?,
			});
		}
		#[cfg(feature = "smoldot")]
		if let Some(stack) = &self.stack {
			return stack.resolve(genesis_hash);
		}
		#[cfg(feature = "networks")]
		if let Some((catalog, relay)) = self.catalog_network(genesis_hash) {
			if let Some((network, service)) = crate::networks::catalog_service(genesis_hash) {
				tracing::info!(network, service, "connecting via light client");
			}
			let source = catalog
				.get(&genesis_hash)
				.expect("catalog_network_chains includes the queried genesis");
			return Ok(ResolvedChain {
				source: source.clone(),
				relay: relay_source(&catalog, source, relay)?,
			});
		}
		Err(ProviderError::UnknownGenesis { genesis: genesis_hash })
	}

	/// Apply a seeded warm-start database blob to `source` if one exists for
	/// `genesis_hash` and the source is a light client with no explicit blob.
	#[cfg(feature = "smoldot")]
	fn with_seeded_database(&self, genesis_hash: [u8; 32], mut source: ChainSource) -> ChainSource {
		// Taken, not cloned: a chain consumes a blob only on its first add, so
		// keeping it would retain megabytes for the life of the provider and
		// make every later connect re-clone a string smoldot then discards.
		// Irrefutable only in the native-bindings build, where ChainSource
		// has no ws variant.
		if let ChainSource::LightClient { database_content, .. } = &mut source &&
			database_content.is_none() &&
			let Some(blob) = lock(&self.seeded_databases).remove(&genesis_hash)
		{
			*database_content = Some(blob);
		}
		source
	}

	#[cfg(not(feature = "smoldot"))]
	fn with_seeded_database(&self, _genesis_hash: [u8; 32], source: ChainSource) -> ChainSource {
		source
	}
}

/// The relay `source` syncs through, looked up in `chains`. Only a
/// light-client chain has one.
#[cfg(feature = "smoldot")]
fn relay_source(
	chains: &HashMap<[u8; 32], ChainSource>,
	source: &ChainSource,
	relay: Option<[u8; 32]>,
) -> Result<Option<([u8; 32], ChainSource)>, ProviderError> {
	match (source, relay) {
		(ChainSource::LightClient { .. }, Some(relay)) => {
			let source = chains.get(&relay).ok_or(ProviderError::UnknownRelay { relay })?;
			Ok(Some((relay, source.clone())))
		},
		_ => Ok(None),
	}
}

/// Max size for a [`snapshot`](EmbeddedChainProvider::snapshot) database blob.
#[cfg(feature = "smoldot")]
const SNAPSHOT_MAX_BYTES: usize = 8_000_000;

/// How long [`EmbeddedChainProvider::snapshot`] waits for the light client to
/// answer. A chain that is still warp syncing has no finalized database to hand
/// over, and the response stream stays open either way, so the call has to end
/// on its own rather than wait on a stream that never yields.
#[cfg(feature = "smoldot")]
const SNAPSHOT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(60);

#[cfg(feature = "smoldot")]
impl EmbeddedChainProvider {
	/// Whether this provider was built with somewhere to keep blobs.
	pub fn has_storage(&self) -> bool {
		self.storage.is_some()
	}

	/// The store this provider keeps blobs in, or an error naming what is
	/// missing. Warm start is never skipped quietly: a host that meant to have
	/// it and did not configure one is told so.
	fn storage(&self) -> Result<std::sync::Arc<dyn crate::storage::StorageClient>, ProviderError> {
		self.storage.clone().ok_or(ProviderError::NoStorage)
	}

	/// Whether the embedded client is running this chain right now, whether a
	/// connection asked for it or a parachain brought it up as its relay.
	pub fn is_connected(&self, genesis_hash: [u8; 32]) -> bool {
		self.light.is_added(genesis_hash)
	}

	/// Watch what the embedded light client is doing on `genesis_hash`: the
	/// first item is the current state, then one item per change. The stream
	/// ends when the client stops running the chain.
	///
	/// Fails when the client is not running the chain, so connect first. A
	/// relay a parachain brought up counts as running, and is the chain whose
	/// warp sync progress a parachain waits on; see [`relay_of`](Self::relay_of).
	pub fn lifecycle(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<futures::stream::BoxStream<'static, crate::ChainLifecycle>, ProviderError> {
		use futures::stream::{self, StreamExt};

		let subscription = self
			.light
			.lifecycle(genesis_hash)
			.ok_or(ProviderError::NotRunning { genesis: genesis_hash })?;
		Ok(stream::unfold(subscription, |mut subscription| async move {
			let state = subscription.next().await?;
			Some((state.into(), subscription))
		})
		.fuse()
		.boxed())
	}

	/// The relay `genesis_hash` syncs through, from the registry, the stack or
	/// the catalog. A chain that is not a parachain has none.
	pub fn relay_of(&self, genesis_hash: [u8; 32]) -> Option<[u8; 32]> {
		if let Some(relay) = self.relays.get(&genesis_hash) {
			return Some(*relay);
		}
		if let Some(stack) = &self.stack {
			return stack.relay_of(genesis_hash);
		}
		#[cfg(feature = "networks")]
		if let Some((_, relay)) = self.catalog_network(genesis_hash) {
			return relay;
		}
		None
	}

	/// Hand over a light spec after the build, so a chain whose spec was not
	/// at hand then comes up without a new provider; see
	/// [`LightStack::add_light_spec`](crate::LightStack::add_light_spec).
	///
	/// Fails with [`ProviderError::NoStack`] on a provider built without a
	/// stack.
	pub fn add_light_spec(
		&self,
		genesis_hash: [u8; 32],
		specification: &[u8],
	) -> Result<(), ProviderError> {
		let stack = self.stack.as_ref().ok_or(ProviderError::NoStack)?;
		stack.add_light_spec(genesis_hash, specification)
	}

	/// Whether the stack's chain with `genesis_hash` has its light spec.
	pub fn has_light_spec(&self, genesis_hash: [u8; 32]) -> bool {
		self.stack.as_ref().is_some_and(|stack| stack.has_light_spec(genesis_hash))
	}

	/// Where the host fetches each light spec of the provider's stack; see
	/// [`LightStack::light_specs`](crate::LightStack::light_specs).
	pub fn light_specs(&self) -> Vec<crate::LightSpecSource> {
		self.stack.as_ref().map(|stack| stack.light_specs()).unwrap_or_default()
	}

	/// Put a seed back after a connect that never reached `add_chain`.
	///
	/// The blob is taken out of the map before connecting, so without this a
	/// transport failure would drop it and the retry would sync cold. The web
	/// path reloads before every connect and would recover anyway; a native
	/// host calls `load_database` once.
	#[cfg(feature = "smoldot")]
	fn return_seed(&self, genesis_hash: [u8; 32], source: ChainSource) {
		if let ChainSource::LightClient { database_content: Some(blob), .. } = source {
			lock(&self.seeded_databases).entry(genesis_hash).or_insert(blob);
		}
	}

	/// Whether a blob is already in hand for `genesis_hash`.
	fn has_database(&self, genesis_hash: [u8; 32]) -> bool {
		lock(&self.seeded_databases).contains_key(&genesis_hash)
	}

	/// Read the stored blob for `genesis_hash` into this provider, so the next
	/// connect to that chain resumes from it instead of warp syncing from the
	/// chain-spec checkpoint. Returns whether a blob is now in hand.
	///
	/// The store is read at most once per chain. smoldot keys a chain by its
	/// genesis hash, so only the first add of a chain consumes a blob and a
	/// later read could not take effect. A blob registered through
	/// [`EmbeddedChainProviderBuilder::database`] wins and is not overwritten.
	///
	/// Call this before [`connect`](crate::platform::ChainProvider::connect),
	/// not from inside a connection callback: on the native bindings `connect`
	/// blocks the calling thread, and a store that needs the main thread would
	/// deadlock underneath it.
	///
	/// Fails when a read is needed and the provider was built without storage.
	/// A chain that silently never resumes is indistinguishable from one that
	/// has nothing stored yet, so missing configuration is reported rather than
	/// swallowed. For the same reason a chain this provider cannot connect to
	/// fails as unknown, as [`connect`](crate::platform::ChainProvider::connect)
	/// would. A chain that is already running, or already holds a blob, answers
	/// without consulting storage at all.
	pub async fn load_database(&self, genesis_hash: [u8; 32]) -> Result<bool, ProviderError> {
		if !self.serves(genesis_hash) {
			return Err(ProviderError::UnknownGenesis { genesis: genesis_hash });
		}
		// A parachain is only as cold as the relay under it, and the relay is
		// the one that warp syncs, so it is seeded first.
		if let Some(relay) = self.relay_of(genesis_hash) &&
			!self.is_connected(relay) &&
			!self.has_database(relay) &&
			let Some(store) = self.storage.clone() &&
			let Some(blob) = store.load(relay).await?
		{
			lock(&self.seeded_databases).entry(relay).or_insert(blob);
		}
		if self.is_connected(genesis_hash) {
			// smoldot keys a chain by its genesis hash and discards the blob on
			// every add after the first, so a seed arriving now cannot take
			// effect and the chain would silently stay cold.
			tracing::warn!(
				genesis = %hex::encode(genesis_hash),
				"load_database ran after the chain was already connected, so the stored blob cannot take effect"
			);
			return Ok(false);
		}
		if self.has_database(genesis_hash) {
			return Ok(true);
		}
		let store = self.storage()?;
		let Some(blob) = store.load(genesis_hash).await? else {
			return Ok(false);
		};
		lock(&self.seeded_databases).entry(genesis_hash).or_insert(blob);
		Ok(true)
	}

	/// Snapshot the finalized state for `genesis_hash` and hand it to storage.
	/// Returns whether a blob was stored.
	///
	/// A chain that has finalized nothing yet still answers the snapshot
	/// request, with a blob carrying no chain information that smoldot would
	/// discard on the next run. Storing it would turn a warm start back into a
	/// cold one, so it is skipped instead.
	///
	/// This is a full round trip against the light client, not a write: call it
	/// while the app is alive, and treat a call from a teardown callback as
	/// best effort, since neither a hidden page nor a backgrounded app is
	/// guaranteed to stay scheduled long enough to finish it.
	///
	/// Fails when the provider was built without a store, for the same reason
	/// [`load_database`](Self::load_database) does.
	pub async fn save_database(&self, genesis_hash: [u8; 32]) -> Result<bool, ProviderError> {
		// Checked before the snapshot, so a provider with nowhere to write
		// fails without paying for a round trip through the light client.
		self.storage()?;
		if !self.is_connected(genesis_hash) {
			// `snapshot` opens its own connection, so persisting a chain this
			// provider never connected would start one, sync it from the
			// checkpoint and store nothing worth having.
			tracing::debug!(
				genesis = %hex::encode(genesis_hash),
				"nothing to store: this provider has not connected to the chain"
			);
			return Ok(false);
		}
		let blob = self.snapshot(genesis_hash).await?;
		self.store_blob(genesis_hash, blob).await
	}

	/// Decide whether `blob` improves on what is stored, and write it if so.
	///
	/// Split from [`save_database`](Self::save_database) so the decision can be
	/// driven with a chosen blob. Reaching it through `save_database` needs a
	/// live chain to snapshot, which leaves no way to present the case this
	/// exists for: a snapshot that lost the runtime code arriving over a stored
	/// blob that still has it.
	#[cfg(feature = "smoldot")]
	async fn store_blob(
		&self,
		genesis_hash: [u8; 32],
		blob: String,
	) -> Result<bool, ProviderError> {
		let store = self.storage()?;
		// A blob carrying the runtime code is never worse than what is stored,
		// so it needs no comparison. Only the weaker case reads what is there,
		// and only until this provider has written once and knows what it left.
		let has_code = crate::storage::carries_runtime_code(&blob);
		let stored_has_code = if has_code {
			None
		} else {
			// Bound before the await so the lock guard is not held across it.
			let known = lock(&self.stored_quality).get(&genesis_hash).copied();
			match known {
				Some(known) => Some(known),
				None => Some(
					store
						.load(genesis_hash)
						.await?
						.is_some_and(|stored| crate::storage::carries_runtime_code(&stored)),
				),
			}
		};
		if !crate::storage::is_worth_storing(&blob, stored_has_code) {
			tracing::debug!(
				genesis = %hex::encode(genesis_hash),
				"nothing better than what is already stored"
			);
			return Ok(false);
		}
		// The blob is not kept in `seeded_databases`: a chain only consumes one
		// on its first add, and this chain is live, so nothing could read it
		// back before the process ends.
		store.save(genesis_hash, blob).await?;
		lock(&self.stored_quality).insert(genesis_hash, has_code);
		Ok(true)
	}

	/// Produce a warm-start database blob for `genesis_hash` by asking the
	/// embedded light client for its finalized-database snapshot.
	///
	/// Persist the returned string and feed it back on a later run via
	/// [`EmbeddedChainProviderBuilder::database`] so the chain resumes from
	/// finalized state instead of re-syncing from the checkpoint.
	///
	/// Meaningful only for light-client chains. A remote node has no local
	/// database to snapshot and answers the request with a JSON-RPC error,
	/// which surfaces here as an error rather than a wait.
	pub async fn snapshot(&self, genesis_hash: [u8; 32]) -> Result<String, ProviderError> {
		use futures::{
			future::{self, Either},
			stream::StreamExt,
		};

		use crate::error::FrameForId;

		let connection = self.connect(genesis_hash).await?;
		let mut responses = connection.responses();
		let id = "truapi-provider:finalizedDatabase";
		connection.send(format!(
			concat!(
				r#"{{"jsonrpc":"2.0","id":"{}","#,
				r#""method":"chainHead_unstable_finalizedDatabase","params":[{}]}}"#
			),
			id, SNAPSHOT_MAX_BYTES,
		));
		let deadline = futures_timer::Delay::new(SNAPSHOT_TIMEOUT);
		futures::pin_mut!(deadline);

		loop {
			match future::select(responses.next(), deadline.as_mut()).await {
				Either::Left((Some(frame), _)) => match crate::error::frame_for_id(&frame, id) {
					Some(FrameForId::Result(result)) => {
						connection.close();
						return Ok(result);
					},
					Some(FrameForId::Failure(reason)) => {
						connection.close();
						return Err(ProviderError::Transport {
							reason: format!("finalized-database snapshot failed: {reason}"),
						});
					},
					None => {},
				},
				Either::Left((None, _)) => {
					connection.close();
					return Err(ProviderError::Transport {
						reason: "connection ended before the finalized-database snapshot"
							.to_owned(),
					});
				},
				Either::Right(((), _)) => {
					connection.close();
					return Err(ProviderError::Transport {
						reason: format!(
							"no finalized-database snapshot within {}s",
							SNAPSHOT_TIMEOUT.as_secs()
						),
					});
				},
			}
		}
	}
}

#[cfg(all(test, feature = "smoldot"))]
impl EmbeddedChainProvider {
	/// Number of implicit relay chains the shared light client currently holds.
	pub fn relay_count(&self) -> usize {
		self.light.relay_count()
	}
}

#[async_trait::async_trait]
impl ChainProvider for EmbeddedChainProvider {
	#[tracing::instrument(skip_all, fields(genesis = %hex::encode(genesis_hash)))]
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let resolved = self.resolve(genesis_hash)?;
		match self.with_seeded_database(genesis_hash, resolved.source) {
			#[cfg(feature = "ws")]
			ChainSource::RpcNode { url } => crate::ws::connect(url).await,
			#[cfg(feature = "smoldot")]
			source @ ChainSource::LightClient { .. } => {
				let relay = resolved
					.relay
					.map(|(genesis, source)| (genesis, self.with_seeded_database(genesis, source)));
				let connected = self.light.connect(genesis_hash, &source, relay.as_ref()).await;
				if connected.is_err() {
					self.return_seed(genesis_hash, source);
					// A relay that is running took its seed on the add that
					// started it.
					if let Some((genesis, source)) = relay &&
						!self.light.is_added(genesis)
					{
						self.return_seed(genesis, source);
					}
				}
				connected
			},
		}
	}
}

#[cfg(test)]
mod tests;
