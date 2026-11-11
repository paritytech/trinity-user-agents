//! In-memory mock [`Platform`] for tests and host simulators.
//!
//! `MockPlatform` implements every capability trait with deterministic,
//! configurable behavior and no OS, device, or network dependency: storage is
//! an in-memory map, permission prompts answer from a fixed per-capability
//! policy (no UI), navigation and notifications are recorded, and chain access
//! returns a configurable connection. Because the protocol logic lives in
//! `truapi`, a `MockPlatform` wired into the core yields a faithful host
//! whose only mocked surface is the OS-primitive seam.
//!
//! Behavior is a [`MockConfig`](crate::platform::mock::MockConfig) read on every call:
//! per-capability permission policy, feature support, theme, confirmation answer,
//! [`ChainBehavior`](crate::platform::mock::ChainBehavior), and
//! [`MockFaults`](crate::platform::mock::MockFaults) error injection. Recordings (`navigations`,
//! `pushed_notifications`, `confirmations`, `auth_states`, `sent_rpc`, …) are
//! the test oracles.
//!
//! Signing and login require a paired wallet answering over the statement-store
//! channel. With [`ChainBehavior::Silent`](crate::platform::mock::ChainBehavior::Silent) the chain
//! connection records outbound requests and never answers, so those flows park; use
//! [`ChainBehavior::Scripted`](crate::platform::mock::ChainBehavior::Scripted) to feed canned
//! response frames.
//!
//! Preimage submission is core-owned on current core (the core builds, signs,
//! and submits the Bulletin `TransactionStorage.store` transaction itself), so
//! the mock only implements host-side content retrieval via `lookup_preimage`.
//! Seed retrievable content with
//! [`MockPlatform::insert_preimage`](crate::platform::mock::MockPlatform::insert_preimage).

use std::{
	collections::{BTreeMap, HashMap},
	pin::Pin,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, AtomicU32, Ordering},
	},
	task::{Context, Poll},
};

use futures::{
	Stream, StreamExt,
	channel::mpsc,
	stream::{self, BoxStream},
};

use truapi::latest;

use crate::platform::{
	AuthPresenter, AuthState, ChainProvider, ChatPlatform, CoreStorage, CoreStorageKey, Features,
	HostScan, JsonRpcConnection, LocaleHost, Navigation, Notifications, PermissionDecision,
	Permissions, PreimageHost, ProductContext, ProductId, ProductOperations, ProductStorage,
	ProviderError, ScannerPlatform, ThemeHost, UserConfirmation, UserConfirmationReview,
};

/// How the mock answers a permission prompt for one capability.
#[derive(Clone, Copy, Default)]
pub enum PermissionPolicy {
	/// Grant without prompting.
	#[default]
	AllowAll,
	/// Deny.
	DenyAll,
}

impl PermissionPolicy {
	fn granted(self) -> bool {
		matches!(self, PermissionPolicy::AllowAll)
	}
}

/// The kind of action the core asked the host to confirm. Recorded on every
/// `confirm_user_action` so tests can assert what the core tried to do
/// (e.g. that a sign-payload review fired) even when the chain parks afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
	/// [`UserConfirmationReview::SignPayload`].
	SignPayload,
	/// [`UserConfirmationReview::SignRaw`].
	SignRaw,
	/// [`UserConfirmationReview::CreateTransaction`].
	CreateTransaction,
	/// [`UserConfirmationReview::AccountAlias`].
	AccountAlias,
	/// [`UserConfirmationReview::CreateProof`].
	CreateProof,
	/// [`UserConfirmationReview::ResourceAllocation`].
	ResourceAllocation,
	/// [`UserConfirmationReview::IdentityDisclosure`].
	IdentityDisclosure,
	/// [`UserConfirmationReview::AccountAccess`].
	AccountAccess,
	/// [`UserConfirmationReview::StatementStoreProductSign`].
	StatementStoreProductSign,
	/// [`UserConfirmationReview::SignVrf`].
	SignVrf,
	/// [`UserConfirmationReview::ProductSubtree`].
	ProductSubtree,
	/// [`UserConfirmationReview::NftPurseDrain`].
	NftPurseDrain,
}

impl ConfirmKind {
	fn of(review: &UserConfirmationReview) -> Self {
		match review {
			UserConfirmationReview::SignPayload(_) => ConfirmKind::SignPayload,
			UserConfirmationReview::SignRaw(_) => ConfirmKind::SignRaw,
			UserConfirmationReview::CreateTransaction(_) => ConfirmKind::CreateTransaction,
			UserConfirmationReview::AccountAlias(_) => ConfirmKind::AccountAlias,
			UserConfirmationReview::CreateProof(_) => ConfirmKind::CreateProof,
			UserConfirmationReview::ResourceAllocation(_) => ConfirmKind::ResourceAllocation,
			UserConfirmationReview::IdentityDisclosure(_) => ConfirmKind::IdentityDisclosure,
			UserConfirmationReview::AccountAccess(_) => ConfirmKind::AccountAccess,
			UserConfirmationReview::StatementStoreProductSign(_) => {
				ConfirmKind::StatementStoreProductSign
			},
			UserConfirmationReview::SignVrf(_) => ConfirmKind::SignVrf,
			UserConfirmationReview::ProductSubtree(_) => ConfirmKind::ProductSubtree,
			UserConfirmationReview::NftPurseDrain(_) => ConfirmKind::NftPurseDrain,
		}
	}
}

/// Which prompt surface a permission decision came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionKind {
	/// An OS capability prompt ([`Permissions::device_permission`]).
	Device,
	/// A remote-capability prompt ([`Permissions::remote_permission`]).
	Remote,
}

/// One operation a product began and has not ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOperation {
	/// Product that began it.
	pub product_id: ProductId,
	/// Id the mock handed back, unique among this product's open operations.
	pub id: u32,
	/// Label the product gave, empty when it gave none.
	pub label: String,
}

/// One permission answer the mock gave, recorded for assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionLogEntry {
	/// The permission's `Display` form, the same key `grant_permission` takes.
	pub tag: String,
	/// What the mock answered.
	pub approved: bool,
	/// Which prompt surface asked.
	pub kind: PermissionKind,
}

/// State of the mock's chain connection, as the host sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChainStatus {
	/// No connection has been opened yet.
	#[default]
	Idle,
	/// At least one connection is open.
	Connected,
	/// [`MockPlatform::simulate_disconnect`] closed the open connections, and
	/// further `connect` calls fail until [`MockPlatform::simulate_reconnect`].
	Disconnected,
}

