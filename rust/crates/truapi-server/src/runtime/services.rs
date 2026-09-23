//! Role-neutral runtime services shared by product-facing runtimes.
//!
//! This module owns only infrastructure that is valid for both pairing hosts
//! and signing hosts. Pairing state, signing state, active sessions, and role
//! controls live on the concrete role objects.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure};
use crate::host_logic::worker::WorkerLedger;
use crate::runtime::bulletin_rpc::BulletinRpc;
use crate::runtime::statement_store_rpc::StatementStoreRpc;
use crate::subscription::Spawner;
use async_trait::async_trait;
use truapi::latest;
use truapi_platform::{
    CoinageWalletHost, HostInfo, JsonRpcConnection, PermissionStatusHost, Platform,
};

/// Upper bound on the in-core preimage cache. The cache is a bridge until
/// content propagates to the lookup backend, not a store, so it stays small.
const PREIMAGE_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Upper bound on accepted statements retained while the remote Statement
/// Store catches up.
const STATEMENT_CACHE_MAX_ENTRIES: usize = 64;

/// Infrastructure shared by all product runtimes created from one host role.
pub(crate) struct RuntimeServices {
    /// Host platform backing all syscalls.
    pub(crate) platform: Arc<dyn Platform>,
    /// Host identity reported to products via `System::host_info`.
    pub(crate) host_info: HostInfo,
    /// Host chat adapter, when the host serves the Chat capability. `None`
    /// makes every product chat call resolve as `Unsupported`.
    pub(crate) chat_platform: Option<Arc<dyn truapi_platform::ChatPlatform>>,
    /// Native wallet custody, fixed at construction. Absence uses Rust; an
    /// injected service remains the owner even while unavailable.
    pub(crate) native_wallet: Option<Arc<dyn CoinageWalletHost>>,
    /// Host adapter reporting live OS permission state, installed once at
    /// startup by a host that can read it. Unset leaves device grants
    /// resolving from stored state alone.
    permission_status: OnceLock<Arc<dyn PermissionStatusHost>>,
    /// Host Pocket adapter, installed once at startup by a host with a Pocket
    /// surface. Unset leaves every product Pocket call `Unsupported`.
    pocket_platform: OnceLock<Arc<dyn truapi_platform::PocketPlatform>>,
    /// Optional native authenticated username index; only supplies candidates.
    identity_backend: OnceLock<Arc<dyn truapi_platform::IdentityBackendHost>>,
    /// Asset Hub the dotNS contracts are deployed on. All-zero says this host
    /// has none, which leaves every manifest unresolvable.
    asset_hub_chain_genesis_hash: [u8; 32],
    /// Reference counts per product worker.
    pub(crate) worker_ledger: WorkerLedger,
    /// Shared chainHead-v1 runtime behind the Chain surface.
    pub(crate) chain: ChainRuntime,
    /// Configured People chain for native Chat identity and main-purse Coinage.
    pub(crate) people_chain_genesis_hash: [u8; 32],
    /// People-chain statement store RPC client.
    pub(crate) statement_store: StatementStoreRpc,
    /// In-core Bulletin submission over the configured Bulletin chain.
    pub(crate) bulletin: BulletinRpc,
    /// Runtime metadata and chain state shared by allowance paths, per chain.
    pub(crate) chain_context: crate::runtime::statement_allowance::ChainContextCache,
    /// Values from confirmed in-core submissions, served to `lookup_subscribe`
    /// until the host's content backend has them. Byte-bounded, oldest-first.
    preimage_cache: Mutex<PreimageCache>,
    /// Confirmed submissions served to new subscriptions until the remote
    /// Statement Store reports them.
    statement_cache: Mutex<StatementCache>,
    /// Task spawner for background runtime work.
    pub(crate) spawner: Spawner,
    /// Serializes the read-or-create of the persisted device encryption key.
    /// Concurrent first-time readers would otherwise each generate a secret and
    /// persist it, leaving peers addressing an overwritten key.
    device_encryption_key: futures::lock::Mutex<()>,
    next_core_instance: AtomicU64,
}

impl RuntimeServices {
    /// Build role-neutral runtime services from the platform, the host
    /// identity reported to products, the People-chain genesis hash used by
    /// statement-store backed protocols, the Bulletin-chain genesis hash used
    /// for in-core preimage submission, and the Asset Hub genesis hash product
    /// manifests are resolved from.
    ///
    /// The three genesis hashes are adjacent and same-typed, so a transposition
    /// compiles. Each call site is pinned by its own test.
    pub(crate) fn new(
        platform: Arc<dyn Platform>,
        host_info: HostInfo,
        people_chain_genesis_hash: [u8; 32],
        bulletin_chain_genesis_hash: [u8; 32],
        asset_hub_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
    ) -> Arc<Self> {
        Self::with_chat_platform(
            platform,
            host_info,
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
            asset_hub_chain_genesis_hash,
            spawner,
            None,
            None,
        )
    }

