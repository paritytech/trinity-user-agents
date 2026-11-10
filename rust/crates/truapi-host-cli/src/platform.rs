//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and a [`UserConfirmation`] that
//! either auto-accepts or prompts on the CLI (the web/iOS "sign?" modal).
//! Auth-state transitions are published on a channel so the CLI can print the
//! pairing deeplink and observe connection status.

use std::{
	collections::HashMap,
	fs,
	io::{IsTerminal, Write},
	path::{Path, PathBuf},
	sync::{
		Arc, Mutex,
		atomic::{AtomicU32, Ordering},
	},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
	io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
	sync::Mutex as AsyncMutex,
};
use truapi::{
	latest as api,
	platform::{
		AuthState, ChainProvider, CoreStorage, CoreStorageKey, CreateTransactionReview,
		DevicePermissionStatus, Features, JsonRpcConnection, LocaleHost, Navigation,
		NftPurseDrainDestination, Notifications, PermissionDecision, PermissionStatusHost,
		Permissions, PreimageHost, ProductContext, ProductId, ProductOperations, ProductStorage,
		ProductStorageKey, ProviderError, SessionUiInfo, SignPayloadReview, SignRawReview,
		StackIdentity, StackStorageKey, ThemeHost, UserConfirmation, UserConfirmationReview,
	},
	v01,
};

use crate::{
	bulletin_lookup::{BitswapRpc, BulletinLookup},
	chain::CliChainProvider,
	stack_descriptor::Roles,
	terminal_ui::{ApprovalKind, SystemEvent, UiHandle},
};

static NEXT_STORAGE_TEMP_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_OPERATION_ID: AtomicU32 = AtomicU32::new(1);

/// A persistence failure with its filesystem operation or serialization cause.
#[derive(derive_more::Display, derive_more::From, Debug)]
pub enum StorageError {
	/// The destination cannot identify a parent directory.
	#[display("storage path has no parent: {}", path.display())]
	MissingParent { path: PathBuf },
	/// The stored value could not be serialized.
	#[display("{_0}")]
	#[from]
	Serialize(serde_json::Error),
	/// The parent storage directory could not be created.
	#[display("create storage dir")]
	CreateStorageDirectory { source: std::io::Error },
	/// The product's storage directory could not be created.
	#[display("create product storage directory {}", path.display())]
	CreateProductDirectory { path: PathBuf, source: std::io::Error },
	/// A storage file or directory could not be created.
	#[display("create {}", path.display())]
	Create { path: PathBuf, source: std::io::Error },
	/// A storage file could not be written.
	#[display("write {}", path.display())]
	Write { path: PathBuf, source: std::io::Error },
	/// A storage file could not be synchronized.
	#[display("sync {}", path.display())]
	Sync { path: PathBuf, source: std::io::Error },
	/// An existing destination could not be removed.
	#[cfg(windows)]
	#[display("replace {}", path.display())]
	Replace { path: PathBuf, source: std::io::Error },
	/// A completed write could not be moved to its destination.
	#[display("persist {}", path.display())]
	Persist { path: PathBuf, source: std::io::Error },
	/// The navigation transcript could not be appended.
	#[display("could not record navigation in {}", path.display())]
	RecordNavigation { path: PathBuf, source: std::io::Error },
	/// The storage directory could not be synchronized.
	#[cfg(unix)]
	#[display("sync storage dir {}", path.display())]
	SyncDirectory { path: PathBuf, source: std::io::Error },
}

impl std::error::Error for StorageError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::MissingParent { .. } => None,
			Self::Serialize(source) => std::error::Error::source(source),
			Self::CreateStorageDirectory { source } |
			Self::CreateProductDirectory { source, .. } |
			Self::Create { source, .. } |
			Self::Write { source, .. } |
			Self::Sync { source, .. } |
			Self::Persist { source, .. } |
			Self::RecordNavigation { source, .. } => Some(source),
			#[cfg(windows)]
			Self::Replace { source, .. } => Some(source),
			#[cfg(unix)]
			Self::SyncDirectory { source, .. } => Some(source),
		}
	}
}

impl From<StorageError> for v01::HostLocalStorageReadError {
	fn from(error: StorageError) -> Self {
		Self::Unknown { reason: format!("{:#}", anyhow::Error::new(error)) }
	}
}

impl From<StorageError> for api::GenericError {
	fn from(error: StorageError) -> Self {
		Self { reason: format!("{:#}", anyhow::Error::new(error)) }
	}
}

/// How the host answers confirmation prompts (the web/iOS "sign?" modals).
#[derive(Clone, Debug, PartialEq, Copy)]
pub enum ApprovalPolicy {
	/// Approve every sensitive action without prompting (`--auto-accept`).
	AutoAccept,
	/// Prompt on the CLI for sensitive actions and permission decisions.
	Prompt,
}

/// Filesystem locations for one host/session runtime.
#[derive(Clone)]
pub struct CliStoragePaths {
	state_dir: PathBuf,
	product_storage_dir: PathBuf,
	pairing_scope: Option<PairingStorageScope>,
}

#[derive(Clone)]
struct PairingStorageScope {
	network_dir: PathBuf,
	bootstrap_dir: PathBuf,
}

impl CliStoragePaths {
	pub fn new(state_dir: PathBuf, product_storage_dir: PathBuf) -> Self {
		Self { state_dir, product_storage_dir, pairing_scope: None }
	}

	/// Resolve the last paired user, falling back to a role-level bootstrap
	/// directory until the first identity is known.
	pub fn pairing(network_dir: PathBuf) -> Self {
		let bootstrap_dir = network_dir.join("pairing-host");
		let state_dir = read_current_pairing_user(&bootstrap_dir)
			.map(|user_id| network_dir.join(format!("{user_id}_pairing_host")))
			.filter(|path| path.is_dir())
			.unwrap_or_else(|| bootstrap_dir.clone());
		Self {
			product_storage_dir: state_dir.join("storage"),
			state_dir,
			pairing_scope: Some(PairingStorageScope { network_dir, bootstrap_dir }),
		}
	}
}