/// One chat message the product posted through the mock.
#[derive(Clone)]
pub struct ChatMessageRecord {
	/// Id the mock assigned and returned to the product.
	pub message_id: String,
	/// Room the message was posted to.
	pub room_id: String,
	/// What was posted.
	pub payload: latest::ChatMessageContent,
	/// The product's one-line description of the message.
	pub alt: Option<String>,
}

/// How the mock's chain connection behaves.
#[derive(Clone, Default)]
pub enum ChainBehavior {
	/// Record outbound requests without replying. Local transaction creation
	/// needs chain metadata; login and statement-store operations also wait
	/// for chain replies. Run those flows under a timeout or use
	/// [`ChainBehavior::Closed`] to observe a disconnect.
	#[default]
	Silent,
	/// Record outbound requests and replay these response frames in order,
	/// then end the stream.
	Scripted(Vec<String>),
	/// Record outbound requests; the response stream ends immediately, so
	/// disconnect/timeout paths can be asserted (fail-fast) rather than parked.
	Closed,
	/// `connect` fails with this reason.
	ConnectError(String),
}

/// Optional error injection. When a field is `Some`, the matching host call
/// returns that error instead of succeeding, exercising the core's
/// error-handling paths.
#[derive(Clone, Default)]
pub struct MockFaults {
	/// Product and core storage reads/writes/clears fail with this reason.
	pub storage_error: Option<String>,
	/// `navigate_to` fails with this reason.
	pub navigate_error: Option<String>,
	/// `push_notification` fails with this reason.
	pub notification_error: Option<String>,
	/// `confirm_user_action` fails with this reason instead of answering.
	///
	/// Distinct from a declined confirmation: the host could not put the
	/// question to the user at all, which the core must not read as a refusal.
	pub confirmation_error: Option<String>,
	/// Device and remote permission prompts fail with this reason.
	pub permission_error: Option<String>,
	/// `feature_supported` and `supported_chains` fail with this reason.
	pub feature_error: Option<String>,
	/// Chat room, bot, and message calls fail with this reason.
	pub chat_error: Option<String>,
}

/// Behavior knobs for [`MockPlatform`], read on every call.
#[derive(Clone)]
pub struct MockConfig {
	/// Answer for `device_permission`.
	pub device_permissions: PermissionPolicy,
	/// Answer for `remote_permission`.
	pub remote_permissions: PermissionPolicy,
	/// Whether `feature_supported` reports support.
	pub feature_supported: bool,
	/// When set, `feature_supported` answers from `supported_chains`, as a real
	/// host does, so a chain outside that set is unsupported. It becomes the
	/// default in a later breaking release. The JS mock's `onlyServedChains` is
	/// the same option.
	pub only_served_chains: bool,
	/// Theme emitted by `subscribe_theme`.
	pub theme: latest::ThemeVariant,
	/// BCP 47 language tag the mock reports.
	pub language_tag: String,
	/// Whether `confirm_user_action` confirms reviewed actions.
	/// [`MockPlatform::set_confirm_user_actions`] changes it for one test.
	pub confirm_user_actions: bool,
	/// Chains the mock reports serving (RFC 0026).
	///
	/// Defaults to the three [`mock_genesis`] chains, the same set the
	/// TypeScript mock declares, so a suite written against either one is
	/// answered the same way. Replace it to exercise a host serving fewer
	/// chains, declaring each with the genesis hash its runtime config carries.
	pub supported_chains: crate::platform::HostChainSet,
	/// Chain connection behavior.
	pub chain: ChainBehavior,
	/// Error injection.
	pub faults: MockFaults,
}

impl Default for MockConfig {
	fn default() -> Self {
		Self {
			device_permissions: PermissionPolicy::AllowAll,
			remote_permissions: PermissionPolicy::AllowAll,
			feature_supported: true,
			only_served_chains: false,
			theme: latest::ThemeVariant::Dark,
			language_tag: "en".to_string(),
			confirm_user_actions: true,
			supported_chains: mock_genesis::chain_set(),
			chain: ChainBehavior::Silent,
			faults: MockFaults::default(),
		}
	}
}

/// Delivers a preimage to a lookup that is waiting on its key.
type PreimageSender = mpsc::UnboundedSender<Option<Vec<u8>>>;

/// Lookups waiting on each preimage key.
type PreimageSubscribers = Arc<Mutex<HashMap<Vec<u8>, Vec<PreimageSender>>>>;

/// One open preimage lookup. Dropping it removes its subscriber, and the key
/// once no lookup waits on it, as the JS mock does on unsubscribe.
struct PreimageLookup {
	receiver: mpsc::UnboundedReceiver<Option<Vec<u8>>>,
	key: Vec<u8>,
	subscribers: PreimageSubscribers,
}

impl Stream for PreimageLookup {
	type Item = Option<Vec<u8>>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.receiver.poll_next_unpin(cx)
	}
}

impl Drop for PreimageLookup {
	fn drop(&mut self) {
		// Closing first marks this lookup's sender closed, so the sweep below
		// removes it along with any other lookup already dropped.
		self.receiver.close();
		let Ok(mut subscribers) = self.subscribers.lock() else { return };
		if let Some(senders) = subscribers.get_mut(&self.key) {
			senders.retain(|sender| !sender.is_closed());
			if senders.is_empty() {
				subscribers.remove(&self.key);
			}
		}
	}
}