    /// Same as [`Self::new`], with optional Chat and native wallet dependencies.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_chat_platform(
        platform: Arc<dyn Platform>,
        host_info: HostInfo,
        people_chain_genesis_hash: [u8; 32],
        bulletin_chain_genesis_hash: [u8; 32],
        asset_hub_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
        chat_platform: Option<Arc<dyn truapi_platform::ChatPlatform>>,
        native_wallet: Option<Arc<dyn CoinageWalletHost>>,
    ) -> Arc<Self> {
        let chain_provider = Arc::new(HostChainProvider {
            platform: platform.clone(),
        });
        let chain = ChainRuntime::new(chain_provider, spawner.clone());
        let statement_store =
            StatementStoreRpc::new(platform.clone(), people_chain_genesis_hash, spawner.clone());
        let bulletin = BulletinRpc::new(chain.clone(), bulletin_chain_genesis_hash);
        Arc::new(Self {
            platform,
            host_info,
            chat_platform,
            native_wallet,
            permission_status: OnceLock::new(),
            pocket_platform: OnceLock::new(),
            identity_backend: OnceLock::new(),
            asset_hub_chain_genesis_hash,
            worker_ledger: WorkerLedger::default(),
            chain,
            people_chain_genesis_hash,
            statement_store,
            bulletin,
            chain_context: crate::runtime::statement_allowance::ChainContextCache::default(),
            preimage_cache: Mutex::new(PreimageCache::default()),
            statement_cache: Mutex::new(StatementCache::default()),
            spawner,
            device_encryption_key: futures::lock::Mutex::new(()),
            next_core_instance: AtomicU64::new(1),
        })
    }

    /// Install the host's live OS permission-status adapter.
    ///
    /// Set-once, so a capability cannot be swapped out from under a running
    /// product. Returns whether this call installed it.
    pub(crate) fn install_permission_status_host(
        &self,
        host: Arc<dyn PermissionStatusHost>,
    ) -> bool {
        self.permission_status.set(host).is_ok()
    }

    pub(crate) fn install_identity_backend_host(
        &self,
        host: Arc<dyn truapi_platform::IdentityBackendHost>,
    ) -> bool {
        self.identity_backend.set(host).is_ok()
    }

    pub(crate) fn identity_backend_host(
        &self,
    ) -> Option<Arc<dyn truapi_platform::IdentityBackendHost>> {
        self.identity_backend.get().cloned()
    }

    /// The Asset Hub dotNS reads run against, when one is configured.
    ///
    /// Taken by construction, so the chain a grant is adjudicated against
    /// cannot move under a running product. It is deliberately not sourced from
    /// `supported_chains()`, which is an uncached per-call host syscall
    /// answering a different question, "which chains do I serve RPC for?", the
    /// product-facing `get_chain_info` advertisement, rather than "which Asset
    /// Hub is dotNS deployed on?". Taking it from there would let the anchor
    /// change between two calls, at host discretion, with nothing recording it.
    ///
    /// An all-zero hash is how a host says it has no Asset Hub. `None` fails
    /// every manifest lookup closed: grants are refused rather than assumed.
    pub(crate) fn asset_hub_chain_genesis_hash(&self) -> Option<[u8; 32]> {
        Some(self.asset_hub_chain_genesis_hash).filter(|hash| *hash != [0u8; 32])
    }

    /// The host's live OS permission-status adapter, when one is installed.
    pub(crate) fn permission_status_host(&self) -> Option<Arc<dyn PermissionStatusHost>> {
        self.permission_status.get().cloned()
    }

    /// Install the host's Pocket adapter.
    ///
    /// Set-once, like every optional capability, so the card collection cannot
    /// change hands under a running product. Returns whether this call
    /// installed it.
    pub(crate) fn install_pocket_platform(
        &self,
        platform: Arc<dyn truapi_platform::PocketPlatform>,
    ) -> bool {
        self.pocket_platform.set(platform).is_ok()
    }

    /// The host's Pocket adapter, when one is installed.
    pub(crate) fn pocket_platform(&self) -> Option<Arc<dyn truapi_platform::PocketPlatform>> {
        self.pocket_platform.get().cloned()
    }

    /// This device's persisted X25519 encryption secret, created on first use.
    ///
    /// The only supported way to reach the key: it bundles the serialization
    /// guard with the read, so no caller can race another into generating a
    /// second secret and overwriting the one peers were told to address.
    pub(crate) async fn device_encryption_secret(&self) -> Result<[u8; 32], String> {
        let _guard = self.device_encryption_key.lock().await;
        crate::host_logic::device_key::read_or_create_device_encryption_secret(
            self.platform.as_ref(),
        )
        .await
    }

    /// Allocate the next per-product-runtime id, used to scope chain follow
    /// operation ids within the shared host runtime.
    pub(crate) fn next_core_instance(&self) -> u64 {
        self.next_core_instance.fetch_add(1, Ordering::Relaxed)
    }

    /// Store a preimage value under its key for later lookup hits.
    pub(crate) fn cache_preimage(&self, key: [u8; 32], value: Vec<u8>) {
        self.preimage_cache
            .lock()
            .expect("preimage cache mutex poisoned")
            .insert(key, value);
    }

    /// Return a cached preimage value for `key`, if present.
    pub(crate) fn cached_preimage(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
        self.preimage_cache
            .lock()
            .expect("preimage cache mutex poisoned")
            .get(key)
    }

    /// Retain a remotely accepted statement for read-after-write consistency.
    pub(crate) fn cache_statement(&self, statement: latest::SignedStatement) {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .insert(statement);
    }

    /// Return accepted statements matching a Statement Store topic filter.
    pub(crate) fn cached_statements(
        &self,
        kind: crate::host_logic::statement_store::TopicFilterKind,
        topics: &[[u8; 32]],
    ) -> Vec<latest::SignedStatement> {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .matching(kind, topics)
    }

    /// Drop bridge entries once a remote subscription reports them.
    pub(crate) fn mark_statements_visible(&self, statements: &[latest::SignedStatement]) {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .remove_all(statements);
    }
}