/// Headless-host platform shared by both roles.
pub struct CliPlatform {
	chain: Arc<CliChainProvider>,
	/// Genesis hashes of the chains this host serves, which
	/// `Features::feature_supported` answers from.
	served_chains: Roles<[u8; 32]>,
	extension_versions: truapi::platform::ExtensionVersionMap,
	/// Identity of the stack this host runs, which every core slot is keyed by.
	stack_identity: StackIdentity,
	product_storage: Mutex<HashMap<ProductId, HashMap<String, Vec<u8>>>>,
	core_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
	/// Device-scoped core slots, kept outside the per-user namespaces that
	/// [`Self::switch_pairing_user_storage`] swaps. Peers address this install
	/// by the key held here, so a user switch must not regenerate it.
	device_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
	product_storage_dir: Mutex<Option<PathBuf>>,
	core_storage_path: Mutex<Option<PathBuf>>,
	device_storage_path: Option<PathBuf>,
	state_dir: Mutex<Option<PathBuf>>,
	pairing_scope: Option<PairingStorageScope>,
	bulletin: Arc<BulletinLookup<BitswapRpc>>,
	next_notification_id: AtomicU32,
	scheduled_notifications: Arc<Mutex<HashMap<u32, api::HostPushNotificationRequest>>>,
	approval: Mutex<ApprovalPolicy>,
	/// Consulted-approval transcript (`TRUAPI_APPROVALS_LOG`): one
	/// `<approved|denied> <action>` line per decided confirmation.
	approvals_log: Option<PathBuf>,
	/// Navigation transcript (`TRUAPI_NAVIGATIONS_LOG`): one JSON line
	/// `{"url":…,"at":<unix ms>}` per destination the core accepted.
	navigations_log: Option<PathBuf>,
	ui: Option<UiHandle>,
	/// Serializes interactive CLI prompts so concurrent confirmations don't
	/// interleave on stdin.
	prompt_lock: AsyncMutex<()>,
}

impl CliPlatform {
	/// The URL a genesis routes to, so a test can assert a routing override
	/// rather than assume it.
	#[cfg(test)]
	pub fn routed_url(&self, genesis_hash: &[u8; 32]) -> &str {
		self.chain.location(*genesis_hash)
	}

	/// Build a platform whose chain provider connects to the network's People
	/// chain and whose optional state directory backs product/core storage.
	pub fn new(
		network: &crate::network::Stack,
		storage: Option<CliStoragePaths>,
		approval: ApprovalPolicy,
		ui: Option<UiHandle>,
	) -> Arc<Self> {
		let (product_storage_dir, core_storage_path) = storage
			.as_ref()
			.map(|paths| {
				if let Err(err) = fs::create_dir_all(&paths.state_dir) {
					tracing::warn!(
						path = %paths.state_dir.display(),
						%err,
						"could not create CLI storage dir"
					);
				}
				(
					Some(paths.product_storage_dir.clone()),
					Some(paths.state_dir.join("core-storage.json")),
				)
			})
			.unwrap_or((None, None));
		let product_storage =
			product_storage_dir.as_deref().map(load_product_storage).unwrap_or_default();
		let core_storage = core_storage_path.as_deref().map(load_hex_key_map).unwrap_or_default();
		// Anchored to the role-level bootstrap directory rather than the active
		// user's, so switching users keeps this install's device identity.
		let device_storage_path = storage.as_ref().map(|paths| {
			let directory = paths
				.pairing_scope
				.as_ref()
				.map(|scope| scope.bootstrap_dir.clone())
				.unwrap_or_else(|| paths.state_dir.clone());
			if let Err(err) = fs::create_dir_all(&directory) {
				tracing::warn!(
					path = %directory.display(),
					%err,
					"could not create CLI device storage dir"
				);
			}
			directory.join("device-storage.json")
		});
		let device_storage =
			device_storage_path.as_deref().map(load_hex_key_map).unwrap_or_default();

		let chain = Arc::new(CliChainProvider::new(network));
		let bitswap = BitswapRpc::new(Arc::clone(&chain), network.genesis().bulletin);
		Arc::new(Self {
			chain,
			served_chains: network.genesis(),
			extension_versions: network.extension_versions(),
			stack_identity: network.identity(),
			product_storage: Mutex::new(product_storage),
			core_storage: Mutex::new(core_storage),
			device_storage: Mutex::new(device_storage),
			product_storage_dir: Mutex::new(product_storage_dir),
			core_storage_path: Mutex::new(core_storage_path),
			device_storage_path,
			state_dir: Mutex::new(storage.as_ref().map(|paths| paths.state_dir.clone())),
			pairing_scope: storage.and_then(|paths| paths.pairing_scope),
			bulletin: Arc::new(BulletinLookup::new(bitswap)),
			next_notification_id: AtomicU32::new(1),
			scheduled_notifications: Arc::new(Mutex::new(HashMap::new())),
			approval: Mutex::new(approval),
			approvals_log: std::env::var_os("TRUAPI_APPROVALS_LOG").map(PathBuf::from),
			navigations_log: std::env::var_os("TRUAPI_NAVIGATIONS_LOG").map(PathBuf::from),
			ui,
			prompt_lock: AsyncMutex::new(()),
		})
	}

	/// Return the policy used by future confirmation requests.
	pub fn approval_policy(&self) -> ApprovalPolicy {
		*self.approval.lock().expect("approval policy mutex poisoned")
	}

	/// Change how future confirmation requests are decided.
	pub fn set_approval_policy(&self, approval: ApprovalPolicy) {
		*self.approval.lock().expect("approval policy mutex poisoned") = approval;
	}

	/// Identity of the stack this host runs.
	pub fn stack_identity(&self) -> &StackIdentity {
		&self.stack_identity
	}