/// In-memory mock host platform. Cheap to `clone`; clones share recordings and
/// storage (state is `Arc`ed), so a recording made through one clone is visible
/// through another.
#[derive(Clone)]
pub struct MockPlatform {
	config: Arc<MockConfig>,
	storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
	preimages: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
	navigations: Arc<Mutex<Vec<String>>>,
	notifications: Arc<Mutex<Vec<latest::HostPushNotificationRequest>>>,
	cancelled_notifications: Arc<Mutex<Vec<u32>>>,
	reviews: Arc<Mutex<Vec<UserConfirmationReview>>>,
	auth_states: Arc<Mutex<Vec<AuthState>>>,
	sent_rpc: Arc<Mutex<Vec<String>>>,
	/// Starts at 1: the id is what a product cancels by, and one that treats
	/// 0 as "no id" cannot cancel the first notification it ever schedules.
	next_notification_id: Arc<AtomicU32>,
	/// Explicit per-permission answers, keyed by `Display` form. Set by
	/// `grant_permission` / `revoke_permission`; overrides the config policy.
	permission_decisions: Arc<Mutex<BTreeMap<String, bool>>>,
	/// When set, a permission with no explicit answer is denied instead of
	/// falling back to the config policy.
	enforce_permissions: Arc<AtomicBool>,
	/// The answer reviews get, seeded from `confirm_user_actions`.
	confirming: Arc<AtomicBool>,
	permission_log: Arc<Mutex<Vec<PermissionLogEntry>>>,
	chat_rooms: Arc<Mutex<BTreeMap<String, latest::ChatRoom>>>,
	chat_bots: Arc<Mutex<BTreeMap<String, latest::HostChatRegisterBotRequest>>>,
	chat_messages: Arc<Mutex<Vec<ChatMessageRecord>>>,
	/// Live `subscribe_chat_rooms` streams, fed a fresh list on every change.
	chat_room_subscribers:
		Arc<Mutex<Vec<mpsc::UnboundedSender<latest::HostChatListSubscribeItem>>>>,
	next_chat_message_id: Arc<AtomicU32>,
	next_operation_id: Arc<AtomicU32>,
	open_operations: Arc<Mutex<Vec<OpenOperation>>>,
	/// Current theme. Seeded from the config and replaced by `set_theme`.
	theme: Arc<Mutex<latest::ThemeVariant>>,
	theme_subscribers: Arc<Mutex<Vec<mpsc::UnboundedSender<latest::HostThemeSubscribeItem>>>>,
	/// What the next scan answers. Starts as a dismissal.
	scan_answer: Arc<Mutex<HostScan>>,
	storage_subscribers:
		Arc<Mutex<HashMap<String, Vec<mpsc::UnboundedSender<latest::HostLocalStorageChangeItem>>>>>,
	preimage_subscribers: PreimageSubscribers,
	chain_status: Arc<Mutex<ChainStatus>>,
	/// One per live connection; sending ends that connection's response stream.
	chain_disconnectors: Arc<Mutex<Vec<mpsc::UnboundedSender<()>>>>,
}

impl Default for MockPlatform {
	fn default() -> Self {
		Self::new()
	}
}

impl MockPlatform {
	/// Build a mock platform with default behavior (allow-all permissions,
	/// feature support on, dark theme, auto-confirm, silent chain, no faults).
	pub fn new() -> Self {
		Self::with_config(MockConfig::default())
	}

	/// Build a mock platform with explicit behavior.
	pub fn with_config(config: MockConfig) -> Self {
		let theme = config.theme;
		let confirming = config.confirm_user_actions;
		Self {
			config: Arc::new(config),
			storage: Arc::new(Mutex::new(HashMap::new())),
			preimages: Arc::new(Mutex::new(HashMap::new())),
			navigations: Arc::new(Mutex::new(Vec::new())),
			notifications: Arc::new(Mutex::new(Vec::new())),
			cancelled_notifications: Arc::new(Mutex::new(Vec::new())),
			reviews: Arc::new(Mutex::new(Vec::new())),
			auth_states: Arc::new(Mutex::new(Vec::new())),
			sent_rpc: Arc::new(Mutex::new(Vec::new())),
			next_notification_id: Arc::new(AtomicU32::new(1)),
			permission_decisions: Arc::new(Mutex::new(BTreeMap::new())),
			enforce_permissions: Arc::new(AtomicBool::new(false)),
			confirming: Arc::new(AtomicBool::new(confirming)),
			permission_log: Arc::new(Mutex::new(Vec::new())),
			chat_rooms: Arc::new(Mutex::new(BTreeMap::new())),
			chat_bots: Arc::new(Mutex::new(BTreeMap::new())),
			chat_messages: Arc::new(Mutex::new(Vec::new())),
			chat_room_subscribers: Arc::new(Mutex::new(Vec::new())),
			next_chat_message_id: Arc::new(AtomicU32::new(0)),
			next_operation_id: Arc::new(AtomicU32::new(0)),
			open_operations: Arc::new(Mutex::new(Vec::new())),
			theme: Arc::new(Mutex::new(theme)),
			theme_subscribers: Arc::new(Mutex::new(Vec::new())),
			scan_answer: Arc::new(Mutex::new(HostScan::Dismissed)),
			storage_subscribers: Arc::new(Mutex::new(HashMap::new())),
			preimage_subscribers: Arc::new(Mutex::new(HashMap::new())),
			chain_status: Arc::new(Mutex::new(ChainStatus::Idle)),
			chain_disconnectors: Arc::new(Mutex::new(Vec::new())),
		}
	}

	/// URLs the core asked the host to open, in order.
	pub fn navigations(&self) -> Vec<String> {
		self.navigations.lock().expect("navigations poisoned").clone()
	}

	/// Notifications the core asked the host to show, in order.
	pub fn pushed_notifications(&self) -> Vec<latest::HostPushNotificationRequest> {
		self.notifications.lock().expect("notifications poisoned").clone()
	}

	/// Notification ids the core asked the host to cancel, in order.
	pub fn cancelled_notifications(&self) -> Vec<u32> {
		self.cancelled_notifications.lock().expect("cancellations poisoned").clone()
	}

	/// Full confirmation reviews the core requested, in order.
	///
	/// Carries the reviewed payload, not just its kind: a `SignRaw` review
	/// holds the bytes the product asked to have signed, so a test can assert
	/// *what* was put to the user rather than only that something was.
	pub fn reviews(&self) -> Vec<UserConfirmationReview> {
		self.reviews.lock().expect("reviews poisoned").clone()
	}

	/// Confirmation kinds the core requested, in order.
	///
	/// The kind-only view of [`MockPlatform::reviews`], for assertions that
	/// only care that a given prompt fired.
	pub fn confirmations(&self) -> Vec<ConfirmKind> {
		self.reviews
			.lock()
			.expect("reviews poisoned")
			.iter()
			.map(ConfirmKind::of)
			.collect()
	}

	/// Permission answers the mock gave, in order.
	pub fn permission_log(&self) -> Vec<PermissionLogEntry> {
		self.permission_log.lock().expect("permission log poisoned").clone()
	}

	/// Permissions with an explicit grant, in `Display` order.
	pub fn granted_permissions(&self) -> Vec<String> {
		self.permission_decisions
			.lock()
			.expect("permission decisions poisoned")
			.iter()
			.filter(|(_, granted)| **granted)
			.map(|(permission, _)| permission.clone())
			.collect()
	}

	/// Answer `permission` with a grant, whatever the configured policy says.
	///
	/// The key is the permission's SCALE variant tag -- `"Camera"`, or
	/// `"Remote"` for a [`latest::RemotePermission::Remote`] whatever domains
	/// it names.
	pub fn grant_permission(&self, permission: impl Into<String>) {
		self.permission_decisions
			.lock()
			.expect("permission decisions poisoned")
			.insert(permission.into(), true);
	}

