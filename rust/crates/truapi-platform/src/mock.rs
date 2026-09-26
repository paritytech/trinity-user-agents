//! In-memory mock [`Platform`](crate::Platform) for tests and host simulators.
//!
//! `MockPlatform` implements every capability trait with deterministic,
//! configurable behavior and no OS, device, or network dependency: storage is
//! an in-memory map, permission prompts answer from a fixed per-capability
//! policy (no UI), navigation and notifications are recorded, and chain access
//! returns a configurable connection. Because the protocol logic lives in
//! `truapi-server`, a `MockPlatform` wired into the core yields a faithful host
//! whose only mocked surface is the OS-primitive seam.
//!
//! Behavior is a [`MockConfig`](crate::mock::MockConfig) read on every call: per-capability permission
//! policy, feature support, theme, confirmation answer, [`ChainBehavior`](crate::mock::ChainBehavior), and
//! [`MockFaults`](crate::mock::MockFaults) error injection. Recordings (`navigations`,
//! `pushed_notifications`, `confirmations`, `auth_states`, `sent_rpc`, …) are
//! the test oracles.
//!
//! Signing and login require a paired wallet answering over the statement-store
//! channel. With [`ChainBehavior::Silent`](crate::mock::ChainBehavior::Silent) the chain connection records
//! outbound requests and never answers, so those flows park; use
//! [`ChainBehavior::Scripted`](crate::mock::ChainBehavior::Scripted) to feed canned response frames.
//!
//! Preimage submission is core-owned on current core (the core builds, signs,
//! and submits the Bulletin `TransactionStorage.store` transaction itself), so
//! the mock only implements host-side content retrieval via `lookup_preimage`.
//! Seed retrievable content with [`MockPlatform::insert_preimage`](crate::mock::MockPlatform::insert_preimage).

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::channel::mpsc;
use futures::stream::{self, BoxStream};

use truapi::latest;

use crate::async_trait;
use crate::{
    AuthPresenter, AuthState, ChainProvider, ChatPlatform, CoreStorage, CoreStorageKey, Features,
    HopProvider, JsonRpcConnection, LocaleHost, NativeChatFileExportRequest,
    NativeChatFilePickRequest, NativeChatFilesHost, NativeChatPickedFile, Navigation,
    Notifications, PermissionDecision, Permissions, PreimageHost, ProductContext,
    ProductOperations, ProductStorage, ThemeHost, UserConfirmation, UserConfirmationReview,
};

/// How the mock answers a permission prompt for one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// [`UserConfirmationReview::PreimageSubmit`].
    PreimageSubmit,
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
    /// [`UserConfirmationReview::ChatAuthority`].
    ChatAuthority,
    /// [`UserConfirmationReview::MainPurseChatPayment`].
    MainPurseChatPayment,
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
            UserConfirmationReview::PreimageSubmit(_) => ConfirmKind::PreimageSubmit,
            UserConfirmationReview::IdentityDisclosure(_) => ConfirmKind::IdentityDisclosure,
            UserConfirmationReview::AccountAccess(_) => ConfirmKind::AccountAccess,
            UserConfirmationReview::StatementStoreProductSign(_) => {
                ConfirmKind::StatementStoreProductSign
            }
            UserConfirmationReview::SignVrf(_) => ConfirmKind::SignVrf,
            UserConfirmationReview::ProductSubtree(_) => ConfirmKind::ProductSubtree,
            UserConfirmationReview::ChatAuthority(_) => ConfirmKind::ChatAuthority,
            UserConfirmationReview::MainPurseChatPayment(_) => ConfirmKind::MainPurseChatPayment,
        }
    }
}

/// Which prompt surface a permission decision came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub product_id: String,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessageRecord {
    /// Id the mock assigned and returned to the product.
    pub message_id: String,
    /// Room the message was posted to.
    pub room_id: String,
    /// What was posted.
    pub payload: latest::ChatMessageContent,
}

/// How the mock's chain connection behaves.
#[derive(Debug, Clone, Default)]
pub enum ChainBehavior {
    /// Record outbound requests, never answer. Whether a flow parks turns on
    /// what it needs from the chain rather than on whether it signs:
    /// `sign_raw` completes hermetically, while `create_transaction` and
    /// `sign_payload` park because `build_local_transaction` needs chain
    /// metadata. Login and the statement store park for the same reason. Drive
    /// any test that reaches a parking flow under a timeout; use
    /// [`ChainBehavior::Closed`] to make a disconnect observable instead.
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
#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Answer for `device_permission`.
    pub device_permissions: PermissionPolicy,
    /// Answer for `remote_permission`.
    pub remote_permissions: PermissionPolicy,
    /// Whether `feature_supported` reports support.
    pub feature_supported: bool,
    /// Theme emitted by `subscribe_theme`.
    pub theme: latest::ThemeVariant,
    /// BCP 47 language tag the mock reports.
    pub language_tag: String,
    /// Whether `confirm_user_action` confirms reviewed actions.
    pub confirm_user_actions: bool,
    /// Chains the mock reports serving (RFC 0026).
    ///
    /// Defaults to the three [`mock_genesis`] chains, the same set the
    /// TypeScript mock declares, so a suite written against either one is
    /// answered the same way. Replace it to exercise a host serving fewer
    /// chains, declaring each with the genesis hash its runtime config carries.
    pub supported_chains: crate::HostChainSet,
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
            theme: latest::ThemeVariant::Dark,
            language_tag: "en".to_string(),
            confirm_user_actions: true,
            supported_chains: mock_genesis::chain_set(),
            chain: ChainBehavior::Silent,
            faults: MockFaults::default(),
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
    cancelled_notifications: Arc<Mutex<Vec<latest::NotificationId>>>,
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
    storage_subscribers:
        Arc<Mutex<HashMap<String, Vec<mpsc::UnboundedSender<latest::HostLocalStorageChangeItem>>>>>,
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
            storage_subscribers: Arc::new(Mutex::new(HashMap::new())),
            chain_status: Arc::new(Mutex::new(ChainStatus::Idle)),
            chain_disconnectors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// URLs the core asked the host to open, in order.
    pub fn navigations(&self) -> Vec<String> {
        self.navigations
            .lock()
            .expect("navigations poisoned")
            .clone()
    }

    /// Notifications the core asked the host to show, in order.
    pub fn pushed_notifications(&self) -> Vec<latest::HostPushNotificationRequest> {
        self.notifications
            .lock()
            .expect("notifications poisoned")
            .clone()
    }

    /// Notification ids the core asked the host to cancel, in order.
    pub fn cancelled_notifications(&self) -> Vec<latest::NotificationId> {
        self.cancelled_notifications
            .lock()
            .expect("cancellations poisoned")
            .clone()
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
        self.permission_log
            .lock()
            .expect("permission log poisoned")
            .clone()
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
            .push(PermissionLogEntry {
                tag: permission,
                approved: granted,
                kind,
            });
        granted
    }

    /// Operations a product began and has not ended, in the order they began.
    pub fn open_operations(&self) -> Vec<OpenOperation> {
        self.open_operations
            .lock()
            .expect("open operations poisoned")
            .clone()
    }