	fn core_key(key: &StackStorageKey) -> Vec<u8> {
		use parity_scale_codec::Encode;
		key.encode()
	}

	fn persist_product_storage(
		&self,
		product_id: &ProductId,
		values: &HashMap<String, Vec<u8>>,
	) -> Result<(), StorageError> {
		let Some(directory) = self
			.product_storage_dir
			.lock()
			.expect("product storage path mutex poisoned")
			.clone()
		else {
			return Ok(());
		};
		save_product_storage(&directory, product_id, values)
	}

	/// Whether a slot belongs to the install rather than the signed-in user.
	fn is_device_scoped(key: &StackStorageKey) -> bool {
		matches!(key.slot, CoreStorageKey::DeviceEncryptionKey)
	}

	fn persist_device_storage(&self) -> Result<(), StorageError> {
		let Some(path) = self.device_storage_path.as_deref() else {
			return Ok(());
		};
		let storage = self.device_storage.lock().expect("device storage mutex poisoned");
		save_hex_key_map(path, &storage)
	}

	fn persist_core_storage(&self) -> Result<(), StorageError> {
		let Some(path) =
			self.core_storage_path.lock().expect("core storage path mutex poisoned").clone()
		else {
			return Ok(());
		};
		let storage = self.core_storage.lock().expect("core storage mutex poisoned");
		save_hex_key_map(&path, &storage)
	}

	/// Current identity-owned state directory (or the pairing bootstrap before
	/// a user has connected).
	pub fn state_dir(&self) -> Option<PathBuf> {
		self.state_dir.lock().expect("state path mutex poisoned").clone()
	}

	fn switch_pairing_user_storage(&self, user_id: &str) -> Result<(), StorageError> {
		let Some(scope) = &self.pairing_scope else {
			return Ok(());
		};
		let user_id = pairing_storage_name(user_id);
		let user_id = user_id.as_ref();
		let target_state = scope.network_dir.join(format!("{user_id}_pairing_host"));
		let current_state = self.state_dir.lock().expect("state path mutex poisoned").clone();
		if current_state.as_ref() == Some(&target_state) {
			persist_current_pairing_user(&scope.bootstrap_dir, user_id)?;
			return Ok(());
		}

		fs::create_dir_all(&target_state)
			.map_err(|source| StorageError::Create { path: target_state.clone(), source })?;
		let target_product_dir = target_state.join("storage");
		let target_core_path = target_state.join("core-storage.json");
		let migrating_bootstrap = current_state.as_ref() == Some(&scope.bootstrap_dir);

		// A fresh login writes these values before its username is known.
		// Carry only that pairing bootstrap across namespaces; permissions,
		// allowances, and product KV remain isolated to their previous user.
		let transient_keys = [
			CoreStorageKey::AuthSession,
			CoreStorageKey::PairingDeviceIdentity,
			CoreStorageKey::LastProcessedPairingStatement,
		]
		.map(|slot| {
			Self::core_key(&StackStorageKey { stack: self.stack_identity.id.clone(), slot })
		});
		let carried = {
			// Keep the same path -> storage lock order used by persistence so
			// an auth transition cannot deadlock with a concurrent core write.
			let current_path =
				self.core_storage_path.lock().expect("core storage path mutex poisoned");
			let mut current = self.core_storage.lock().expect("core storage mutex poisoned");
			if migrating_bootstrap {
				current.drain().collect::<Vec<_>>()
			} else {
				let carried = transient_keys
					.iter()
					.filter_map(|key| current.remove(key).map(|value| (key.clone(), value)))
					.collect::<Vec<_>>();
				if let Some(path) = current_path.as_deref() {
					save_hex_key_map(path, &current)?;
				}
				carried
			}
		};

		let mut target_core = load_hex_key_map(&target_core_path);
		target_core.extend(carried);
		let mut target_products = load_product_storage(&target_product_dir);
		if migrating_bootstrap {
			target_products.extend(
				self.product_storage.lock().expect("product storage mutex poisoned").clone(),
			);
			for (product_id, values) in &target_products {
				save_product_storage(&target_product_dir, product_id, values)?;
			}
		}
		{
			let mut core_storage_path =
				self.core_storage_path.lock().expect("core storage path mutex poisoned");
			let mut core_storage = self.core_storage.lock().expect("core storage mutex poisoned");
			*core_storage = target_core;
			*core_storage_path = Some(target_core_path);
		}
		*self.product_storage.lock().expect("product storage mutex poisoned") = target_products;
		*self.product_storage_dir.lock().expect("product storage path mutex poisoned") =
			Some(target_product_dir);
		*self.state_dir.lock().expect("state path mutex poisoned") = Some(target_state);
		self.persist_core_storage()?;
		persist_current_pairing_user(&scope.bootstrap_dir, user_id)
	}

	pub async fn decide(&self, action: &str, detail: String) -> bool {
		self.decide_with(action, detail, ApprovalKind::Action).await != PermissionDecision::Deny
	}

	async fn decide_with(
		&self,
		action: &str,
		detail: String,
		kind: ApprovalKind,
	) -> PermissionDecision {
		let decision = match self.approval_policy() {
			ApprovalPolicy::AutoAccept => {
				if let Some(ui) = &self.ui {
					ui.success(format!("Approved {action} automatically"), Some(detail));
				} else {
					crate::terminal_ui::output_success(
						format!("Approved {action} automatically"),
						Some(detail),
					);
				}
				PermissionDecision::AllowAlways
			},
			ApprovalPolicy::Prompt => {
				let _guard = self.prompt_lock.lock().await;
				if let Some(ui) = &self.ui {
					ui.decide(action, detail, kind).await
				} else {
					prompt_decision(action, &detail, kind).await
				}
			},
		};
		if let Some(path) = &self.approvals_log {
			record_approval(path, decision != PermissionDecision::Deny, action);
		}
		decision
	}
}