	/// Answer `permission` with a denial, whatever the configured policy says.
	pub fn revoke_permission(&self, permission: impl Into<String>) {
		self.permission_decisions
			.lock()
			.expect("permission decisions poisoned")
			.insert(permission.into(), false);
	}

	/// Drop the explicit answer for `permission`, restoring policy fallback.
	pub fn reset_permission(&self, permission: &str) {
		self.permission_decisions
			.lock()
			.expect("permission decisions poisoned")
			.remove(permission);
	}

	/// When enforcing, deny every permission without an explicit grant instead
	/// of falling back to the configured policy. Off by default.
	pub fn set_enforce_permissions(&self, enforce: bool) {
		self.enforce_permissions.store(enforce, Ordering::SeqCst);
	}

	/// Answer one permission prompt and record what was answered.
	fn decide_permission(
		&self,
		kind: PermissionKind,
		permission: String,
		policy: PermissionPolicy,
	) -> bool {
		let explicit = self
			.permission_decisions
			.lock()
			.expect("permission decisions poisoned")
			.get(&permission)
			.copied();
		let granted = match explicit {
			Some(decision) => decision,
			None if self.enforce_permissions.load(Ordering::SeqCst) => false,
			None => policy.granted(),
		};
		self.permission_log
			.lock()
			.expect("permission log poisoned")
			.push(PermissionLogEntry { tag: permission, approved: granted, kind });
		granted
	}

	/// Operations a product began and has not ended, in the order they began.
	pub fn open_operations(&self) -> Vec<OpenOperation> {
		self.open_operations.lock().expect("open operations poisoned").clone()
	}

	/// Auth state transitions the core emitted, in order.
	pub fn auth_states(&self) -> Vec<AuthState> {
		self.auth_states.lock().expect("auth states poisoned").clone()
	}

	/// Raw JSON-RPC requests the core sent over the chain connection.
	pub fn sent_rpc(&self) -> Vec<String> {
		self.sent_rpc.lock().expect("sent rpc poisoned").clone()
	}

	/// Seed a preimage so a later [`PreimageHost::lookup_preimage`] resolves it.
	///
	/// On main the core (not the host) builds, signs, and submits the Bulletin
	/// `TransactionStorage.store` transaction; the host's only preimage duty is
	/// content retrieval. This helper lets tests and host simulators pre-load
	/// the content-addressed store the mock's `lookup_preimage` reads from, and
	/// returns the blake2b-256 content address of the value, which is the key
	/// the core looks it up under. Lookups already open for that key receive
	/// the value.
	pub fn insert_preimage(&self, value: Vec<u8>) -> Vec<u8> {
		let key = preimage_key(&value);
		// Held across the store and the push, as `lookup_preimage` holds it
		// across its read and its registration, so no lookup falls between them.
		let mut subscribers =
			self.preimage_subscribers.lock().expect("preimage subscribers poisoned");
		let previous = self
			.preimages
			.lock()
			.expect("preimages poisoned")
			.insert(key.clone(), value.clone());
		// A key names its content, so a value already held is already what every
		// open lookup was given.
		if previous.is_some() {
			return key;
		}
		if let Some(senders) = subscribers.get_mut(&key) {
			senders.retain(|sender| sender.unbounded_send(Some(value.clone())).is_ok());
			if senders.is_empty() {
				subscribers.remove(&key);
			}
		}
		key
	}

	/// Store `value` under `stored_key`, the key the core reads product storage
	/// by, as an entry persisted from an earlier session would be, and notify
	/// that key's subscribers.
	pub fn write_product_storage(&self, stored_key: String, value: Vec<u8>) {
		self.storage
			.lock()
			.expect("storage poisoned")
			.insert(product_key(&stored_key), value.clone());
		self.publish_storage(&stored_key, Some(value));
	}

	/// Drop the recorded navigations.
	pub fn clear_navigations(&self) {
		self.navigations.lock().expect("navigations poisoned").clear();
	}

	/// Drop the recorded shown and cancelled notifications.
	pub fn clear_notifications(&self) {
		self.notifications.lock().expect("notifications poisoned").clear();
		self.cancelled_notifications.lock().expect("cancellations poisoned").clear();
	}

	/// Drop the recorded confirmation reviews.
	pub fn clear_reviews(&self) {
		self.reviews.lock().expect("reviews poisoned").clear();
	}

	/// Drop the recorded permission answers, keeping explicit grants.
	pub fn clear_permission_log(&self) {
		self.permission_log.lock().expect("permission log poisoned").clear();
	}

	/// Drop every explicit permission grant and denial, restoring policy
	/// fallback for all permissions.
	pub fn clear_permission_decisions(&self) {
		self.permission_decisions.lock().expect("permission decisions poisoned").clear();
	}

	/// Drop the recorded auth-state transitions.
	pub fn clear_auth_states(&self) {
		self.auth_states.lock().expect("auth states poisoned").clear();
	}

	/// Drop the recorded outbound JSON-RPC requests.
	pub fn clear_sent_rpc(&self) {
		self.sent_rpc.lock().expect("sent rpc poisoned").clear();
	}

	/// Drop the seeded preimages.
	pub fn clear_preimages(&self) {
		self.preimages.lock().expect("preimages poisoned").clear();
	}

	/// Drop the product and core storage contents.
	pub fn clear_storage(&self) {
		self.storage.lock().expect("storage poisoned").clear();
	}

	/// Product-scoped storage the core has written, as `(key, value)` in key
	/// order, with the internal namespace prefix stripped.
	///
	/// A test asserting what the product stored should read it here rather
	/// than reach into whatever the host keeps underneath: the namespacing is
	/// an implementation detail and tying a suite to it is what makes a host
	/// impossible to replace.
	pub fn product_storage(&self) -> Vec<(String, Vec<u8>)> {
		let prefix = product_key("");
		let mut entries: Vec<(String, Vec<u8>)> = self
			.storage
			.lock()
			.expect("storage poisoned")
			.iter()
			.filter_map(|(key, value)| {
				key.strip_prefix(&prefix).map(|bare| (bare.to_string(), value.clone()))
			})
			.collect();
		entries.sort_by(|(left, _), (right, _)| left.cmp(right));
		entries
	}

	/// Seeded preimages as `(key, value)`, in key order.
	pub fn preimages(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
		let mut entries: Vec<(Vec<u8>, Vec<u8>)> = self
			.preimages
			.lock()
			.expect("preimages poisoned")
			.iter()
			.map(|(key, value)| (key.clone(), value.clone()))
			.collect();
		entries.sort_by(|(left, _), (right, _)| left.cmp(right));
		entries
	}