    /// Auth state transitions the core emitted, in order.
    pub fn auth_states(&self) -> Vec<AuthState> {
        self.auth_states
            .lock()
            .expect("auth states poisoned")
            .clone()
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
    /// the core looks it up under.
    pub fn insert_preimage(&self, value: Vec<u8>) -> Vec<u8> {
        let key = preimage_key(&value);
        self.preimages
            .lock()
            .expect("preimages poisoned")
            .insert(key.clone(), value);
        key
    }

    /// Drop the recorded navigations.
    pub fn clear_navigations(&self) {
        self.navigations
            .lock()
            .expect("navigations poisoned")
            .clear();
    }

    /// Drop the recorded shown and cancelled notifications.
    pub fn clear_notifications(&self) {
        self.notifications
            .lock()
            .expect("notifications poisoned")
            .clear();
        self.cancelled_notifications
            .lock()
            .expect("cancellations poisoned")
            .clear();
    }

    /// Drop the recorded confirmation reviews.
    pub fn clear_reviews(&self) {
        self.reviews.lock().expect("reviews poisoned").clear();
    }

    /// Drop the recorded permission answers, keeping explicit grants.
    pub fn clear_permission_log(&self) {
        self.permission_log
            .lock()
            .expect("permission log poisoned")
            .clear();
    }

    /// Drop every explicit permission grant and denial, restoring policy
    /// fallback for all permissions.
    pub fn clear_permission_decisions(&self) {
        self.permission_decisions
            .lock()
            .expect("permission decisions poisoned")
            .clear();
    }

    /// Drop the recorded auth-state transitions.
    pub fn clear_auth_states(&self) {
        self.auth_states
            .lock()
            .expect("auth states poisoned")
            .clear();
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
                key.strip_prefix(&prefix)
                    .map(|bare| (bare.to_string(), value.clone()))
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

    /// Replace the reported theme and push it to every live subscriber.
    pub fn set_theme(&self, variant: latest::ThemeVariant) {
        *self.theme.lock().expect("theme poisoned") = variant;
        let item = latest::HostThemeSubscribeItem {
            name: latest::ThemeName::Default,
            variant,
        };
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
        self.chain_disconnectors
            .lock()
            .expect("chain disconnectors poisoned")
            .clear();
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
        self.chat_rooms
            .lock()
            .expect("chat rooms poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Chat bots the product registered, in bot-id order.
    pub fn chat_bots(&self) -> Vec<latest::HostChatRegisterBotRequest> {
        self.chat_bots
            .lock()
            .expect("chat bots poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Messages the product posted, in order, with the ids the mock assigned.
    pub fn posted_chat_messages(&self) -> Vec<ChatMessageRecord> {
        self.chat_messages
            .lock()
            .expect("chat messages poisoned")
            .clone()
    }

    /// Drop the registered rooms and bots and the posted-message log.
    ///
    /// Live `subscribe_chat_rooms` streams stay open and see the emptied list,
    /// the same as they would see any other replacement.
    pub fn clear_chat_state(&self) {
        self.chat_rooms.lock().expect("chat rooms poisoned").clear();
        self.chat_bots.lock().expect("chat bots poisoned").clear();
        self.chat_messages
            .lock()
            .expect("chat messages poisoned")
            .clear();
        self.publish_chat_rooms();
    }

    /// Send the current room list to every live subscriber, dropping the ones
    /// whose stream has been closed.
    fn publish_chat_rooms(&self) {
        let item = latest::HostChatListSubscribeItem {
            rooms: self.chat_rooms(),
        };
        self.chat_room_subscribers
            .lock()
            .expect("chat subscribers poisoned")
            .retain(|subscriber| subscriber.unbounded_send(item.clone()).is_ok());
    }

    /// Push a product key's new value to every live subscriber of that key.
    fn publish_storage(&self, key: &str, value: Option<Vec<u8>>) {
        let item = latest::HostLocalStorageChangeItem { value };
        let mut subscribers = self
            .storage_subscribers
            .lock()
            .expect("storage subscribers poisoned");
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
        self.next_notification_id.store(1, Ordering::SeqCst);
        self.next_chat_message_id.store(0, Ordering::SeqCst);
        self.open_operations
            .lock()
            .expect("open operations poisoned")
            .clear();
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
        CoreStorageKey::PermissionAuthorization {
            product_id,
            request,
        } => format!("core:permission:{product_id}:{request:?}"),
        CoreStorageKey::AutoSigningKey { product_id } => {
            format!("core:auto-signing-key:{product_id}")
        }
        CoreStorageKey::AutoSigningKeys => "core:auto-signing-keys".to_string(),
        CoreStorageKey::RingVrfRegistry { root_public_key } => {
            format!("core:ring-vrf-registry:{}", hex_key(root_public_key))
        }
        CoreStorageKey::StatementRenewalTargets => "core:statement-renewal-targets".to_string(),
        CoreStorageKey::DeviceEncryptionKey => "core:device-encryption-key".to_string(),
        CoreStorageKey::ProductSubtree {
            session_id,
            product_id,
        } => format!("core:product-subtree:{session_id}:{product_id}"),
        CoreStorageKey::SsoResponderRequestLedger {
            root_public_key,
            peer_statement_account_id,
            peer_encryption_public_key,
        } => format!(
            "core:sso-responder-ledger:{}:{}:{}",
            hex_key(root_public_key),
            hex_key(peer_statement_account_id),
            hex_key(peer_encryption_public_key)
        ),
        CoreStorageKey::MainPurseCoinage {
            root_public_key,
            genesis_hash,
        } => format!(
            "core:main-purse-coinage:{}:{}",
            hex_key(root_public_key),
            hex_key(genesis_hash)
        ),
        CoreStorageKey::NativeChatDevice {
            root_public_key,
            genesis_hash,
            product_id,
        } => format!(
            "core:native-chat-device:{}:{}:{product_id}",
            hex_key(root_public_key),
            hex_key(genesis_hash)
        ),
        CoreStorageKey::NativeChatProducts {
            root_public_key,
            genesis_hash,
        } => format!(
            "core:native-chat-products:{}:{}",
            hex_key(root_public_key),
            hex_key(genesis_hash)
        ),
        CoreStorageKey::ProfileDisclosure => "core:profile-disclosure".to_string(),
        CoreStorageKey::ProfileReferencesReceived { product_id } => {
            format!("core:profile-references-received:{product_id}")
        }
        CoreStorageKey::NativeChatFileChunk {
            root_public_key,
            genesis_hash,
            product_id,
            attachment_id,
            ..
        } => format!(
            "core:native-chat-file-chunk:{}:{}:{product_id}:{}",
            hex_key(root_public_key),
            hex_key(genesis_hash),
            hex_key(attachment_id)
        ),
        CoreStorageKey::ProductManifest { product_id } => {
            format!("core:product-manifest:{product_id}")
        }
        CoreStorageKey::AllowanceKeys { session_id } => {
            format!("core:allowance-keys:{session_id}")
        }
        CoreStorageKey::LastProcessedPairingStatement => {
            "core:last-processed-pairing-statement".to_string()
        }
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

#[async_trait]
impl ProductStorage for MockPlatform {
    async fn read(
        &self,
        key: String,
    ) -> Result<Option<Vec<u8>>, truapi::v01::HostLocalStorageReadError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(truapi::v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
        }
        Ok(self
            .storage
            .lock()
            .expect("storage poisoned")
            .get(&product_key(&key))
            .cloned())
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), truapi::v01::HostLocalStorageReadError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(truapi::v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
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
            return Err(truapi::v01::HostLocalStorageReadError::Unknown {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .remove(&product_key(&key));
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

#[async_trait]
impl CoreStorage for MockPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, latest::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(self
            .storage
            .lock()
            .expect("storage poisoned")
            .get(&core_key(&key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), latest::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .insert(core_key(&key), value);
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), latest::GenericError> {
        if let Some(reason) = &self.config.faults.storage_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        self.storage
            .lock()
            .expect("storage poisoned")
            .remove(&core_key(&key));
        Ok(())
    }
}

#[async_trait]
impl Navigation for MockPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), latest::HostNavigateToError> {
        if let Some(reason) = &self.config.faults.navigate_error {
            return Err(latest::HostNavigateToError::Unknown {
                reason: reason.clone(),
            });
        }
        self.navigations
            .lock()
            .expect("navigations poisoned")
            .push(url);
        Ok(())
    }
}

#[async_trait]
impl Notifications for MockPlatform {
    async fn push_notification(
        &self,
        notification: latest::HostPushNotificationRequest,
    ) -> Result<latest::HostPushNotificationResponse, latest::GenericError> {
        if let Some(reason) = &self.config.faults.notification_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        self.notifications
            .lock()
            .expect("notifications poisoned")
            .push(notification);
        let id = self.next_notification_id.fetch_add(1, Ordering::SeqCst);
        Ok(latest::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(
        &self,
        id: latest::NotificationId,
    ) -> Result<(), latest::GenericError> {
        self.cancelled_notifications
            .lock()
            .expect("cancellations poisoned")
            .push(id);
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
    pub fn chain_set() -> crate::HostChainSet {
        use truapi::latest::ChainIdentifier;
        crate::HostChainSet {
            network: "mock".to_string(),
            chains: vec![
                crate::HostChainEntry {
                    identifier: ChainIdentifier::People,
                    genesis_hash: PEOPLE,
                },
                crate::HostChainEntry {
                    identifier: ChainIdentifier::Bulletin,
                    genesis_hash: BULLETIN,
                },
                crate::HostChainEntry {
                    identifier: ChainIdentifier::AssetHub,
                    genesis_hash: ASSET_HUB,
                },
            ],
        }
    }
}

/// Decision key for a device permission: the SCALE variant tag.
///
/// The `Display` impls render prose for a person to read, so they are the
/// wrong thing to store a grant under. The generated TypeScript mock keys on
/// these same tags, which is what lets one suite drive both mocks.
fn device_permission_key(request: &latest::HostDevicePermissionRequest) -> &'static str {
    use latest::HostDevicePermissionRequest as Request;
    match request {
        Request::Notifications => "Notifications",
        Request::Camera => "Camera",
        Request::Microphone => "Microphone",
        Request::Bluetooth => "Bluetooth",
        Request::NFC => "NFC",
        Request::Location => "Location",
        Request::Clipboard => "Clipboard",
        Request::OpenUrl => "OpenUrl",
        Request::Biometrics => "Biometrics",
    }
}

/// Decision key for a remote permission: the SCALE variant tag.
///
/// `Remote` renders its domain list through `Display`, so keying on that form
/// would make the key depend on what the product asked for and leave the
/// permission impossible to grant ahead of the request.
fn remote_permission_key(permission: &latest::RemotePermission) -> &'static str {
    use latest::RemotePermission as Permission;
    match permission {
        Permission::Remote { .. } => "Remote",
        Permission::WebRtc => "WebRtc",
        Permission::ChainSubmit => "ChainSubmit",
        Permission::PreimageSubmit => "PreimageSubmit",
        Permission::StatementSubmit => "StatementSubmit",
    }
}

/// A mock policy is two-valued, so a grant is durable and a refusal is durable.
/// `AllowOnce` is a host answer the mock has no knob to ask for.
fn decision(granted: bool) -> PermissionDecision {
    if granted {
        PermissionDecision::AllowAlways
    } else {
        PermissionDecision::Deny
    }
}

#[async_trait]
impl Permissions for MockPlatform {
    async fn device_permission(
        &self,
        _product: &ProductContext,
        request: latest::HostDevicePermissionRequest,
    ) -> Result<PermissionDecision, latest::GenericError> {
        if let Some(reason) = &self.config.faults.permission_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(decision(self.decide_permission(
            PermissionKind::Device,
            device_permission_key(&request).to_string(),
            self.config.device_permissions,
        )))
    }

    async fn remote_permission(
        &self,
        _product: &ProductContext,
        request: latest::RemotePermissionRequest,
    ) -> Result<PermissionDecision, latest::GenericError> {
        if let Some(reason) = &self.config.faults.permission_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(decision(self.decide_permission(
            PermissionKind::Remote,
            remote_permission_key(&request.permission).to_string(),
            self.config.remote_permissions,
        )))
    }
}

#[async_trait]
impl Features for MockPlatform {
    async fn supported_chains(&self) -> Result<crate::HostChainSet, latest::GenericError> {
        if let Some(reason) = &self.config.faults.feature_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(self.config.supported_chains.clone())
    }

    async fn feature_supported(
        &self,
        _request: latest::HostFeatureSupportedRequest,
    ) -> Result<latest::HostFeatureSupportedResponse, latest::GenericError> {
        if let Some(reason) = &self.config.faults.feature_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(latest::HostFeatureSupportedResponse {
            supported: self.config.feature_supported,
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
        let Some(disconnected) = self
            .disconnected
            .lock()
            .expect("disconnect signal poisoned")
            .take()
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

#[async_trait]
impl ChainProvider for MockPlatform {
    async fn connect(
        &self,
        _genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, latest::GenericError> {
        if self.chain_status() == ChainStatus::Disconnected {
            return Err(latest::GenericError {
                reason: "mock chain is disconnected".to_string(),
            });
        }
        if let ChainBehavior::ConnectError(reason) = &self.config.chain {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
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
        self.auth_states
            .lock()
            .expect("auth states poisoned")
            .push(state);
    }
}

#[async_trait]
impl UserConfirmation for MockPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, latest::GenericError> {
        self.reviews.lock().expect("reviews poisoned").push(review);
        if let Some(reason) = &self.config.faults.confirmation_error {
            return Err(latest::GenericError {
                reason: reason.clone(),
            });
        }
        Ok(self.config.confirm_user_actions)
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
        self.theme_subscribers
            .lock()
            .expect("theme subscribers poisoned")
            .push(sender);
        Box::pin(receiver.map(Ok))
    }
}

impl LocaleHost for MockPlatform {
    fn subscribe_locale(
        &self,
    ) -> BoxStream<'static, Result<latest::HostLocaleSubscribeItem, latest::GenericError>> {
        let item = latest::HostLocaleSubscribeItem {
            language_tag: self.config.language_tag.clone(),
        };
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
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, latest::GenericError>> {
        let found = self
            .preimages
            .lock()
            .expect("preimages poisoned")
            .get(&key)
            .cloned();
        // Emit the current value/miss, then stay open — a live subscription that
        // never ends, matching `subscribe_theme`, the trait doc, and the JS mock.
        Box::pin(
            stream::once(async move { Ok(found) }).chain(stream::pending::<
                Result<Option<Vec<u8>>, latest::GenericError>,
            >()),
        )
    }
}

#[async_trait]
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
            .push(OpenOperation {
                product_id: product.product_id.clone(),
                id,
                label,
            });
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

#[async_trait]
impl NativeChatFilesHost for MockPlatform {
    async fn pick_chat_files(
        &self,
        _request: NativeChatFilePickRequest,
    ) -> Result<Vec<NativeChatPickedFile>, latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn read_chat_file(
        &self,
        _source_id: String,
        _offset: u64,
        _length: u32,
    ) -> Result<Vec<u8>, latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn release_chat_file(&self, _source_id: String) -> Result<(), latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn begin_chat_file_export(
        &self,
        _request: NativeChatFileExportRequest,
    ) -> Result<Option<String>, latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn write_chat_file_export(
        &self,
        _export_id: String,
        _offset: u64,
        _data: Vec<u8>,
    ) -> Result<(), latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn finish_chat_file_export(
        &self,
        _export_id: String,
    ) -> Result<(), latest::GenericError> {
        Err(native_chat_files_unavailable())
    }

    async fn cancel_chat_file_export(
        &self,
        _export_id: String,
    ) -> Result<(), latest::GenericError> {
        Err(native_chat_files_unavailable())
    }
}

#[async_trait]
impl HopProvider for MockPlatform {}

fn native_chat_files_unavailable() -> latest::GenericError {
    latest::GenericError {
        reason: "native chat files unavailable".to_string(),
    }
}

#[async_trait]
impl ChatPlatform for MockPlatform {
    async fn create_chat_room(
        &self,
        _product: &ProductContext,
        request: latest::HostChatCreateRoomRequest,
    ) -> Result<latest::HostChatCreateRoomResponse, latest::HostChatCreateRoomError> {
        if let Some(reason) = &self.config.faults.chat_error {
            return Err(latest::HostChatCreateRoomError::Unknown {
                reason: reason.clone(),
            });
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
            return Err(latest::HostChatRegisterBotError::Unknown {
                reason: reason.clone(),
            });
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
            return Err(latest::HostChatPostMessageError::Unknown {
                reason: reason.clone(),
            });
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
        let message_id = format!(
            "mock-message:{}",
            self.next_chat_message_id.fetch_add(1, Ordering::SeqCst)
        );
        self.chat_messages
            .lock()
            .expect("chat messages poisoned")
            .push(ChatMessageRecord {
                message_id: message_id.clone(),
                room_id: request.room_id,
                payload: request.payload,
            });
        Ok(latest::HostChatPostMessageResponse { message_id })
    }

    fn subscribe_chat_rooms(
        &self,
        _product: &ProductContext,
    ) -> BoxStream<'static, Result<latest::HostChatListSubscribeItem, latest::GenericError>> {
        // The other chat calls fail with this reason, so the subscription
        // reports it too rather than handing back a stream that looks healthy
        // and never carries the rooms a failing host would refuse to list.
        if let Some(reason) = &self.config.faults.chat_error {
            return Box::pin(stream::iter([Err(latest::GenericError {
                reason: reason.clone(),
            })]));
        }
        let (sender, receiver) = mpsc::unbounded();
        // Seed the current list before registering, so a subscriber that never
        // sees a change still sees the state it subscribed to.
        sender
            .unbounded_send(latest::HostChatListSubscribeItem {
                rooms: self.chat_rooms(),
            })
            .expect("a fresh receiver is open");
        self.chat_room_subscribers
            .lock()
            .expect("chat subscribers poisoned")
            .push(sender);
        Box::pin(receiver.map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::executor::block_on;

    /// Decode a lowercase hex string into bytes.
    fn hex_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&hex[index..index + 2], 16).expect("test vector is valid hex")
            })
            .collect()
    }

    fn resource_review() -> UserConfirmationReview {
        UserConfirmationReview::ResourceAllocation(crate::ResourceAllocationReview {
            calling_product_id: "mock.dot".to_string(),
            resources: vec![],
        })
    }

    #[test]
    fn implements_platform() {
        fn assert_platform<P: crate::Platform>(_: &P) {}
        assert_platform(&MockPlatform::new());
    }

    #[test]
    fn product_storage_round_trips() {
        // Namespacing is `core_and_product_keys_do_not_collide`'s subject; this
        // is the write/read/clear cycle on its own.
        let p = MockPlatform::new();
        block_on(p.write("k".into(), vec![1, 2, 3])).unwrap();
        assert_eq!(block_on(p.read("k".into())).unwrap(), Some(vec![1, 2, 3]));
        block_on(p.clear("k".into())).unwrap();
        assert_eq!(block_on(p.read("k".into())).unwrap(), None);
    }

    #[test]
    fn core_storage_round_trips() {
        let p = MockPlatform::new();
        block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![7])).unwrap();
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(),
            Some(vec![7])
        );
        block_on(p.clear_core_storage(CoreStorageKey::AuthSession)).unwrap();
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(),
            None
        );
    }

    #[test]
    fn core_and_product_keys_do_not_collide() {
        let p = MockPlatform::new();
        block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![1])).unwrap();
        // Reading the same logical name as a product key must miss the core slot.
        assert_eq!(block_on(p.read("auth-session".into())).unwrap(), None);
        assert_eq!(block_on(p.read("core:auth-session".into())).unwrap(), None);
        // ...and a product key must not be visible through core storage.
        block_on(p.write("x".into(), vec![2])).unwrap();
        assert_eq!(
            block_on(p.read_core_storage(CoreStorageKey::PairingDeviceIdentity)).unwrap(),
            None
        );
    }

    #[test]
    fn permissions_deny_all_denies_device_and_remote() {
        let p = MockPlatform::with_config(MockConfig {
            device_permissions: PermissionPolicy::DenyAll,
            remote_permissions: PermissionPolicy::DenyAll,
            ..Default::default()
        });
        assert_eq!(
            block_on(p.device_permission(
                &mock_product(),
                latest::HostDevicePermissionRequest::Notifications
            ))
            .unwrap(),
            PermissionDecision::Deny
        );
        assert_eq!(
            block_on(p.remote_permission(
                &mock_product(),
                latest::RemotePermissionRequest {
                    permission: latest::RemotePermission::WebRtc
                }
            ))
            .unwrap(),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn permissions_split_allows_device_denies_remote() {
        let p = MockPlatform::with_config(MockConfig {
            device_permissions: PermissionPolicy::AllowAll,
            remote_permissions: PermissionPolicy::DenyAll,
            ..Default::default()
        });
        assert_eq!(
            block_on(p.device_permission(
                &mock_product(),
                latest::HostDevicePermissionRequest::Notifications
            ))
            .unwrap(),
            PermissionDecision::AllowAlways
        );
        assert_eq!(
            block_on(p.remote_permission(
                &mock_product(),
                latest::RemotePermissionRequest {
                    permission: latest::RemotePermission::WebRtc
                }
            ))
            .unwrap(),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn remote_permission_granted_when_allowed() {
        let p = MockPlatform::with_config(MockConfig {
            remote_permissions: PermissionPolicy::AllowAll,
            ..Default::default()
        });
        assert_eq!(
            block_on(p.remote_permission(
                &mock_product(),
                latest::RemotePermissionRequest {
                    permission: latest::RemotePermission::WebRtc
                }
            ))
            .unwrap(),
            PermissionDecision::AllowAlways
        );
    }

    #[test]
    fn navigation_records_and_can_error() {
        let p = MockPlatform::new();
        block_on(p.navigate_to("a".into())).unwrap();
        block_on(p.navigate_to("b".into())).unwrap();
        assert_eq!(p.navigations(), vec!["a".to_string(), "b".to_string()]);

        let p2 = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                navigate_error: Some("blocked".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(block_on(p2.navigate_to("c".into())).is_err());
    }

    #[test]
    fn notifications_record_order_with_unique_ids() {
        let p = MockPlatform::new();
        let make = |text: &str| latest::HostPushNotificationRequest {
            text: text.to_string(),
            deeplink: None,
            scheduled_at: None,
        };
        let id0 = block_on(p.push_notification(make("one"))).unwrap().id;
        let id1 = block_on(p.push_notification(make("two"))).unwrap().id;
        // From 1, not 0: a product that reads 0 as "no id" cannot cancel the
        // first notification it schedules, and the JS mock starts at 1 too.
        assert_eq!((id0, id1), (1, 2));
        assert_eq!(p.pushed_notifications().len(), 2);
        block_on(p.cancel_notification(id1)).unwrap();
        assert_eq!(p.cancelled_notifications(), vec![id1]);
    }

    #[test]
    fn notification_error_injected() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                notification_error: Some("denied".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        let request = latest::HostPushNotificationRequest {
            text: "x".into(),
            deeplink: None,
            scheduled_at: None,
        };
        assert!(block_on(p.push_notification(request)).is_err());
    }

    #[test]
    fn a_storage_subscription_sees_the_current_value_and_later_writes() {
        let p = MockPlatform::new();
        block_on(p.write("k".into(), vec![1])).expect("seed write succeeds");

        // `now_or_never`, not `block_on`: the fan-out is a synchronous send, so
        // an item that is not already there is never coming, and awaiting one
        // would hang this test instead of failing it.
        let mut stream = p.subscribe_storage("k".into());
        assert_eq!(next_value(&mut stream), Some(Some(vec![1])), "seeded value");

        block_on(p.write("k".into(), vec![2])).expect("write succeeds");
        assert_eq!(
            next_value(&mut stream),
            Some(Some(vec![2])),
            "write reaches"
        );

        block_on(p.clear("k".into())).expect("clear succeeds");
        assert_eq!(next_value(&mut stream), Some(None), "clear reaches");
    }

    /// The next item already queued on a storage subscription, or `None` when
    /// nothing is queued. The outer `Option` is arrival, the inner is the value.
    fn next_value(
        stream: &mut BoxStream<
            'static,
            Result<latest::HostLocalStorageChangeItem, latest::GenericError>,
        >,
    ) -> Option<Option<Vec<u8>>> {
        stream
            .next()
            .now_or_never()
            .flatten()
            .map(|item| item.expect("a mock subscription never errors").value)
    }

    #[test]
    fn a_storage_subscription_hears_nothing_from_another_key() {
        // Per-key, not a single broadcast: a subscriber woken by every write
        // would make a test asserting "no change" pass for the wrong reason.
        let p = MockPlatform::new();
        let mut stream = p.subscribe_storage("watched".into());
        assert_eq!(next_value(&mut stream), Some(None), "seeded value");

        block_on(p.write("other".into(), vec![9])).expect("write succeeds");
        assert_eq!(
            next_value(&mut stream),
            None,
            "a write to another key woke this subscription"
        );
    }

    #[test]
    fn an_operation_is_open_until_its_own_product_ends_it() {
        let p = MockPlatform::new();
        let mine = mock_product();
        let theirs = ProductContext::new("other.dot".to_string()).expect("product id is valid");

        let id = block_on(p.begin_operation(&mine, "sync".into()))
            .expect("begin succeeds")
            .id;
        assert_eq!(
            p.open_operations(),
            vec![OpenOperation {
                product_id: "mock.dot".to_string(),
                id,
                label: "sync".to_string(),
            }]
        );

        // Ending is per product: another product holding the same id must not
        // drop this one's demand.
        block_on(p.end_operation(&theirs, id)).expect("a foreign end is Ok");
        assert_eq!(p.open_operations().len(), 1);

        block_on(p.end_operation(&mine, id)).expect("end succeeds");
        assert!(p.open_operations().is_empty());
        // Idempotent by contract.
        block_on(p.end_operation(&mine, id)).expect("a repeat end is Ok");
    }

    #[test]
    fn storage_error_injected() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                storage_error: Some("disk".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(block_on(p.read("k".into())).is_err());
        assert!(block_on(p.read_core_storage(CoreStorageKey::AuthSession)).is_err());
    }

    #[test]
    fn confirm_records_kind_and_answers() {
        let p = MockPlatform::new();
        assert!(block_on(p.confirm_user_action(resource_review())).unwrap());
        assert_eq!(p.confirmations(), vec![ConfirmKind::ResourceAllocation]);

        let p2 = MockPlatform::with_config(MockConfig {
            confirm_user_actions: false,
            ..Default::default()
        });
        assert!(!block_on(p2.confirm_user_action(resource_review())).unwrap());
    }

    #[test]
    fn feature_supported_reflects_config() {
        let p = MockPlatform::with_config(MockConfig {
            feature_supported: false,
            ..Default::default()
        });
        let response = block_on(
            p.feature_supported(latest::HostFeatureSupportedRequest::Chain {
                genesis_hash: vec![0; 32],
            }),
        )
        .unwrap();
        assert!(!response.supported);
    }

    #[test]
    fn theme_emits_configured_variant_then_stays_open() {
        let p = MockPlatform::new();
        let mut stream = p.subscribe_theme();
        assert_eq!(
            block_on(stream.next()).unwrap().unwrap().variant,
            latest::ThemeVariant::Dark
        );
        // A live subscription does not end after the current value.
        assert!(stream.next().now_or_never().is_none());
    }

    #[test]
    fn theme_emits_configured_light() {
        let p = MockPlatform::with_config(MockConfig {
            theme: latest::ThemeVariant::Light,
            ..Default::default()
        });
        assert_eq!(
            block_on(p.subscribe_theme().next())
                .unwrap()
                .unwrap()
                .variant,
            latest::ThemeVariant::Light
        );
    }

    #[test]
    fn preimage_insert_then_lookup_round_trips() {
        let p = MockPlatform::new();
        // The core owns Bulletin submission on main; the host only retrieves
        // content, so tests seed the content-addressed store directly.
        let key = p.insert_preimage(vec![1, 2, 3]);
        // The key is the content address the core asks for, not an arbitrary
        // digest: the core recomputes blake2b-256 over whatever comes back and
        // reports a mismatch as a miss, so a key derived any other way makes
        // every seeded preimage unreachable through the core. The expected
        // value is the published blake2b-256 of `[1, 2, 3]`, so this fails if
        // the algorithm changes even where both sides change together.
        assert_eq!(
            key,
            hex_bytes("11c0e79b71c3976ccd0c02d1310e2516c08edc9d8b6f57ccd680d63a4d8e72da")
        );
        let found = block_on(p.lookup_preimage(key).next()).unwrap().unwrap();
        assert_eq!(found, Some(vec![1, 2, 3]));
        // An unknown key misses.
        let miss = block_on(p.lookup_preimage(vec![9; 32]).next())
            .unwrap()
            .unwrap();
        assert_eq!(miss, None);
    }

    #[test]
    fn chain_silent_records_sends_and_parks() {
        let p = MockPlatform::new();
        let conn = block_on(p.connect([0u8; 32])).unwrap();
        conn.send("req-1".to_string());
        assert_eq!(p.sent_rpc(), vec!["req-1".to_string()]);
        // Silent: the response stream never yields (parks rather than ends).
        assert!(conn.responses().next().now_or_never().is_none());
    }

    #[test]
    fn chain_scripted_replays_frames() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::Scripted(vec!["frame-1".into(), "frame-2".into()]),
            ..Default::default()
        });
        let conn = block_on(p.connect([0u8; 32])).unwrap();
        let frames: Vec<String> = block_on(conn.responses().collect());
        assert_eq!(frames, vec!["frame-1".to_string(), "frame-2".to_string()]);
    }

    #[test]
    fn chain_closed_ends_stream_immediately() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::Closed,
            ..Default::default()
        });
        let conn = block_on(p.connect([0u8; 32])).unwrap();
        // Closed ends at once (None), so disconnect paths fail fast instead of
        // parking like Silent.
        assert!(block_on(conn.responses().next()).is_none());
    }

    #[test]
    fn chain_connect_error() {
        let p = MockPlatform::with_config(MockConfig {
            chain: ChainBehavior::ConnectError("offline".into()),
            ..Default::default()
        });
        assert!(block_on(p.connect([0u8; 32])).is_err());
    }

    #[test]
    fn auth_states_record_in_order() {
        let p = MockPlatform::new();
        p.auth_state_changed(AuthState::Disconnected);
        p.auth_state_changed(AuthState::Pairing {
            deeplink: "dl".into(),
        });
        assert_eq!(p.auth_states().len(), 2);
    }

    #[test]
    fn clone_shares_recordings() {
        let p = MockPlatform::new();
        let clone = p.clone();
        block_on(clone.navigate_to("z".into())).unwrap();
        assert_eq!(p.navigations(), vec!["z".to_string()]);
    }

    fn allocation_review(product: &str) -> UserConfirmationReview {
        UserConfirmationReview::ResourceAllocation(crate::ResourceAllocationReview {
            calling_product_id: product.to_string(),
            resources: vec![],
        })
    }

    fn device_request(p: &MockPlatform, request: latest::HostDevicePermissionRequest) -> bool {
        block_on(p.device_permission(&mock_product(), request)).expect("device permission answers")
            == PermissionDecision::AllowAlways
    }

    fn remote_request(p: &MockPlatform, permission: latest::RemotePermission) -> bool {
        block_on(p.remote_permission(
            &mock_product(),
            latest::RemotePermissionRequest { permission },
        ))
        .expect("remote permission answers")
            == PermissionDecision::AllowAlways
    }

    #[test]
    fn reviews_carry_the_payload_not_just_the_kind() {
        // Two reviews of the SAME kind with different payloads: a kind-only
        // recording cannot tell these apart, which is the whole reason the
        // payload log exists.
        let p = MockPlatform::new();
        block_on(p.confirm_user_action(allocation_review("first.dot"))).unwrap();
        block_on(p.confirm_user_action(allocation_review("second.dot"))).unwrap();

        assert_eq!(
            p.confirmations(),
            vec![ConfirmKind::ResourceAllocation; 2],
            "the kind view should see two identical kinds",
        );
        let products: Vec<String> = p
            .reviews()
            .into_iter()
            .map(|review| match review {
                UserConfirmationReview::ResourceAllocation(inner) => inner.calling_product_id,
                other => panic!("unexpected review {other:?}"),
            })
            .collect();
        assert_eq!(products, vec!["first.dot", "second.dot"]);
    }

    #[test]
    fn an_explicit_grant_overrides_a_deny_all_policy() {
        let p = MockPlatform::with_config(MockConfig {
            device_permissions: PermissionPolicy::DenyAll,
            ..MockConfig::default()
        });
        assert!(!device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));
        p.grant_permission("Camera");
        assert!(device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));
        // The grant is per permission, not a policy flip.
        assert!(!device_request(
            &p,
            latest::HostDevicePermissionRequest::Microphone
        ));
        assert_eq!(p.granted_permissions(), vec!["Camera".to_string()]);
    }

    #[test]
    fn an_explicit_revoke_overrides_an_allow_all_policy() {
        let p = MockPlatform::new();
        assert!(remote_request(&p, latest::RemotePermission::ChainSubmit));
        p.revoke_permission("ChainSubmit");
        assert!(!remote_request(&p, latest::RemotePermission::ChainSubmit));
        assert!(p.granted_permissions().is_empty());
        // Dropping the override restores the policy rather than leaving a denial.
        p.reset_permission("ChainSubmit");
        assert!(remote_request(&p, latest::RemotePermission::ChainSubmit));
    }

    #[test]
    fn enforcing_denies_whatever_was_not_explicitly_granted() {
        let p = MockPlatform::new();
        p.set_enforce_permissions(true);
        assert!(
            !device_request(&p, latest::HostDevicePermissionRequest::Camera),
            "enforcing must not fall back to the allow-all policy",
        );
        p.grant_permission("Camera");
        assert!(device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));
        assert!(!device_request(
            &p,
            latest::HostDevicePermissionRequest::Location
        ));
    }

    #[test]
    fn the_permission_log_records_surface_key_and_answer() {
        // The keys are spelled out rather than derived from the permission,
        // because a test that builds them the same way the mock does would
        // agree with any key space the mock happened to pick.
        let p = MockPlatform::new();
        p.revoke_permission("Camera");
        device_request(&p, latest::HostDevicePermissionRequest::Camera);
        remote_request(&p, latest::RemotePermission::ChainSubmit);

        assert_eq!(
            p.permission_log(),
            vec![
                PermissionLogEntry {
                    tag: "Camera".to_string(),
                    approved: false,
                    kind: PermissionKind::Device,
                },
                PermissionLogEntry {
                    tag: "ChainSubmit".to_string(),
                    approved: true,
                    kind: PermissionKind::Remote,
                },
            ]
        );
    }

    #[test]
    fn permissions_are_keyed_by_variant_tag_not_display_prose() {
        // The TypeScript mock keys on the SCALE tag, so a suite written once
        // against either mock has to reach the same decision here. `Display`
        // renders "camera" for a person to read, and keying on that would make
        // the grant below silently miss.
        let p = MockPlatform::new();
        p.revoke_permission("Camera");
        assert!(!device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));

        p.grant_permission("Camera");
        assert!(device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));

        // A grant that keyed on `Display` would read "access to a.example",
        // so it could never be issued before knowing the domains asked for,
        // and would not cover a second request naming different ones.
        p.revoke_permission("Remote");
        assert!(!remote_request(
            &p,
            latest::RemotePermission::Remote {
                domains: vec!["a.example".to_string()],
            }
        ));
        assert!(!remote_request(
            &p,
            latest::RemotePermission::Remote {
                domains: vec!["b.example".to_string(), "c.example".to_string()],
            }
        ));
    }

    fn mock_product() -> ProductContext {
        ProductContext::new("mock.dot".to_string()).expect("product context is valid")
    }

    fn create_room(p: &MockPlatform, room_id: &str) -> latest::ChatRoomRegistrationStatus {
        block_on(p.create_chat_room(
            &mock_product(),
            latest::HostChatCreateRoomRequest {
                room_id: room_id.to_string(),
                name: format!("{room_id} room"),
                icon: "https://example.invalid/i.png".to_string(),
            },
        ))
        .expect("room registration succeeds")
        .status
    }

    fn post_text(
        p: &MockPlatform,
        room_id: &str,
        body: &str,
    ) -> Result<latest::HostChatPostMessageResponse, latest::HostChatPostMessageError> {
        block_on(p.post_chat_message(
            &mock_product(),
            latest::HostChatPostMessageRequest {
                room_id: room_id.to_string(),
                payload: latest::ChatMessageContent::Text {
                    text: body.to_string(),
                },
            },
        ))
    }

    #[test]
    fn registering_a_room_twice_reports_the_second_as_existing() {
        let p = MockPlatform::new();
        assert_eq!(
            create_room(&p, "lobby"),
            latest::ChatRoomRegistrationStatus::New
        );
        assert_eq!(
            create_room(&p, "lobby"),
            latest::ChatRoomRegistrationStatus::Exists
        );
        // The repeat must not duplicate the room.
        assert_eq!(p.chat_rooms().len(), 1);
        assert_eq!(p.chat_rooms()[0].room_id, "lobby");
    }

    #[test]
    fn posting_records_the_message_against_the_id_the_product_was_given() {
        let p = MockPlatform::new();
        create_room(&p, "lobby");
        let first = post_text(&p, "lobby", "hello").expect("post succeeds");
        let second = post_text(&p, "lobby", "again").expect("post succeeds");
        assert_ne!(first.message_id, second.message_id);

        let posted = p.posted_chat_messages();
        assert_eq!(posted.len(), 2);
        assert_eq!(posted[0].message_id, first.message_id);
        assert_eq!(
            posted[0].payload,
            latest::ChatMessageContent::Text {
                text: "hello".to_string()
            }
        );
        assert_eq!(posted[1].message_id, second.message_id);
    }

    #[test]
    fn posting_to_an_unregistered_room_is_an_error() {
        // A mock that accepted this would hide a product bug rather than
        // surface it.
        let p = MockPlatform::new();
        let err = post_text(&p, "never-created", "hi").expect_err("unknown room is rejected");
        assert!(
            matches!(err, latest::HostChatPostMessageError::Unknown { reason } if reason.contains("never-created")),
        );
        assert!(p.posted_chat_messages().is_empty());
    }

    #[test]
    fn an_injected_chat_error_refuses_every_chat_entry_point() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                chat_error: Some("chat down".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        let product = mock_product();

        let room = block_on(p.create_chat_room(
            &product,
            latest::HostChatCreateRoomRequest {
                room_id: "lobby".to_string(),
                name: "lobby room".to_string(),
                icon: "https://example.invalid/i.png".to_string(),
            },
        ))
        .expect_err("room creation carries the injected reason");
        assert!(
            matches!(room, latest::HostChatCreateRoomError::Unknown { reason } if reason == "chat down"),
        );

        let bot = block_on(p.register_chat_bot(
            &product,
            latest::HostChatRegisterBotRequest {
                bot_id: "greeter".to_string(),
                name: "Greeter".to_string(),
                icon: "https://example.invalid/i.png".to_string(),
            },
        ))
        .expect_err("bot registration carries the injected reason");
        assert!(
            matches!(bot, latest::HostChatRegisterBotError::Unknown { reason } if reason == "chat down"),
        );

        // Posting reports the injected reason rather than the unknown-room
        // rejection it would otherwise hit, so the guard is what refused it.
        let message =
            post_text(&p, "lobby", "hi").expect_err("posting carries the injected reason");
        assert!(
            matches!(message, latest::HostChatPostMessageError::Unknown { reason } if reason == "chat down"),
        );

        // The subscription is an entry point too. Handing back a stream that
        // looks healthy and simply never carries a room is the one answer a
        // failing host must not give, because it is indistinguishable from a
        // host with nothing to list.
        let mut subscription = p.subscribe_chat_rooms(&product);
        let subscribed = subscription
            .next()
            .now_or_never()
            .expect("the refusal is ready immediately")
            .expect("an item")
            .expect_err("the subscription carries the injected reason");
        assert_eq!(subscribed.reason, "chat down");
        // And ends there. A stream that kept the consumer waiting after
        // refusing is the stalled stream this guard exists to avoid.
        assert!(
            subscription
                .next()
                .now_or_never()
                .expect("the end is ready immediately")
                .is_none()
        );

        assert!(p.chat_rooms().is_empty());
        assert!(p.posted_chat_messages().is_empty());
    }

    #[test]
    fn a_room_subscriber_sees_the_current_list_and_later_replacements() {
        let p = MockPlatform::new();
        create_room(&p, "first");
        let mut rooms = p.subscribe_chat_rooms(&mock_product());

        // Both items are already queued on an unbounded channel by the time
        // they are asserted, so take them without blocking: a subscription
        // that never delivers must fail this test rather than hang it.
        let seeded = rooms
            .next()
            .now_or_never()
            .expect("the seeded item is ready immediately")
            .expect("a seeded item")
            .expect("chat rooms stream is infallible here");
        assert_eq!(seeded.rooms.len(), 1);

        // A later registration is delivered as a replacement list, which is
        // what the trait's "and later replacements" contract requires.
        create_room(&p, "second");
        let replacement = rooms
            .next()
            .now_or_never()
            .expect("registering a room must deliver a replacement list")
            .expect("a replacement item")
            .expect("chat rooms stream is infallible here");
        let ids: Vec<String> = replacement
            .rooms
            .into_iter()
            .map(|room| room.room_id)
            .collect();
        assert_eq!(ids, vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn clearing_chat_state_empties_rooms_bots_and_messages() {
        let p = MockPlatform::new();
        create_room(&p, "lobby");
        post_text(&p, "lobby", "hi").expect("post succeeds");
        block_on(p.register_chat_bot(
            &mock_product(),
            latest::HostChatRegisterBotRequest {
                bot_id: "helper".to_string(),
                name: "Helper".to_string(),
                icon: "https://example.invalid/b.png".to_string(),
            },
        ))
        .expect("bot registration succeeds");
        assert_eq!(p.chat_bots().len(), 1);

        p.clear_chat_state();

        assert!(p.chat_rooms().is_empty());
        assert!(p.chat_bots().is_empty());
        assert!(p.posted_chat_messages().is_empty());
    }

    #[test]
    fn a_theme_subscriber_sees_the_current_theme_and_later_changes() {
        let p = MockPlatform::new();
        let mut themes = p.subscribe_theme();

        let seeded = themes
            .next()
            .now_or_never()
            .expect("the seeded theme is ready immediately")
            .expect("a seeded item")
            .expect("theme stream is infallible here");
        assert_eq!(seeded.variant, latest::ThemeVariant::Dark);

        p.set_theme(latest::ThemeVariant::Light);
        let changed = themes
            .next()
            .now_or_never()
            .expect("set_theme must deliver to live subscribers")
            .expect("a changed item")
            .expect("theme stream is infallible here");
        assert_eq!(changed.variant, latest::ThemeVariant::Light);
        assert_eq!(p.theme(), latest::ThemeVariant::Light);
    }

    #[test]
    fn a_simulated_disconnect_ends_the_response_stream_and_blocks_reconnect() {
        let p = MockPlatform::with_config(MockConfig {
            // Silent responses stay pending forever, so if the disconnect is
            // not observed this assertion fails rather than passing by luck.
            chain: ChainBehavior::Silent,
            ..MockConfig::default()
        });
        let connection = block_on(p.connect([0; 32])).expect("first connect succeeds");
        assert_eq!(p.chain_status(), ChainStatus::Connected);
        let mut responses = connection.responses();
        assert!(
            responses.next().now_or_never().is_none(),
            "a silent connection has nothing to deliver yet",
        );

        p.simulate_disconnect();

        assert_eq!(p.chain_status(), ChainStatus::Disconnected);
        assert!(
            matches!(responses.next().now_or_never(), Some(None)),
            "the disconnect must end the stream, not leave it pending",
        );
        assert!(
            block_on(p.connect([0; 32])).is_err(),
            "a disconnected chain must refuse new connections",
        );

        p.simulate_reconnect();
        assert!(block_on(p.connect([0; 32])).is_ok());
    }

    #[test]
    fn seeded_preimages_are_readable_in_key_order() {
        let p = MockPlatform::new();
        let first = p.insert_preimage(vec![1, 2, 3]);
        let second = p.insert_preimage(vec![4, 5, 6]);
        let mut expected = vec![(first, vec![1, 2, 3]), (second, vec![4, 5, 6])];
        expected.sort_by(|(left, _), (right, _)| left.cmp(right));
        assert_eq!(p.preimages(), expected);

        p.clear_preimages();
        assert!(p.preimages().is_empty());
    }

    #[test]
    fn an_injected_confirmation_fault_is_not_a_refusal() {
        // A host that could not ask is not a user who said no, and the mock
        // has to be able to express the difference.
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                confirmation_error: Some("no UI available".to_string()),
                ..MockFaults::default()
            },
            ..MockConfig::default()
        });
        let err = block_on(p.confirm_user_action(allocation_review("mock.dot")))
            .expect_err("the confirmation fails");
        assert_eq!(err.reason, "no UI available");
        // It still records what was asked, so a test can assert the prompt fired.
        assert_eq!(p.confirmations(), vec![ConfirmKind::ResourceAllocation]);
    }