/// Append one `<approved|denied> <action>` line to the approvals transcript.
///
/// The line is written before the confirmation result is returned to the
/// runtime, so by the time a product call resolves, every prompt it consulted
/// is already on disk.
fn record_approval(path: &Path, approved: bool, action: &str) {
	let decision = if approved { "approved" } else { "denied" };
	let result = fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.and_then(|mut file| writeln!(file, "{decision} {action}"));
	if let Err(err) = result {
		tracing::warn!(path = %path.display(), %err, "could not record approval");
	}
}

async fn prompt_decision(action: &str, detail: &str, kind: ApprovalKind) -> PermissionDecision {
	if !std::io::stdin().is_terminal() {
		eprintln!("approval required for {action}, but stdin is not a terminal; rejecting");
		return PermissionDecision::Deny;
	}
	let action = crate::terminal_ui::sanitize_terminal_text(action);
	let detail = crate::terminal_ui::sanitize_terminal_text(detail);
	let mut stdout = tokio::io::stdout();
	let _ = stdout
		.write_all(
			format!(
				"\n\u{2500}\u{2500} confirm: {action} \u{2500}\u{2500}\n{detail}\n{} (default: deny) ",
				kind.choices()
			)
			.as_bytes(),
		)
		.await;
	let _ = stdout.flush().await;
	let mut line = String::new();
	let mut reader = BufReader::new(tokio::io::stdin());
	if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
		return PermissionDecision::Deny;
	}
	kind.parse(&line).unwrap_or(PermissionDecision::Deny)
}

#[async_trait::async_trait]
impl ProductStorage for CliPlatform {
	async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
		let scoped = ProductStorageKey::decode(&key)?;
		Ok(self
			.product_storage
			.lock()
			.expect("product storage mutex poisoned")
			.get(scoped.product_id())
			.and_then(|values| values.get(scoped.key()))
			.cloned())
	}

	async fn write(
		&self,
		key: String,
		value: Vec<u8>,
	) -> Result<(), v01::HostLocalStorageReadError> {
		let scoped = ProductStorageKey::decode(&key)?;
		let mut storage = self.product_storage.lock().expect("product storage mutex poisoned");
		let values = storage.entry(scoped.product_id().clone()).or_default();
		values.insert(scoped.key().to_string(), value);
		self.persist_product_storage(scoped.product_id(), values)?;
		Ok(())
	}

	async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
		let scoped = ProductStorageKey::decode(&key)?;
		let mut storage = self.product_storage.lock().expect("product storage mutex poisoned");
		let values = storage.entry(scoped.product_id().clone()).or_default();
		values.remove(scoped.key());
		self.persist_product_storage(scoped.product_id(), values)?;
		Ok(())
	}

	fn subscribe_storage(
		&self,
		key: String,
	) -> BoxStream<'static, Result<api::HostLocalStorageChangeItem, api::GenericError>> {
		// TODO: current value only; the CLI never pushes later changes. Needs a
		// per-key broadcast off write/clear for cross-context storage sync.
		let value = ProductStorageKey::decode(&key).ok().and_then(|scoped| {
			self.product_storage
				.lock()
				.expect("product storage mutex poisoned")
				.get(scoped.product_id())
				.and_then(|values| values.get(scoped.key()))
				.cloned()
		});
		Box::pin(stream::once(async move { Ok(api::HostLocalStorageChangeItem { value }) }))
	}
}

#[async_trait::async_trait]
impl ProductOperations for CliPlatform {
	async fn begin_operation(
		&self,
		_product: &ProductContext,
		_label: String,
	) -> Result<api::HostWorkerBeginOperationResponse, api::HostWorkerOperationError> {
		// The headless CLI has no worker to keep alive, so the id exists only so
		// a product can pair begin/end.
		Ok(api::HostWorkerBeginOperationResponse {
			id: NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed),
		})
	}

	async fn end_operation(
		&self,
		_product: &ProductContext,
		_id: u32,
	) -> Result<(), api::HostWorkerOperationError> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl CoreStorage for CliPlatform {
	async fn read_core_storage(
		&self,
		key: StackStorageKey,
	) -> Result<Option<Vec<u8>>, api::GenericError> {
		let store =
			if Self::is_device_scoped(&key) { &self.device_storage } else { &self.core_storage };
		Ok(store
			.lock()
			.expect("core storage mutex poisoned")
			.get(&Self::core_key(&key))
			.cloned())
	}

	async fn write_core_storage(
		&self,
		key: StackStorageKey,
		value: Vec<u8>,
	) -> Result<(), api::GenericError> {
		let device_scoped = Self::is_device_scoped(&key);
		{
			let store = if device_scoped { &self.device_storage } else { &self.core_storage };
			store
				.lock()
				.expect("core storage mutex poisoned")
				.insert(Self::core_key(&key), value);
		}
		if device_scoped { self.persist_device_storage() } else { self.persist_core_storage() }?;
		Ok(())
	}

	async fn clear_core_storage(&self, key: StackStorageKey) -> Result<(), api::GenericError> {
		let device_scoped = Self::is_device_scoped(&key);
		{
			let store = if device_scoped { &self.device_storage } else { &self.core_storage };
			store.lock().expect("core storage mutex poisoned").remove(&Self::core_key(&key));
		}
		if device_scoped { self.persist_device_storage() } else { self.persist_core_storage() }?;
		Ok(())
	}
}

#[async_trait::async_trait]
impl ChainProvider for CliPlatform {
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		self.chain.connect(genesis_hash).await
	}
}

#[async_trait::async_trait]
impl Navigation for CliPlatform {
	/// The core has already validated the URL, applied the open-URL
	/// permission and normalized product destinations to `polkadot://`, so
	/// this host only reports the request: it has no browser to open.
	async fn navigate_to(&self, url: String) -> Result<(), api::HostNavigateToError> {
		tracing::debug!(%url, "navigate_to");
		if let Some(path) = &self.navigations_log {
			let at = u64::try_from(
				SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
			)
			.expect("milliseconds since the epoch fit in u64");
			record_navigation(path, &url, at).map_err(|error| {
				api::HostNavigateToError::Unknown {
					reason: format!("{:#}", anyhow::Error::new(error)),
				}
			})?;
		}
		emit_notification_event(self.ui.as_ref(), SystemEvent::NavigationRequested { url });
		Ok(())
	}
}