	/// The theme the mock currently reports.
	pub fn theme(&self) -> latest::ThemeVariant {
		*self.theme.lock().expect("theme poisoned")
	}

	/// Set what the next product scan answers.
	pub fn set_scan_answer(&self, answer: HostScan) {
		*self.scan_answer.lock().expect("scan answer poisoned") = answer;
	}

	/// Answer reviews from now on with `confirm`. A review already answered
	/// keeps its answer, and so does a permission the core has cached.
	/// [`MockPlatform::reset`] restores the configured value.
	pub fn set_confirm_user_actions(&self, confirm: bool) {
		self.confirming.store(confirm, Ordering::SeqCst);
	}

	/// Replace the reported theme and push it to every live subscriber.
	pub fn set_theme(&self, variant: latest::ThemeVariant) {
		*self.theme.lock().expect("theme poisoned") = variant;
		let item = latest::HostThemeSubscribeItem { name: latest::ThemeName::Default, variant };
		self.theme_subscribers
			.lock()
			.expect("theme subscribers poisoned")
			.retain(|subscriber| subscriber.unbounded_send(item.clone()).is_ok());
	}

	/// State of the mock's chain connection.
	pub fn chain_status(&self) -> ChainStatus {
		*self.chain_status.lock().expect("chain status poisoned")
	}

	/// End every live chain connection's response stream and fail subsequent
	/// `connect` calls, as a dropped transport would.
	pub fn simulate_disconnect(&self) {
		*self.chain_status.lock().expect("chain status poisoned") = ChainStatus::Disconnected;
		// Dropping each sender completes the `take_until` future on that
		// connection's response stream, which is what ends it.
		self.chain_disconnectors.lock().expect("chain disconnectors poisoned").clear();
	}

	/// Allow `connect` to succeed again after a simulated disconnect.
	///
	/// Connections closed by the disconnect stay closed: the core reconnects
	/// by opening a new one, which is what this makes possible.
	pub fn simulate_reconnect(&self) {
		*self.chain_status.lock().expect("chain status poisoned") = ChainStatus::Idle;
	}

	/// Chat rooms the product registered, in room-id order.
	pub fn chat_rooms(&self) -> Vec<latest::ChatRoom> {
		self.chat_rooms.lock().expect("chat rooms poisoned").values().cloned().collect()
	}

	/// Chat bots the product registered, in bot-id order.
	pub fn chat_bots(&self) -> Vec<latest::HostChatRegisterBotRequest> {
		self.chat_bots.lock().expect("chat bots poisoned").values().cloned().collect()
	}

	/// Messages the product posted, in order, with the ids the mock assigned.
	pub fn posted_chat_messages(&self) -> Vec<ChatMessageRecord> {
		self.chat_messages.lock().expect("chat messages poisoned").clone()
	}

	/// Drop the registered rooms and bots and the posted-message log.
	///
	/// Live `subscribe_chat_rooms` streams stay open and see the emptied list,
	/// the same as they would see any other replacement.
	pub fn clear_chat_state(&self) {
		self.chat_rooms.lock().expect("chat rooms poisoned").clear();
		self.chat_bots.lock().expect("chat bots poisoned").clear();
		self.chat_messages.lock().expect("chat messages poisoned").clear();
		self.publish_chat_rooms();
	}

	/// Send the current room list to every live subscriber, dropping the ones
	/// whose stream has been closed.
	fn publish_chat_rooms(&self) {
		let item = latest::HostChatListSubscribeItem { rooms: self.chat_rooms() };
		self.chat_room_subscribers
			.lock()
			.expect("chat subscribers poisoned")
			.retain(|subscriber| subscriber.unbounded_send(item.clone()).is_ok());
	}

	/// Push a product key's new value to every live subscriber of that key.
	fn publish_storage(&self, key: &str, value: Option<Vec<u8>>) {
		let item = latest::HostLocalStorageChangeItem { value };
		let mut subscribers =
			self.storage_subscribers.lock().expect("storage subscribers poisoned");
		let Some(senders) = subscribers.get_mut(key) else {
			return;
		};
		senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
		if senders.is_empty() {
			subscribers.remove(key);
		}
	}

	/// Return the mock to its freshly-constructed state, keeping the
	/// [`MockConfig`] it was built with.
	///
	/// Tests reset between cases; doing it in one call is what keeps a
	/// recording from one case out of the assertions of the next.
	pub fn reset(&self) {
		self.clear_navigations();
		self.clear_notifications();
		self.clear_reviews();
		self.clear_permission_log();
		self.clear_permission_decisions();
		self.clear_auth_states();
		self.clear_sent_rpc();
		self.clear_preimages();
		self.clear_storage();
		self.clear_chat_state();
		self.set_theme(self.config.theme);
		self.simulate_reconnect();
		self.set_enforce_permissions(false);
		self.set_confirm_user_actions(self.config.confirm_user_actions);
		self.next_notification_id.store(1, Ordering::SeqCst);
		self.next_chat_message_id.store(0, Ordering::SeqCst);
		self.open_operations.lock().expect("open operations poisoned").clear();
		self.next_operation_id.store(0, Ordering::SeqCst);
	}
}

/// Product keys are namespaced from core slots so neither can shadow the other.
fn product_key(key: &str) -> String {
	format!("product:{key}")
}