    #[test]
    fn injected_permission_and_feature_faults_surface_as_errors() {
        let p = MockPlatform::with_config(MockConfig {
            faults: MockFaults {
                permission_error: Some("permission backend down".to_string()),
                feature_error: Some("feature backend down".to_string()),
                ..MockFaults::default()
            },
            ..MockConfig::default()
        });
        assert!(
            block_on(
                p.device_permission(&mock_product(), latest::HostDevicePermissionRequest::Camera)
            )
            .is_err()
        );
        assert!(
            block_on(p.remote_permission(
                &mock_product(),
                latest::RemotePermissionRequest {
                    permission: latest::RemotePermission::ChainSubmit,
                }
            ))
            .is_err()
        );
        assert!(block_on(p.supported_chains()).is_err());
        // A failed prompt is not a recorded decision.
        assert!(p.permission_log().is_empty());
    }

    #[test]
    fn declared_supported_chains_are_what_the_mock_reports() {
        // The default set is spelled out rather than compared against
        // `mock_genesis::chain_set()`, because the TypeScript mock declares
        // these same three and a suite written against either has to be
        // answered the same way.
        let p = MockPlatform::new();
        let default_set = block_on(p.supported_chains()).expect("default set");
        assert_eq!(default_set.network, "mock");
        assert_eq!(
            default_set
                .chains
                .iter()
                .map(|entry| (entry.identifier, entry.genesis_hash))
                .collect::<Vec<_>>(),
            vec![
                (latest::ChainIdentifier::People, [0x11; 32]),
                (latest::ChainIdentifier::Bulletin, [0x22; 32]),
                (latest::ChainIdentifier::AssetHub, [0x33; 32]),
            ]
        );

        let p = MockPlatform::with_config(MockConfig {
            supported_chains: crate::HostChainSet {
                network: "paseo".to_string(),
                chains: vec![crate::HostChainEntry {
                    identifier: latest::ChainIdentifier::AssetHub,
                    genesis_hash: [0xaa; 32],
                }],
            },
            ..MockConfig::default()
        });
        let set = block_on(p.supported_chains()).expect("declared set");
        assert_eq!(set.network, "paseo");
        assert_eq!(set.chains.len(), 1);
        assert_eq!(set.chains[0].genesis_hash, [0xaa; 32]);
    }

    #[test]
    fn reset_clears_recordings_and_restores_policy_fallback() {
        let p = MockPlatform::new();
        block_on(p.navigate_to("u".into())).unwrap();
        block_on(p.confirm_user_action(allocation_review("mock.dot"))).unwrap();
        block_on(p.write("k".into(), vec![1])).unwrap();
        p.insert_preimage(vec![9]);
        p.auth_state_changed(AuthState::Disconnected);
        p.set_enforce_permissions(true);
        p.revoke_permission(latest::HostDevicePermissionRequest::Camera.to_string());
        device_request(&p, latest::HostDevicePermissionRequest::Camera);

        p.reset();

        assert!(p.navigations().is_empty());
        assert!(p.reviews().is_empty());
        assert!(p.confirmations().is_empty());
        assert!(p.auth_states().is_empty());
        assert!(p.permission_log().is_empty());
        assert!(p.granted_permissions().is_empty());
        assert_eq!(block_on(p.read("k".into())).unwrap(), None);
        // Enforcement and the explicit denial are both gone, so the allow-all
        // policy answers again.
        assert!(device_request(
            &p,
            latest::HostDevicePermissionRequest::Camera
        ));
    }
}