/// Append one `{"url":…,"at":…}` JSON line to the navigation transcript.
///
/// Written before `navigate_to` resolves, so by the time the product's call
/// returns the request is on disk. A write failure fails the call rather than
/// silently dropping the record a test would assert on.
fn record_navigation(path: &Path, url: &str, at: u64) -> Result<(), StorageError> {
	let line = serde_json::to_string(&serde_json::json!({ "url": url, "at": at }))?;
	fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.and_then(|mut file| writeln!(file, "{line}"))
		.map_err(|source| StorageError::RecordNavigation { path: path.to_path_buf(), source })
}

#[async_trait::async_trait]
impl Notifications for CliPlatform {
	async fn push_notification(
		&self,
		notification: api::HostPushNotificationRequest,
	) -> Result<api::HostPushNotificationResponse, api::GenericError> {
		let id = self.next_notification_id.fetch_add(1, Ordering::Relaxed);
		let now = u64::try_from(
			SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
		)
		.expect("milliseconds since the epoch fit in u64");
		if let Some(scheduled_at) = notification.scheduled_at.filter(|at| *at > now) {
			{
				let mut pending =
					self.scheduled_notifications.lock().expect("notification mutex poisoned");
				if pending.len() >= 64 {
					return Err(api::GenericError {
						reason: "the CLI notification schedule is full (64 pending notifications)"
							.to_string(),
					});
				}
				pending.insert(id, notification.clone());
			}
			emit_notification_event(
				self.ui.as_ref(),
				SystemEvent::NotificationScheduled {
					id,
					text: notification.text.clone(),
					scheduled_at,
				},
			);
			let pending = self.scheduled_notifications.clone();
			let ui = self.ui.clone();
			tokio::spawn(async move {
				tokio::time::sleep(Duration::from_millis(scheduled_at.saturating_sub(now))).await;
				let notification = pending.lock().expect("notification mutex poisoned").remove(&id);
				if let Some(notification) = notification {
					emit_notification_event(
						ui.as_ref(),
						SystemEvent::NotificationDelivered {
							id,
							text: notification.text,
							deeplink: notification.deeplink,
						},
					);
				}
			});
		} else {
			emit_notification_event(
				self.ui.as_ref(),
				SystemEvent::NotificationDelivered {
					id,
					text: notification.text,
					deeplink: notification.deeplink,
				},
			);
		}
		Ok(api::HostPushNotificationResponse { id })
	}

	async fn cancel_notification(&self, id: u32) -> Result<(), api::GenericError> {
		if self
			.scheduled_notifications
			.lock()
			.expect("notification mutex poisoned")
			.remove(&id)
			.is_some()
		{
			emit_notification_event(self.ui.as_ref(), SystemEvent::NotificationCancelled { id });
		}
		Ok(())
	}
}

fn emit_notification_event(ui: Option<&UiHandle>, event: SystemEvent) {
	if let Some(ui) = ui {
		ui.event(event);
	} else {
		crate::terminal_ui::output_event(event);
	}
}

#[async_trait::async_trait]
impl PermissionStatusHost for CliPlatform {
	async fn device_permission_status(
		&self,
		_request: api::HostDevicePermissionRequest,
	) -> Result<DevicePermissionStatus, api::GenericError> {
		// A terminal has no OS permission gate, so every capability is
		// `NotApplicable` rather than `Granted`: claiming a grant would assert
		// state this host cannot see, and the stored product decision governs.
		Ok(DevicePermissionStatus::NotApplicable)
	}
}

#[async_trait::async_trait]
impl Permissions for CliPlatform {
	async fn device_permission(
		&self,
		product: &ProductContext,
		request: api::HostDevicePermissionRequest,
	) -> Result<PermissionDecision, api::GenericError> {
		let product_id = &product.product_id;
		Ok(self
			.decide_with(
				"device permission",
				format!("{product_id} requested access to {request}."),
				ApprovalKind::Permission,
			)
			.await)
	}

	async fn remote_permission(
		&self,
		product: &ProductContext,
		request: api::RemotePermissionRequest,
	) -> Result<PermissionDecision, api::GenericError> {
		let product_id = &product.product_id;
		let detail = match &request.permission {
			api::RemotePermission::Remote { .. } => {
				format!(
					"{product_id} requested {request}. This covers all ports on each host, including local services."
				)
			},
			_ => format!("{product_id} requested {request}."),
		};
		Ok(self.decide_with("remote permission", detail, ApprovalKind::Permission).await)
	}
}

#[async_trait::async_trait]
impl truapi::platform::ExtrinsicVersionHost for CliPlatform {
	async fn transaction_extension_versions(
		&self,
	) -> Result<truapi::platform::ExtensionVersionMap, api::GenericError> {
		Ok(self.extension_versions.clone())
	}
}

#[async_trait::async_trait]
impl Features for CliPlatform {
	async fn feature_supported(
		&self,
		request: api::HostFeatureSupportedRequest,
	) -> Result<api::HostFeatureSupportedResponse, api::GenericError> {
		let api::HostFeatureSupportedRequest::Chain { genesis_hash } = request;
		let supported = self
			.served_chains
			.iter()
			.any(|served| served.as_slice() == genesis_hash.as_slice());
		Ok(api::HostFeatureSupportedResponse { supported })
	}
}