/// Lowercase hex for a 32-byte key used in a storage slot name.
fn hex_key(bytes: &[u8; 32]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Stable string key for a typed core-storage slot.
fn core_key(key: &CoreStorageKey) -> String {
	match key {
		CoreStorageKey::AuthSession => "core:auth-session".to_string(),
		CoreStorageKey::PairingDeviceIdentity => "core:pairing-device-identity".to_string(),
		CoreStorageKey::PermissionAuthorization { scope, request } => {
			format!("core:permission:{scope}:{request:?}")
		},
		CoreStorageKey::AutoSigningKey { product_id } => {
			format!("core:auto-signing-key:{product_id}")
		},
		CoreStorageKey::AutoSigningKeys => "core:auto-signing-keys".to_string(),
		CoreStorageKey::RingVrfRegistry { root_public_key } => {
			format!("core:ring-vrf-registry:{}", hex_key(root_public_key))
		},
		CoreStorageKey::StatementRenewalTargets => "core:statement-renewal-targets".to_string(),
		CoreStorageKey::DeviceEncryptionKey => "core:device-encryption-key".to_string(),
		CoreStorageKey::ProductSubtree { session_id, product_id } => {
			format!("core:product-subtree:{session_id}:{product_id}")
		},
		CoreStorageKey::SsoResponderRequestLedger {
			root_public_key,
			peer_statement_account_id,
			peer_encryption_public_key,
		} => {
			format!(
				"core:sso-responder-ledger:{}:{}:{}",
				hex_key(root_public_key),
				hex_key(peer_statement_account_id),
				hex_key(peer_encryption_public_key)
			)
		},
		CoreStorageKey::ProductManifest { product_label } => {
			format!("core:product-manifest:{product_label}")
		},
		CoreStorageKey::WorkerManifest { product_label } => {
			format!("core:worker-manifest:{product_label}")
		},
		CoreStorageKey::FundingSupport => "core:funding-support".to_string(),
		CoreStorageKey::AppManifest { product_label } => {
			format!("core:app-manifest:{product_label}")
		},
		CoreStorageKey::AllowanceKeys { session_id } => {
			format!("core:allowance-keys:{session_id}")
		},
		CoreStorageKey::LastProcessedPairingStatement => {
			"core:last-processed-pairing-statement".to_string()
		},
	}
}

/// Content address of a preimage value: blake2b-256 of the raw bytes.
///
/// This is the same key the core derives before it asks the host to look a
/// preimage up, and the core discards any value whose hash does not match the
/// key it asked for. A key computed any other way is therefore unreachable
/// through the core, however well it round-trips against the mock alone.
fn preimage_key(value: &[u8]) -> Vec<u8> {
	sp_crypto_hashing::blake2_256(value).to_vec()
}

#[async_trait::async_trait]
impl ProductStorage for MockPlatform {
	async fn read(
		&self,
		key: String,
	) -> Result<Option<Vec<u8>>, truapi::v01::HostLocalStorageReadError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(truapi::v01::HostLocalStorageReadError::Unknown { reason: reason.clone() });
		}
		Ok(self.storage.lock().expect("storage poisoned").get(&product_key(&key)).cloned())
	}

	async fn write(
		&self,
		key: String,
		value: Vec<u8>,
	) -> Result<(), truapi::v01::HostLocalStorageReadError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(truapi::v01::HostLocalStorageReadError::Unknown { reason: reason.clone() });
		}
		self.storage
			.lock()
			.expect("storage poisoned")
			.insert(product_key(&key), value.clone());
		self.publish_storage(&key, Some(value));
		Ok(())
	}

	async fn clear(&self, key: String) -> Result<(), truapi::v01::HostLocalStorageReadError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(truapi::v01::HostLocalStorageReadError::Unknown { reason: reason.clone() });
		}
		self.storage.lock().expect("storage poisoned").remove(&product_key(&key));
		self.publish_storage(&key, None);
		Ok(())
	}

	fn subscribe_storage(
		&self,
		key: String,
	) -> BoxStream<'static, Result<latest::HostLocalStorageChangeItem, latest::GenericError>> {
		let (sender, receiver) = mpsc::unbounded();
		// Seed the current value before registering, so a subscriber that never
		// sees a write still sees what it subscribed to.
		sender
			.unbounded_send(latest::HostLocalStorageChangeItem {
				value: self
					.storage
					.lock()
					.expect("storage poisoned")
					.get(&product_key(&key))
					.cloned(),
			})
			.expect("a fresh receiver is open");
		self.storage_subscribers
			.lock()
			.expect("storage subscribers poisoned")
			.entry(key)
			.or_default()
			.push(sender);
		Box::pin(receiver.map(Ok))
	}
}

#[async_trait::async_trait]
impl CoreStorage for MockPlatform {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, latest::GenericError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		Ok(self.storage.lock().expect("storage poisoned").get(&core_key(&key)).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), latest::GenericError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		self.storage.lock().expect("storage poisoned").insert(core_key(&key), value);
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), latest::GenericError> {
		if let Some(reason) = &self.config.faults.storage_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		self.storage.lock().expect("storage poisoned").remove(&core_key(&key));
		Ok(())
	}
}

#[async_trait::async_trait]
impl Navigation for MockPlatform {
	async fn navigate_to(&self, url: String) -> Result<(), latest::HostNavigateToError> {
		if let Some(reason) = &self.config.faults.navigate_error {
			return Err(latest::HostNavigateToError::Unknown { reason: reason.clone() });
		}
		self.navigations.lock().expect("navigations poisoned").push(url);
		Ok(())
	}
}

#[async_trait::async_trait]
impl Notifications for MockPlatform {
	async fn push_notification(
		&self,
		notification: latest::HostPushNotificationRequest,
	) -> Result<latest::HostPushNotificationResponse, latest::GenericError> {
		if let Some(reason) = &self.config.faults.notification_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		self.notifications.lock().expect("notifications poisoned").push(notification);
		let id = self.next_notification_id.fetch_add(1, Ordering::SeqCst);
		Ok(latest::HostPushNotificationResponse { id })
	}

	async fn cancel_notification(&self, id: u32) -> Result<(), latest::GenericError> {
		self.cancelled_notifications.lock().expect("cancellations poisoned").push(id);
		Ok(())
	}
}

/// Placeholder genesis hashes for the chains a mock host serves.
///
/// The TypeScript mock exports the same three under `MOCK_GENESIS`, byte for
/// byte, so a fixture naming a chain by hash means the same chain to both.
/// They are placeholders and match no real network.
pub mod mock_genesis {
	/// Genesis hash the mock reports for the people chain.
	pub const PEOPLE: truapi::Bytes32 = [0x11; 32];
	/// Genesis hash the mock reports for the bulletin chain.
	pub const BULLETIN: truapi::Bytes32 = [0x22; 32];
	/// Genesis hash the mock reports for the asset hub chain.
	pub const ASSET_HUB: truapi::Bytes32 = [0x33; 32];

	/// The chain set a mock host serves by default.
	pub fn chain_set() -> crate::platform::HostChainSet {
		use truapi::latest::ChainIdentifier;
		crate::platform::HostChainSet {
			network: "mock".to_string(),
			chains: vec![
				crate::platform::HostChainEntry {
					identifier: ChainIdentifier::People,
					genesis_hash: PEOPLE,
				},
				crate::platform::HostChainEntry {
					identifier: ChainIdentifier::Bulletin,
					genesis_hash: BULLETIN,
				},
				crate::platform::HostChainEntry {
					identifier: ChainIdentifier::AssetHub,
					genesis_hash: ASSET_HUB,
				},
			],
		}
	}
}