/// Byte-bounded, insertion-ordered preimage cache.
#[derive(Default)]
struct PreimageCache {
    entries: VecDeque<([u8; 32], Vec<u8>)>,
    total_bytes: usize,
}

impl PreimageCache {
    fn insert(&mut self, key: [u8; 32], value: Vec<u8>) {
        if value.len() > PREIMAGE_CACHE_MAX_BYTES {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|(existing, _)| *existing == key)
        {
            let (_, old) = self.entries.remove(index).expect("index in range");
            self.total_bytes -= old.len();
        }
        self.total_bytes += value.len();
        self.entries.push_back((key, value));
        while self.total_bytes > PREIMAGE_CACHE_MAX_BYTES {
            let Some((_, evicted)) = self.entries.pop_front() else {
                break;
            };
            self.total_bytes -= evicted.len();
        }
    }

    fn get(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value.clone())
    }
}

/// Entry-bounded, insertion-ordered Statement Store propagation bridge.
#[derive(Default)]
struct StatementCache {
    entries: VecDeque<latest::SignedStatement>,
}

impl StatementCache {
    fn insert(&mut self, statement: latest::SignedStatement) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|existing| existing == &statement)
        {
            self.entries.remove(index);
        }
        self.entries.push_back(statement);
        while self.entries.len() > STATEMENT_CACHE_MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    fn matching(
        &self,
        kind: crate::host_logic::statement_store::TopicFilterKind,
        topics: &[[u8; 32]],
    ) -> Vec<latest::SignedStatement> {
        self.entries
            .iter()
            .filter(|statement| match kind {
                crate::host_logic::statement_store::TopicFilterKind::MatchAll => {
                    topics.iter().all(|topic| statement.topics.contains(topic))
                }
                crate::host_logic::statement_store::TopicFilterKind::MatchAny => {
                    topics.iter().any(|topic| statement.topics.contains(topic))
                }
            })
            .cloned()
            .collect()
    }

    fn remove_all(&mut self, statements: &[latest::SignedStatement]) {
        self.entries
            .retain(|cached| !statements.iter().any(|visible| visible == cached));
    }
}

/// Adapter from `truapi_platform::ChainProvider` into the
/// [`RuntimeChainProvider`] surface the chain runtime expects.
struct HostChainProvider {
    platform: Arc<dyn Platform>,
}

#[async_trait]
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
        self.platform
            .connect(genesis_hash)
            .await
            .map(Arc::from)
            .map_err(|err| {
                RuntimeFailure::unavailable_with_reason("remote_chain_connect", format!("{err:?}"))
            })
    }
}