impl truapi::platform::AuthPresenter for CliPlatform {
	fn auth_state_changed(&self, state: AuthState) {
		if let AuthState::Connected(info) = &state &&
			let Some(user_id) = storage_user_id(info) &&
			let Err(reason) = self.switch_pairing_user_storage(user_id)
		{
			tracing::warn!(reason = %format!("{:#}", anyhow::Error::new(reason)), %user_id, "could not switch pairing-host user storage");
		}
		let (connection, event) = match &state {
			AuthState::Pairing { deeplink } => {
				("pairing".to_string(), SystemEvent::PairingDeeplink { url: deeplink.clone() })
			},
			AuthState::Authenticating => {
				("authenticating".to_string(), SystemEvent::PairingAuthenticating)
			},
			AuthState::Connected(info) => (
				connected_user_id(info).unwrap_or("connected").to_string(),
				SystemEvent::PairingConnected {
					user_id: connected_user_id(info).map(str::to_string),
				},
			),
			AuthState::Disconnected => {
				("disconnected".to_string(), SystemEvent::PairingDisconnected)
			},
			AuthState::LoginFailed { reason, .. } => {
				("failed".to_string(), SystemEvent::PairingFailed { reason: reason.clone() })
			},
		};
		if let Some(ui) = &self.ui {
			ui.connection(connection);
			ui.event(event);
		} else {
			crate::terminal_ui::output_event(event);
		}
	}
}

fn connected_user_id(info: &SessionUiInfo) -> Option<&str> {
	info.full_username
		.as_deref()
		.filter(|value| !value.is_empty())
		.or_else(|| info.lite_username.as_deref().filter(|value| !value.is_empty()))
}

fn storage_user_id(info: &SessionUiInfo) -> Option<&str> {
	info.lite_username
		.as_deref()
		.filter(|value| !value.is_empty())
		.or_else(|| info.full_username.as_deref().filter(|value| !value.is_empty()))
}

#[async_trait::async_trait]
impl UserConfirmation for CliPlatform {
	async fn confirm_permission(
		&self,
		review: UserConfirmationReview,
	) -> Result<PermissionDecision, api::GenericError> {
		let (action, detail) = approval_summary(&review);
		Ok(self.decide_with(action, detail, ApprovalKind::Permission).await)
	}

	async fn confirm_user_action(
		&self,
		review: UserConfirmationReview,
	) -> Result<bool, api::GenericError> {
		let (action, detail) = approval_summary(&review);
		Ok(self.decide(action, detail).await)
	}
}

/// Names the product that asked, for the reviews that carry one. A relayed
/// request carries no caller, and saying so is more use than naming nobody.
fn asking(calling_product_id: Option<&ProductId>) -> String {
	match calling_product_id {
		Some(product_id) => format!("Product {product_id}"),
		None => "A paired host".to_string(),
	}
}

fn approval_summary(review: &UserConfirmationReview) -> (&'static str, String) {
	match review {
        UserConfirmationReview::SignPayload(SignPayloadReview::Product {
            calling_product_id,
            request,
        }) => (
            "sign payload",
            format!(
                "{} requested a SCALE payload signature for the {} account.",
                asking(calling_product_id.as_ref()),
                request.account.dot_ns_identifier,
            ),
        ),
        UserConfirmationReview::SignPayload(_) => (
            "sign payload",
            "A product requested a SCALE payload signature.".to_string(),
        ),
        UserConfirmationReview::SignRaw(SignRawReview::Product {
            calling_product_id,
            request,
            watermarked: true,
        }) => (
            "sign raw data",
            format!(
                "{} requested a raw-data signature for the {} account. The payload is hidden here.",
                asking(calling_product_id.as_ref()),
                request.account.dot_ns_identifier,
            ),
        ),
        UserConfirmationReview::SignRaw(
            SignRawReview::Product { watermarked: false, .. }
            | SignRawReview::LegacyAccount { watermarked: false, .. },
        ) => (
            "sign unprotected data",
            "Warning: this signature has no transaction-payload protection and may authorize transactions. The payload is hidden here.".to_string(),
        ),
        UserConfirmationReview::SignRaw(_) => (
            "sign raw data",
            "A product requested a raw-data signature. The payload is hidden here.".to_string(),
        ),
        UserConfirmationReview::SignVrf(review) => (
            "sign VRF transcript",
            format!(
                "Product {} requested a VRF signature for the {} account over {} transcript items.",
                review.calling_product_id,
                review.request.account.dot_ns_identifier,
                review.request.items.len()
            ),
        ),
        UserConfirmationReview::StatementStoreProductSign(review) => (
            "sign statement proof",
            format!(
                "{} requested a Statement Store proof signature for the {} account over a {}-byte payload.",
                asking(review.calling_product_id.as_ref()),
                review.account.dot_ns_identifier,
                review.payload.len()
            ),
        ),
        UserConfirmationReview::CreateTransaction(CreateTransactionReview::Product {
            calling_product_id,
            payload,
        }) => (
            "create transaction",
            format!(
                "{} requested a transaction from the {} account.",
                asking(calling_product_id.as_ref()),
                payload.signer.dot_ns_identifier,
            ),
        ),
        UserConfirmationReview::CreateTransaction(_) => (
            "create transaction",
            "A product requested a transaction from one of your accounts.".to_string(),
        ),
        UserConfirmationReview::AccountAlias(review) => (
            "derive account alias",
            format!(
                "Product {} requested a contextual account alias.",
                review.calling_product_id
            ),
        ),
        UserConfirmationReview::CreateProof(review) => (
            "create account proof",
            format!(
                "Product {} requested a contextual proof bound to {} bytes.",
                review.calling_product_id,
                review.message.len()
            ),
        ),
        UserConfirmationReview::IdentityDisclosure(review) => (
            "share identity",
            format!(
                "Product {} requested your primary identity.",
                review.product_id
            ),
        ),
        UserConfirmationReview::ResourceAllocation(_) => (
            "allocate resources",
            "A product requested host-managed resources.".to_string(),
        ),
        UserConfirmationReview::AccountAccess(review) => (
            "access another product account",
            format!(
                "Product {} requested access to the {} account.",
                review.requesting_product_id, review.target_product_id
            ),
        ),
        UserConfirmationReview::ProductSubtree(review) => (
            "resolve account subtree",
            format!(
                "Product {} requested its account from your device.",
                review.product_id
            ),
        ),
        UserConfirmationReview::NftPurseDrain(review) => (
            "move NFT purse items",
            format!(
                "Product {} requested to move {} item(s) out of purse {}, owned by {}, to {}.",
                review.calling_product_id,
                review.item_count,
                review.from_purse,
                review.owner,
                drain_destination(&review.destination),
            ),
        ),
    }
}