/// A mock policy is two-valued, so a grant is durable and a refusal is durable.
/// `AllowOnce` is a host answer the mock has no knob to ask for.
fn decision(granted: bool) -> PermissionDecision {
	if granted { PermissionDecision::AllowAlways } else { PermissionDecision::Deny }
}

#[async_trait::async_trait]
impl Permissions for MockPlatform {
	async fn device_permission(
		&self,
		_product: &ProductContext,
		request: latest::HostDevicePermissionRequest,
	) -> Result<PermissionDecision, latest::GenericError> {
		if let Some(reason) = &self.config.faults.permission_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		// Keyed by variant name, not the prose `Display`, so a grant can be set
		// ahead of the request. The generated TypeScript mock uses the same keys.
		let key: &'static str = (&request).into();
		Ok(decision(self.decide_permission(
			PermissionKind::Device,
			key.to_string(),
			self.config.device_permissions,
		)))
	}

	async fn remote_permission(
		&self,
		_product: &ProductContext,
		request: latest::RemotePermissionRequest,
	) -> Result<PermissionDecision, latest::GenericError> {
		if let Some(reason) = &self.config.faults.permission_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		let key: &'static str = (&request.permission).into();
		Ok(decision(self.decide_permission(
			PermissionKind::Remote,
			key.to_string(),
			self.config.remote_permissions,
		)))
	}
}

impl crate::platform::ExtrinsicVersionHost for MockPlatform {}

#[async_trait::async_trait]
impl Features for MockPlatform {
	async fn supported_chains(
		&self,
	) -> Result<crate::platform::HostChainSet, latest::GenericError> {
		if let Some(reason) = &self.config.faults.feature_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		Ok(self.config.supported_chains.clone())
	}

	async fn feature_supported(
		&self,
		request: latest::HostFeatureSupportedRequest,
	) -> Result<latest::HostFeatureSupportedResponse, latest::GenericError> {
		if let Some(reason) = &self.config.faults.feature_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		let latest::HostFeatureSupportedRequest::Chain { genesis_hash } = request;
		let served = !self.config.only_served_chains ||
			self.config
				.supported_chains
				.chains
				.iter()
				.any(|entry| entry.genesis_hash.as_slice() == genesis_hash.as_slice());
		Ok(latest::HostFeatureSupportedResponse {
			supported: self.config.feature_supported && served,
		})
	}
}

/// A configurable chain connection: records outbound requests, and either
/// stays silent (`responses` `None`) or replays canned frames.
struct MockConnection {
	sent: Arc<Mutex<Vec<String>>>,
	responses: Option<Vec<String>>,
	/// Taken by the first `responses()` call; firing ends that stream.
	disconnected: Mutex<Option<mpsc::UnboundedReceiver<()>>>,
}

impl JsonRpcConnection for MockConnection {
	fn send(&self, request: String) {
		self.sent.lock().expect("sent rpc poisoned").push(request);
	}

	fn responses(&self) -> BoxStream<'static, String> {
		let base: BoxStream<'static, String> = match &self.responses {
			None => Box::pin(stream::pending()),
			Some(frames) => Box::pin(stream::iter(frames.clone())),
		};
		let Some(disconnected) =
			self.disconnected.lock().expect("disconnect signal poisoned").take()
		else {
			// Only the first stream carries the signal; later ones behave as
			// before, which is enough for the single-stream core.
			return base;
		};
		Box::pin(base.take_until(disconnected.into_future()))
	}

	// No real transport to release. A `None` (silent) `responses()` stream stays
	// pending after close(); scripted/`Closed` behaviors terminate on their own.
	fn close(&self) {}
}

#[async_trait::async_trait]
impl ChainProvider for MockPlatform {
	async fn connect(
		&self,
		_genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		if self.chain_status() == ChainStatus::Disconnected {
			return Err(ProviderError::Host { reason: "mock chain is disconnected".to_string() });
		}
		if let ChainBehavior::ConnectError(reason) = &self.config.chain {
			return Err(ProviderError::Host { reason: reason.clone() });
		}
		let responses = match &self.config.chain {
			ChainBehavior::Silent => None,
			ChainBehavior::Scripted(frames) => Some(frames.clone()),
			ChainBehavior::Closed => Some(Vec::new()),
			ChainBehavior::ConnectError(_) => unreachable!("handled above"),
		};
		let (disconnector, disconnected) = mpsc::unbounded();
		self.chain_disconnectors
			.lock()
			.expect("chain disconnectors poisoned")
			.push(disconnector);
		*self.chain_status.lock().expect("chain status poisoned") = ChainStatus::Connected;
		Ok(Box::new(MockConnection {
			sent: self.sent_rpc.clone(),
			responses,
			disconnected: Mutex::new(Some(disconnected)),
		}))
	}
}

impl AuthPresenter for MockPlatform {
	fn auth_state_changed(&self, state: AuthState) {
		self.auth_states.lock().expect("auth states poisoned").push(state);
	}
}

#[async_trait::async_trait]
impl UserConfirmation for MockPlatform {
	async fn confirm_user_action(
		&self,
		review: UserConfirmationReview,
	) -> Result<bool, latest::GenericError> {
		self.reviews.lock().expect("reviews poisoned").push(review);
		if let Some(reason) = &self.config.faults.confirmation_error {
			return Err(latest::GenericError { reason: reason.clone() });
		}
		Ok(self.confirming.load(Ordering::SeqCst))
	}
}

impl ThemeHost for MockPlatform {
	fn subscribe_theme(
		&self,
	) -> BoxStream<'static, Result<latest::HostThemeSubscribeItem, latest::GenericError>> {
		let (sender, receiver) = mpsc::unbounded();
		// Seed the current theme before registering, so a subscriber that
		// never sees a change still sees what it subscribed to.
		sender
			.unbounded_send(latest::HostThemeSubscribeItem {
				name: latest::ThemeName::Default,
				variant: self.theme(),
			})
			.expect("a fresh receiver is open");
		self.theme_subscribers.lock().expect("theme subscribers poisoned").push(sender);
		Box::pin(receiver.map(Ok))
	}
}

impl LocaleHost for MockPlatform {
	fn subscribe_locale(
		&self,
	) -> BoxStream<'static, Result<latest::HostLocaleSubscribeItem, latest::GenericError>> {
		let item =
			latest::HostLocaleSubscribeItem { language_tag: self.config.language_tag.clone() };
		Box::pin(
			stream::once(async move {
				Ok::<latest::HostLocaleSubscribeItem, latest::GenericError>(item)
			})
			.chain(stream::pending::<
				Result<latest::HostLocaleSubscribeItem, latest::GenericError>,
			>()),
		)
	}
}

