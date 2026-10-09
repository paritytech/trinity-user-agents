//! Genesis-hash registry dispatching each connection to its backend.
//!
//! A parachain's relay is provider topology (see `relays`), not part of
//! [`ChainSource`]; the light backend brings the relay up behind the parachain.

use std::collections::HashMap;
#[cfg(feature = "smoldot")]
use std::sync::Mutex;

use crate::platform::{ChainProvider, JsonRpcConnection};

use crate::config::ChainSource;
use crate::error::ProviderError;

/// Lock a mutex, recovering the guard if a previous holder panicked.
#[cfg(feature = "smoldot")]
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            .field("connection_types", &self.connection_types);
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
    ///
    /// The catalog is the only caller outside tests, so a build without it has
    /// no parachain to register.
    #[cfg(all(feature = "smoldot", any(feature = "networks", test)))]
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
            light: crate::light::LightState::new(self.connection_types),
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
}

impl EmbeddedChainProvider {
    /// Start building a provider.
    pub fn builder() -> EmbeddedChainProviderBuilder {
        EmbeddedChainProviderBuilder::new()
    }

    /// Open a connection for `source`; for a parachain, `relay`/`chains` give
    /// the light backend the relay to sync it through.
    ///
    /// The relay's source is resolved and warm-start-seeded here, so a blob
    /// registered for a relay applies whether it is connected directly or
    /// brought up implicitly behind one of its parachains.
    #[cfg_attr(not(feature = "smoldot"), allow(unused_variables))]
    async fn connect_source(
        &self,
        genesis_hash: [u8; 32],
        source: &ChainSource,
        chains: &HashMap<[u8; 32], ChainSource>,
        relay: Option<[u8; 32]>,
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        match source {
            #[cfg(feature = "ws")]
            ChainSource::RpcNode { url } => crate::ws::connect(url.clone()).await,
            #[cfg(feature = "smoldot")]
            ChainSource::LightClient { .. } => {
                let relay = match relay {
                    None => None,
                    Some(relay_genesis) => Some(self.resolve_relay(chains, relay_genesis)?),
                };
                self.light.connect(genesis_hash, source, relay).await
            }
        }
    }

    /// Resolve a parachain's relay to the source the light backend should add
    /// it under, carrying any warm-start blob registered for the relay.
    #[cfg(feature = "smoldot")]
    fn resolve_relay(
        &self,
        chains: &HashMap<[u8; 32], ChainSource>,
        relay_genesis: [u8; 32],
    ) -> Result<([u8; 32], ChainSource), ProviderError> {
        let source = chains
            .get(&relay_genesis)
            .ok_or(ProviderError::UnknownRelay {
                relay: relay_genesis,
            })?;
        Ok((
            relay_genesis,
            self.with_seeded_database(relay_genesis, source.clone()),
        ))
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
        #[allow(irrefutable_let_patterns)]
        if let ChainSource::LightClient {
            database_content, ..
        } = &mut source
            && database_content.is_none()
            && let Some(blob) = lock(&self.seeded_databases).remove(&genesis_hash)
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
            .ok_or(ProviderError::NotRunning {
                genesis: genesis_hash,
            })?;
        Ok(stream::unfold(subscription, |mut subscription| async move {
            let state = subscription.next().await?;
            Some((state.into(), subscription))
        })
        .fuse()
        .boxed())
    }

    /// The relay `genesis_hash` syncs through, from the registry or the
    /// catalog. A chain that is not a parachain has none.
    pub fn relay_of(&self, genesis_hash: [u8; 32]) -> Option<[u8; 32]> {
        if let Some(relay) = self.relays.get(&genesis_hash) {
            return Some(*relay);
        }
        #[cfg(feature = "networks")]
        if let Some((_, relay)) = crate::networks::catalog_network_chains(genesis_hash) {
            return relay;
        }
        None
    }