fn drain_destination(destination: &NftPurseDrainDestination) -> String {
	match destination {
		NftPurseDrainDestination::Purse { purse } => format!("purse {purse}"),
		NftPurseDrainDestination::ExternalKey { key } => format!("key 0x{}", hex::encode(key)),
		NftPurseDrainDestination::Cheque => "a cheque".to_owned(),
		NftPurseDrainDestination::Refund => "the sender of a deposited cheque".to_owned(),
	}
}

impl ThemeHost for CliPlatform {
	fn subscribe_theme(
		&self,
	) -> BoxStream<'static, Result<api::HostThemeSubscribeItem, api::GenericError>> {
		Box::pin(stream::once(async {
			Ok(api::HostThemeSubscribeItem {
				name: api::ThemeName::Default,
				variant: api::ThemeVariant::Dark,
			})
		}))
	}
}

impl LocaleHost for CliPlatform {
	fn subscribe_locale(
		&self,
	) -> BoxStream<'static, Result<api::HostLocaleSubscribeItem, api::GenericError>> {
		Box::pin(stream::once(async {
			Ok(api::HostLocaleSubscribeItem { language_tag: "en".to_string() })
		}))
	}
}

impl PreimageHost for CliPlatform {
	fn lookup_preimage(
		&self,
		key: [u8; 32],
	) -> BoxStream<'static, Result<Option<Vec<u8>>, api::GenericError>> {
		self.bulletin.subscribe(key)
	}
}

#[derive(Serialize, Deserialize)]
struct JsonMap {
	values: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductStorageDocument {
	version: u32,
	product_id: ProductId,
	values: HashMap<String, String>,
}

fn load_product_storage(directory: &Path) -> HashMap<ProductId, HashMap<String, Vec<u8>>> {
	let mut products = HashMap::<ProductId, HashMap<String, Vec<u8>>>::new();

	let entries = match fs::read_dir(directory) {
		Ok(entries) => Some(entries),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
		Err(error) => {
			tracing::warn!(
				path = %directory.display(),
				%error,
				"could not list per-product CLI storage"
			);
			None
		},
	};
	if let Some(entries) = entries {
		for entry in entries.filter_map(Result::ok) {
			let path = entry.path();
			if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
				continue;
			}
			let Some((product_id, values)) = load_product_storage_file(&path) else {
				continue;
			};
			let expected_path = product_storage_path(directory, &product_id);
			if path != expected_path {
				tracing::warn!(
					path = %path.display(),
					expected = %expected_path.display(),
					"ignored product storage with a non-canonical filename"
				);
				continue;
			}
			products.entry(product_id).or_default().extend(values);
		}
	}

	products
}

fn load_product_storage_file(path: &Path) -> Option<(ProductId, HashMap<String, Vec<u8>>)> {
	let text = fs::read_to_string(path)
		.inspect_err(|error| {
			tracing::warn!(path = %path.display(), %error, "could not read product storage");
		})
		.ok()?;
	let document = serde_json::from_str::<ProductStorageDocument>(&text)
		.inspect_err(|error| {
			tracing::warn!(path = %path.display(), %error, "could not decode product storage");
		})
		.ok()?;
	if document.version != 1 {
		tracing::warn!(
			path = %path.display(),
			version = document.version,
			"unsupported product storage version"
		);
		return None;
	}

	let values = document
		.values
		.into_iter()
		.map(|(key, value)| hex::decode(value).map(|bytes| (key, bytes)))
		.collect::<Result<HashMap<_, _>, _>>()
		.inspect_err(|error| {
			tracing::warn!(
				path = %path.display(),
				%error,
				"product storage contains an invalid value"
			);
		})
		.ok()?;
	Some((document.product_id, values))
}

fn save_product_storage(
	directory: &Path,
	product_id: &ProductId,
	values: &HashMap<String, Vec<u8>>,
) -> Result<(), StorageError> {
	fs::create_dir_all(directory).map_err(|source| StorageError::CreateProductDirectory {
		path: directory.to_path_buf(),
		source,
	})?;
	let document = ProductStorageDocument {
		version: 1,
		product_id: product_id.clone(),
		values: values.iter().map(|(key, value)| (key.clone(), hex::encode(value))).collect(),
	};
	let text = serde_json::to_string_pretty(&document)?;
	atomic_write(&product_storage_path(directory, product_id), text.as_bytes())
}

fn product_storage_path(directory: &Path, product_id: &ProductId) -> PathBuf {
	let mut slug = product_id
		.as_str()
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
				character
			} else {
				'-'
			}
		})
		.take(48)
		.collect::<String>();
	slug = slug.trim_matches(|character| matches!(character, '.' | '-')).to_string();
	if slug.is_empty() {
		slug.push_str("product");
	}
	let digest = Sha256::digest(product_id.as_str().as_bytes());
	directory.join(format!("{slug}--{}.json", hex::encode(digest)))
}

/// Extension appended to a storage file that could not be read, so its contents
/// survive for manual recovery.
const UNREADABLE_SUFFIX: &str = "unreadable";