impl PreimageHost for MockPlatform {
	fn lookup_preimage(
		&self,
		key: [u8; 32],
	) -> BoxStream<'static, Result<Option<Vec<u8>>, latest::GenericError>> {
		let key = key.to_vec();
		let (sender, receiver) = mpsc::unbounded();
		// The subscribers lock is held across the read and the registration, so
		// an insert cannot land between them unseen.
		let mut subscribers =
			self.preimage_subscribers.lock().expect("preimage subscribers poisoned");
		sender
			.unbounded_send(self.preimages.lock().expect("preimages poisoned").get(&key).cloned())
			.expect("a fresh receiver is open");
		subscribers.entry(key.clone()).or_default().push(sender);
		drop(subscribers);
		Box::pin(
			PreimageLookup { receiver, key, subscribers: self.preimage_subscribers.clone() }
				.map(Ok),
		)
	}
}

#[async_trait::async_trait]
impl ProductOperations for MockPlatform {
	async fn begin_operation(
		&self,
		product: &ProductContext,
		label: String,
	) -> Result<latest::HostWorkerBeginOperationResponse, latest::HostWorkerOperationError> {
		let id = self.next_operation_id.fetch_add(1, Ordering::SeqCst);
		self.open_operations
			.lock()
			.expect("open operations poisoned")
			.push(OpenOperation { product_id: product.product_id.clone(), id, label });
		Ok(latest::HostWorkerBeginOperationResponse { id })
	}

	async fn end_operation(
		&self,
		product: &ProductContext,
		id: u32,
	) -> Result<(), latest::HostWorkerOperationError> {
		// Idempotent by contract, so an unknown or already-ended id is `Ok`.
		self.open_operations
			.lock()
			.expect("open operations poisoned")
			.retain(|open| open.id != id || open.product_id != product.product_id);
		Ok(())
	}
}

#[async_trait::async_trait]
impl ScannerPlatform for MockPlatform {
	async fn scan_code(
		&self,
		_product: &ProductContext,
		_request: &latest::HostScannerScanRequest,
	) -> Result<HostScan, latest::GenericError> {
		Ok(self.scan_answer.lock().expect("scan answer poisoned").clone())
	}
}

#[async_trait::async_trait]
impl ChatPlatform for MockPlatform {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		request: latest::HostChatCreateRoomRequest,
	) -> Result<latest::HostChatCreateRoomResponse, latest::HostChatCreateRoomError> {
		if let Some(reason) = &self.config.faults.chat_error {
			return Err(latest::HostChatCreateRoomError::Unknown { reason: reason.clone() });
		}
		let status = {
			let mut rooms = self.chat_rooms.lock().expect("chat rooms poisoned");
			if rooms.contains_key(&request.room_id) {
				latest::ChatRoomRegistrationStatus::Exists
			} else {
				rooms.insert(
					request.room_id.clone(),
					latest::ChatRoom {
						room_id: request.room_id.clone(),
						// A product that creates a room hosts it; a product
						// reaching a room as a bot registers the bot instead.
						participating_as: latest::ChatRoomParticipation::RoomHost,
					},
				);
				latest::ChatRoomRegistrationStatus::New
			}
		};
		if status == latest::ChatRoomRegistrationStatus::New {
			self.publish_chat_rooms();
		}
		Ok(latest::HostChatCreateRoomResponse { status })
	}

	async fn register_chat_bot(
		&self,
		_product: &ProductContext,
		request: latest::HostChatRegisterBotRequest,
	) -> Result<latest::HostChatRegisterBotResponse, latest::HostChatRegisterBotError> {
		if let Some(reason) = &self.config.faults.chat_error {
			return Err(latest::HostChatRegisterBotError::Unknown { reason: reason.clone() });
		}
		let mut bots = self.chat_bots.lock().expect("chat bots poisoned");
		let status = if bots.contains_key(&request.bot_id) {
			latest::ChatBotRegistrationStatus::Exists
		} else {
			bots.insert(request.bot_id.clone(), request);
			latest::ChatBotRegistrationStatus::New
		};
		Ok(latest::HostChatRegisterBotResponse { status })
	}

	async fn post_chat_message(
		&self,
		_product: &ProductContext,
		request: latest::HostChatPostMessageRequest,
	) -> Result<latest::HostChatPostMessageResponse, latest::HostChatPostMessageError> {
		if let Some(reason) = &self.config.faults.chat_error {
			return Err(latest::HostChatPostMessageError::Unknown { reason: reason.clone() });
		}
		// Posting to a room the product never registered is a product bug, and
		// a mock that silently accepted it would hide one.
		if !self
			.chat_rooms
			.lock()
			.expect("chat rooms poisoned")
			.contains_key(&request.room_id)
		{
			return Err(latest::HostChatPostMessageError::Unknown {
				reason: format!("unknown chat room {}", request.room_id),
			});
		}
		let message_id =
			format!("mock-message:{}", self.next_chat_message_id.fetch_add(1, Ordering::SeqCst));
		self.chat_messages
			.lock()
			.expect("chat messages poisoned")
			.push(ChatMessageRecord {
				message_id: message_id.clone(),
				room_id: request.room_id,
				payload: request.payload,
				alt: request.alt,
			});
		Ok(latest::HostChatPostMessageResponse { message_id })
	}

	async fn set_chat_room_footer(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatSetRoomFooterRequest,
	) -> Result<(), latest::GenericError> {
		match &self.config.faults.chat_error {
			Some(reason) => Err(latest::GenericError { reason: reason.clone() }),
			None => Ok(()),
		}
	}

	fn subscribe_chat_rooms(
		&self,
		_product: &ProductContext,
	) -> BoxStream<'static, Result<latest::HostChatListSubscribeItem, latest::GenericError>> {
		// The other chat calls fail with this reason, so the subscription
		// reports it too rather than handing back a stream that looks healthy
		// and never carries the rooms a failing host would refuse to list.
		if let Some(reason) = &self.config.faults.chat_error {
			return Box::pin(stream::iter([Err(latest::GenericError { reason: reason.clone() })]));
		}
		let (sender, receiver) = mpsc::unbounded();
		// Seed the current list before registering, so a subscriber that never
		// sees a change still sees the state it subscribed to.
		sender
			.unbounded_send(latest::HostChatListSubscribeItem { rooms: self.chat_rooms() })
			.expect("a fresh receiver is open");
		self.chat_room_subscribers
			.lock()
			.expect("chat subscribers poisoned")
			.push(sender);
		Box::pin(receiver.map(Ok))
	}
}

#[cfg(test)]
mod tests;