    /// Put a seed back after a connect that never reached `add_chain`.
    ///
    /// The blob is taken out of the map before connecting, so without this a
    /// transport failure would drop it and the retry would sync cold. The web
    /// path reloads before every connect and would recover anyway; a native
    /// host calls `load_database` once.
    #[cfg(feature = "smoldot")]
    fn return_seed(&self, genesis_hash: [u8; 32], source: ChainSource) {
        if let ChainSource::LightClient {
            database_content: Some(blob),
            ..
        } = source
        {
            lock(&self.seeded_databases)
                .entry(genesis_hash)
                .or_insert(blob);
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
    /// swallowed. A chain that is already running, or already holds a blob,
    /// answers without consulting storage at all.
    pub async fn load_database(&self, genesis_hash: [u8; 32]) -> Result<bool, ProviderError> {
        // A parachain is only as cold as the relay under it, and the relay is
        // the one that warp syncs, so it is seeded first.
        if let Some(relay) = self.relay_of(genesis_hash)
            && !self.is_connected(relay)
            && !self.has_database(relay)
            && let Some(store) = self.storage.clone()
            && let Some(blob) = store.load(relay).await?
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
        lock(&self.seeded_databases)
            .entry(genesis_hash)
            .or_insert(blob);
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
        use futures::future::{self, Either};
        use futures::stream::StreamExt;

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
                    }
                    Some(FrameForId::Failure(reason)) => {
                        connection.close();
                        return Err(ProviderError::Transport {
                            reason: format!("finalized-database snapshot failed: {reason}"),
                        });
                    }
                    None => {}
                },
                Either::Left((None, _)) => {
                    connection.close();
                    return Err(ProviderError::Transport {
                        reason: "connection ended before the finalized-database snapshot"
                            .to_owned(),
                    });
                }
                Either::Right(((), _)) => {
                    connection.close();
                    return Err(ProviderError::Transport {
                        reason: format!(
                            "no finalized-database snapshot within {}s",
                            SNAPSHOT_TIMEOUT.as_secs()
                        ),
                    });
                }
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
        // Explicit registrations win; otherwise the catalog resolves the whole
        // network from the genesis hash alone.
        if let Some(source) = self.chains.get(&genesis_hash) {
            let source = self.with_seeded_database(genesis_hash, source.clone());
            #[cfg(feature = "smoldot")]
            let relay = self.relays.get(&genesis_hash).copied();
            #[cfg(not(feature = "smoldot"))]
            let relay = None;
            let connected = self
                .connect_source(genesis_hash, &source, &self.chains, relay)
                .await;
            #[cfg(feature = "smoldot")]
            if connected.is_err() {
                self.return_seed(genesis_hash, source);
            }
            return connected;
        }
        #[cfg(feature = "networks")]
        if let Some((catalog, relay)) = crate::networks::catalog_network_chains(genesis_hash) {
            if let Some((network, service)) = crate::networks::catalog_service(genesis_hash) {
                tracing::info!(network, service, "connecting via light client");
            }
            let source = catalog
                .get(&genesis_hash)
                .expect("catalog_network_chains includes the queried genesis")
                .clone();
            let source = self.with_seeded_database(genesis_hash, source);
            let connected = self
                .connect_source(genesis_hash, &source, &catalog, relay)
                .await;
            if connected.is_err() {
                self.return_seed(genesis_hash, source);
            }
            return connected;
        }
        Err(ProviderError::UnknownGenesis {
            genesis: genesis_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::platform::ChainProvider;

    use super::EmbeddedChainProvider;
    use crate::config::ChainSource;

    #[test]
    fn unknown_genesis_is_an_error_naming_the_hash() {
        let provider = EmbeddedChainProvider::builder().build();
        let error = futures::executor::block_on(provider.connect([0xab; 32]))
            .err()
            .expect("connect must fail for an unregistered genesis");
        assert!(error.to_string().contains(&"ab".repeat(32)));
    }

    /// A blob registered for a relay reaches it even when the relay is never
    /// connected directly, only brought up behind one of its parachains.
    #[cfg(feature = "smoldot")]
    #[test]
    // Without the `ws` backend the enum has a single variant, making the
    // let-else below irrefutable there.
    #[allow(irrefutable_let_patterns)]
    fn a_relay_blob_seeds_the_relay_brought_up_behind_a_parachain() {
        use std::collections::HashMap;

        const RELAY: [u8; 32] = [1; 32];
        let mut chains = HashMap::new();
        chains.insert(RELAY, ChainSource::light_client("{}").build());
        let provider = EmbeddedChainProvider::builder()
            .database(RELAY, "relay-blob".to_owned())
            .build();

        let (genesis, source) = provider
            .resolve_relay(&chains, RELAY)
            .expect("the relay is registered");
        assert_eq!(genesis, RELAY);
        let ChainSource::LightClient {
            database_content, ..
        } = source
        else {
            panic!("expected a LightClient source");
        };
        assert_eq!(database_content.as_deref(), Some("relay-blob"));
    }

    /// The snapshot round trip resolves against a live light client, and the
    /// blob it returns is accepted back as a warm-start seed.
    ///
    /// `chainHead_unstable_finalizedDatabase` first asks the runtime service for
    /// the finalized runtime's storage merkle values, and that answer waits on
    /// smoldot's sync-mode decision, which no reachable peer will ever settle
    /// here. The test platform collapses that deadline; see
    /// [`crate::light_platform_test`].
    #[cfg(feature = "smoldot")]
    #[test]
    fn snapshot_round_trips_into_a_seed() {
        const GENESIS: [u8; 32] = [1; 32];
        const SPEC: &str = include_str!("../tests/fixtures/paseo.json");
        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client(SPEC).build())
            .build();
        let blob = futures::executor::block_on(provider.snapshot(GENESIS))
            .expect("the light client answers the finalized-database request");
        assert!(blob.contains("genesisHash"), "unexpected blob: {blob}");

        let seeded = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client(SPEC).build())
            .database(GENESIS, blob)
            .build();
        futures::executor::block_on(seeded.connect(GENESIS))
            .expect("a chain seeded with its own snapshot connects");
    }

    /// A store answering from memory, recording what it was asked to keep.
    #[cfg(feature = "smoldot")]
    #[derive(Default)]
    struct MemoryStorageClient {
        stored: std::sync::Mutex<std::collections::HashMap<[u8; 32], String>>,
        loads: std::sync::atomic::AtomicUsize,
    }

    #[cfg(feature = "smoldot")]
    #[async_trait::async_trait]
    impl crate::storage::StorageClient for MemoryStorageClient {
        async fn load(
            &self,
            genesis_hash: [u8; 32],
        ) -> Result<Option<String>, crate::storage::StorageClientError> {
            self.loads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(self
                .stored
                .lock()
                .expect("test store")
                .get(&genesis_hash)
                .cloned())
        }

        async fn save(
            &self,
            genesis_hash: [u8; 32],
            blob: String,
        ) -> Result<(), crate::storage::StorageClientError> {
            self.stored
                .lock()
                .expect("test store")
                .insert(genesis_hash, blob);
            Ok(())
        }
    }

    /// A stored blob reaches the chain source the light backend is handed.
    #[cfg(feature = "smoldot")]
    #[test]
    #[allow(irrefutable_let_patterns)]
    fn a_stored_blob_seeds_a_chain() {
        const GENESIS: [u8; 32] = [3; 32];
        let store = std::sync::Arc::new(MemoryStorageClient::default());
        store
            .stored
            .lock()
            .expect("test store")
            .insert(GENESIS, "stored-blob".to_owned());

        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(store.clone())
            .build();

        assert!(
            futures::executor::block_on(provider.load_database(GENESIS))
                .expect("the store answers"),
            "a stored blob is reported as in hand"
        );

        futures::executor::block_on(provider.load_database(GENESIS)).expect("the store answers");
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "a blob already in hand is not read again"
        );

        let seeded =
            provider.with_seeded_database(GENESIS, ChainSource::light_client("{}").build());
        let ChainSource::LightClient {
            database_content, ..
        } = seeded
        else {
            panic!("expected a LightClient source");
        };
        assert_eq!(database_content.as_deref(), Some("stored-blob"));
        assert!(
            !provider.has_database(GENESIS),
            "the blob is taken by the chain that consumed it rather than retained"
        );
    }