/// Load a storage file, treating an unreadable one as empty.
///
/// `read_string_map` collects into a `Result`, so one undecodable value fails the
/// whole file. Returning an empty map on that would be silently destructive: the
/// caller's next write persists the empty map over the file, taking every intact
/// entry with it — including keys this run never touched. So the file is first
/// moved aside to `<name>.unreadable`, which keeps the data recoverable and makes
/// the warning actionable.
fn load_string_map(path: &Path) -> HashMap<String, Vec<u8>> {
	match read_string_map(path) {
		Ok(values) => values,
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
		Err(err) => {
			let preserved = path.with_extension(UNREADABLE_SUFFIX);
			match fs::rename(path, &preserved) {
				Ok(()) => {
					tracing::warn!(
						path = %path.display(),
						preserved = %preserved.display(),
						%err,
						"could not read CLI storage; moved it aside and started empty"
					)
				},
				Err(rename_err) => {
					tracing::warn!(
						path = %path.display(),
						%err,
						%rename_err,
						"could not read CLI storage, and could not move it aside; \
						 the next write will overwrite it"
					)
				},
			}
			HashMap::new()
		},
	}
}

fn read_string_map(path: &Path) -> std::io::Result<HashMap<String, Vec<u8>>> {
	let text = fs::read_to_string(path)?;
	let json = serde_json::from_str::<JsonMap>(&text)
		.map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
	json.values
		.into_iter()
		.map(|(key, value)| {
			hex::decode(value)
				.map(|bytes| (key, bytes))
				.map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
		})
		.collect()
}

fn save_string_map(path: &Path, values: &HashMap<String, Vec<u8>>) -> Result<(), StorageError> {
	let json = JsonMap {
		values: values.iter().map(|(key, value)| (key.clone(), hex::encode(value))).collect(),
	};
	let text = serde_json::to_string_pretty(&json)?;
	atomic_write(path, text.as_bytes())
}

/// Replace a persisted value after its contents have reached the filesystem.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
	let parent = match path.parent() {
		Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
		Some(parent) => parent,
		None => return Err(StorageError::MissingParent { path: path.to_path_buf() }),
	};
	fs::create_dir_all(parent).map_err(|source| StorageError::CreateStorageDirectory { source })?;
	let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("storage.json");
	let temporary_id = NEXT_STORAGE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
	let temporary =
		path.with_file_name(format!(".{name}.{}.{temporary_id}.tmp", std::process::id()));
	let mut file = fs::File::create(&temporary)
		.map_err(|source| StorageError::Create { path: temporary.clone(), source })?;
	file.write_all(bytes)
		.map_err(|source| StorageError::Write { path: temporary.clone(), source })?;
	file.sync_all()
		.map_err(|source| StorageError::Sync { path: temporary.clone(), source })?;
	drop(file);
	#[cfg(windows)]
	if path.exists() {
		fs::remove_file(path)
			.map_err(|source| StorageError::Replace { path: path.to_path_buf(), source })?;
	}
	fs::rename(&temporary, path)
		.map_err(|source| StorageError::Persist { path: path.to_path_buf(), source })?;
	#[cfg(unix)]
	fs::File::open(parent)
		.and_then(|directory| directory.sync_all())
		.map_err(|source| StorageError::SyncDirectory { path: parent.to_path_buf(), source })?;
	Ok(())
}

fn load_hex_key_map(path: &Path) -> HashMap<Vec<u8>, Vec<u8>> {
	load_string_map(path)
		.into_iter()
		.filter_map(|(key, value)| hex::decode(key).ok().map(|decoded| (decoded, value)))
		.collect()
}

/// Directory-safe name for one paired identity's storage namespace.
///
/// The connected id is whatever the dotNS identity yields: a lite username
/// when there is one, otherwise the free-form `full_username`. Only the former is
/// guaranteed to satisfy [`crate::sessions::validate_name`], so a display name
/// like `"Tarik Gul"` is rejected on both the space and the capitals.
///
/// Rejecting cannot mean "keep the previous namespace mounted" — that serves one
/// identity out of another's `state_dir`, core storage and product KV. So an
/// unusable id is replaced by the hex SHA-256 of its bytes, which isolates the
/// session regardless of what the chain returns. Hex is `[0-9a-f]`, so the derived
/// name is itself a legal session name and survives the `validate_name` check on
/// the read path in [`read_current_pairing_user`].
fn pairing_storage_name(user_id: &str) -> std::borrow::Cow<'_, str> {
	if crate::sessions::validate_name(user_id).is_ok() {
		return std::borrow::Cow::Borrowed(user_id);
	}
	std::borrow::Cow::Owned(hex::encode(Sha256::digest(user_id.as_bytes())))
}

const CURRENT_PAIRING_USER_FILE: &str = "current-user";

fn read_current_pairing_user(bootstrap_dir: &Path) -> Option<String> {
	let user_id = fs::read_to_string(bootstrap_dir.join(CURRENT_PAIRING_USER_FILE))
		.ok()?
		.trim()
		.to_string();
	crate::sessions::validate_name(&user_id).ok()?;
	Some(user_id)
}

fn persist_current_pairing_user(bootstrap_dir: &Path, user_id: &str) -> Result<(), StorageError> {
	fs::create_dir_all(bootstrap_dir)
		.map_err(|source| StorageError::Create { path: bootstrap_dir.to_path_buf(), source })?;
	let path = bootstrap_dir.join(CURRENT_PAIRING_USER_FILE);
	let temporary =
		bootstrap_dir.join(format!(".{CURRENT_PAIRING_USER_FILE}.{}.tmp", std::process::id()));
	fs::write(&temporary, format!("{user_id}\n"))
		.map_err(|source| StorageError::Write { path: temporary.clone(), source })?;
	fs::rename(&temporary, &path).map_err(|source| StorageError::Persist { path, source })
}

fn save_hex_key_map(path: &Path, values: &HashMap<Vec<u8>, Vec<u8>>) -> Result<(), StorageError> {
	let keyed: HashMap<String, Vec<u8>> =
		values.iter().map(|(key, value)| (hex::encode(key), value.clone())).collect();
	save_string_map(path, &keyed)
}

#[cfg(test)]
mod tests;