    /// An explicitly registered blob is not replaced by the stored one.
    #[cfg(feature = "smoldot")]
    #[test]
    fn an_explicit_blob_wins_over_the_store() {
        const GENESIS: [u8; 32] = [4; 32];
        let store = std::sync::Arc::new(MemoryStorageClient::default());
        store
            .stored
            .lock()
            .expect("test store")
            .insert(GENESIS, "stored-blob".to_owned());

        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .database(GENESIS, "explicit-blob".to_owned())
            .storage(store.clone())
            .build();

        assert!(
            futures::executor::block_on(provider.load_database(GENESIS)).expect("no store read")
        );
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "an explicit blob means the store is never consulted"
        );
    }

    /// Persisting a chain nobody connected would start one just to snapshot it.
    #[cfg(feature = "smoldot")]
    #[test]
    fn persist_without_a_connection_stores_nothing() {
        const GENESIS: [u8; 32] = [7; 32];
        let store = std::sync::Arc::new(MemoryStorageClient::default());
        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(store.clone())
            .build();

        assert!(
            !futures::executor::block_on(provider.save_database(GENESIS))
                .expect("no snapshot attempted"),
            "a chain that was never connected has nothing to store"
        );
        assert!(
            store.stored.lock().expect("test store").is_empty(),
            "nothing is written for a chain that was never connected"
        );
    }

    /// A client implemented in Rust is driven by both database calls, which is
    /// the coverage the deleted file store used to give.
    #[cfg(feature = "smoldot")]
    #[test]
    // Without the `ws` backend the enum has a single variant, making the
    // let-else below irrefutable there.
    #[allow(irrefutable_let_patterns)]
    fn a_rust_client_is_driven_by_both_database_calls() {
        const GENESIS: [u8; 32] = [10; 32];
        let client = std::sync::Arc::new(MemoryStorageClient::default());
        client
            .stored
            .lock()
            .expect("test client")
            .insert(GENESIS, "stored-blob".to_owned());

        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(client.clone())
            .build();

        assert!(
            futures::executor::block_on(provider.load_database(GENESIS))
                .expect("the client answers")
        );
        assert_eq!(client.loads.load(std::sync::atomic::Ordering::Relaxed), 1);

        let seeded =
            provider.with_seeded_database(GENESIS, ChainSource::light_client("{}").build());
        let ChainSource::LightClient {
            database_content, ..
        } = seeded
        else {
            panic!("expected a LightClient source");
        };
        assert_eq!(
            database_content.as_deref(),
            Some("stored-blob"),
            "what the client returned is what the chain is seeded with"
        );
    }

    /// A snapshot that lost the runtime code must not replace a stored blob
    /// that still has it. Flipping the guard in `store_blob` makes this fail,
    /// which the unit test on `is_worth_storing` alone does not catch.
    #[cfg(feature = "smoldot")]
    #[test]
    fn a_snapshot_without_the_runtime_code_does_not_replace_one_with_it() {
        const GENESIS: [u8; 32] = [11; 32];
        const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;
        const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

        let client = std::sync::Arc::new(MemoryStorageClient::default());
        client
            .stored
            .lock()
            .expect("test client")
            .insert(GENESIS, WITH_CODE.to_owned());

        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(client.clone())
            .build();

        assert!(
            !futures::executor::block_on(provider.store_blob(GENESIS, WITHOUT_CODE.to_owned()))
                .expect("the client answers"),
            "a blob that lost the runtime code is not worth storing"
        );
        assert_eq!(
            client.stored.lock().expect("test client").get(&GENESIS),
            Some(&WITH_CODE.to_owned()),
            "the better blob is still there"
        );
    }

    /// A blob carrying the runtime code always wins, and does so without
    /// reading the stored one back.
    #[cfg(feature = "smoldot")]
    #[test]
    fn a_snapshot_with_the_runtime_code_is_stored_without_a_read() {
        const GENESIS: [u8; 32] = [12; 32];
        const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;

        let client = std::sync::Arc::new(MemoryStorageClient::default());
        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(client.clone())
            .build();

        assert!(
            futures::executor::block_on(provider.store_blob(GENESIS, WITH_CODE.to_owned()))
                .expect("the client accepts it")
        );
        assert_eq!(
            client.loads.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "a blob with the runtime code needs no comparison"
        );
        assert_eq!(
            client.stored.lock().expect("test client").get(&GENESIS),
            Some(&WITH_CODE.to_owned())
        );
    }

    /// A blob with nothing finalized is never stored, whatever is there.
    #[cfg(feature = "smoldot")]
    #[test]
    fn a_snapshot_with_no_chain_information_is_never_stored() {
        const GENESIS: [u8; 32] = [13; 32];

        let client = std::sync::Arc::new(MemoryStorageClient::default());
        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(client.clone())
            .build();

        assert!(
            !futures::executor::block_on(
                provider.store_blob(GENESIS, r#"{"genesisHash":"0x01"}"#.to_owned())
            )
            .expect("the client answers")
        );
        assert!(client.stored.lock().expect("test client").is_empty());
    }

    /// Once this provider has written, it knows what it left behind and stops
    /// reading the blob back on every snapshot.
    #[cfg(feature = "smoldot")]
    #[test]
    fn the_stored_quality_is_remembered_after_the_first_write() {
        const GENESIS: [u8; 32] = [14; 32];
        const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

        let client = std::sync::Arc::new(MemoryStorageClient::default());
        let provider = EmbeddedChainProvider::builder()
            .chain(GENESIS, ChainSource::light_client("{}").build())
            .storage(client.clone())
            .build();

        for _ in 0..3 {
            assert!(
                futures::executor::block_on(provider.store_blob(GENESIS, WITHOUT_CODE.to_owned()))
                    .expect("the client answers")
            );
        }
        assert_eq!(
            client.loads.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "only the first write has to look at what was already there"
        );
    }

    /// A provider with nowhere to keep blobs says so, rather than answering the
    /// same way a working client answers for a chain it has nothing stored for.
    #[cfg(feature = "smoldot")]
    #[test]
    fn load_database_without_storage_is_an_error_naming_what_is_missing() {
        let provider = EmbeddedChainProvider::builder()
            .chain([5; 32], ChainSource::light_client("{}").build())
            .build();
        let error = futures::executor::block_on(provider.load_database([5; 32]))
            .expect_err("a provider with no storage cannot load a database");
        assert!(error.to_string().contains("without storage"), "{error}");
    }

    #[cfg(feature = "ws")]
    #[test]
    // On wasm32 without the smoldot backend the enum has a single variant,
    // making the let-else irrefutable there.
    #[allow(irrefutable_let_patterns)]
    fn later_registration_wins() {
        let first = url::Url::parse("ws://first.example").expect("static URL parses");
        let second = url::Url::parse("ws://second.example").expect("static URL parses");
        let provider = EmbeddedChainProvider::builder()
            .chain([1; 32], ChainSource::rpc_node(first))
            .chain([1; 32], ChainSource::rpc_node(second.clone()))
            .build();
        let ChainSource::RpcNode { url } = &provider.chains[&[1; 32]] else {
            panic!("expected an RpcNode source");
        };
        assert_eq!(*url, second);
    }
}
