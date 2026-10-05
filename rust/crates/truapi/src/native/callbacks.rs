use crate::platform::{
    AuthState, DevicePermissionStatus, PermissionDecision, UserConfirmationReview,
};
use truapi::v01;

use crate::PairedSsoPeer;

use super::config::ProductExecutionConfig;
use super::errors::HostRejection;
use crate::platform::SecretCoreStorageKey;
#[cfg(doc)]
use super::{NativeTrUApiHostRuntime, NativeWorkerEngineHost};

/// Wallet-only access to the existing protected root store after native unlock.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeWalletSecretProvider: Send + Sync {
    /// Read the selected wallet without copying its persisted root store.
    async fn read_wallet_root_entropy(&self, wallet_id: String) -> Result<Vec<u8>, HostRejection>;
}

/// Callback surface that iOS and Android implement.
///
/// Threading contract: every callback executes on the shared bridge
/// executor's worker threads, and blocking one of those threads can stall
/// the entire bridge — not just the request being served. Async callbacks
/// (`navigate_to`, `push_notification`, `device_permission`,
/// `remote_permission`, `feature_supported`, `confirm_user_action`, `confirm_permission`,
/// `lookup_preimage`, and the core and local storage callbacks) are awaited by the core. Implementations hop to the
/// main thread for any UI and may keep the future pending arbitrarily long,
/// but must suspend rather than block the polling thread (foreign
/// implementations bridged through UniFFI suspend naturally; the rule
/// chiefly binds Rust implementations). Dropping the returned future
/// cancels the foreign task. The remaining sync callbacks run inline on the
/// dispatcher thread and must return promptly without blocking; in
/// particular `auth_state_changed` should only hand the state to the host
/// UI thread, never wait for the user. `chain_send` and `chain_close` are
/// sync so requests reach the connection in the order the core sent them;
/// they must only enqueue work on the host's connection, never wait on the
/// network.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait HostCallbacks: Send + Sync {
    /// Invalidate native projections after an account record commits.
    fn runtime_records_changed(&self);

    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError>;

    /// Register the Rust-owned notification id with the OS; success confirms registration.
    async fn schedule_notification(
        &self,
        product_id: String,
        id: u32,
        request: v01::HostPushNotificationRequest,
    ) -> Result<(), HostRejection>;

    /// Inspect OS registration without maintaining a competing native journal.
    async fn is_scheduled_notification_pending(&self, product_id: String, id: u32) -> Result<bool, HostRejection>;

    /// Confirm OS cancellation before Rust removes the owning record.
    async fn cancel_scheduled_notification(&self, product_id: String, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...)
    /// `product` requested; the host preserves whether approval applies once
    /// or always.
    async fn device_permission(
        &self,
        product: ProductExecutionConfig,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<PermissionDecision, HostRejection>;

    /// Report the OS status of a device capability without prompting.
    ///
    /// Answer from the platform's own authorization APIs. This must not show
    /// UI: the core calls it before every device-permission request and status
    /// read, and prompting here would re-ask a question the user has already
    /// answered. A host with no OS gate for the capability answers
    /// `NotApplicable`, which leaves the stored product decision governing.
    ///
    /// It is async because reading notification authorization on iOS is
    /// `UNUserNotificationCenter.getNotificationSettings(completionHandler:)`.
    async fn device_permission_status(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<DevicePermissionStatus, HostRejection>;

    /// Prompt the user for a remote permission `product` requested.
    async fn remote_permission(
        &self,
        product: ProductExecutionConfig,
        request: v01::RemotePermission,
    ) -> Result<PermissionDecision, HostRejection>;

    /// Observe an auth state change, in transition order: render `Pairing` as
    /// the pairing QR UI, `Connected`/`Disconnected` as the account badge,
    /// `LoginFailed` as a retryable error unless its `kind` is
    /// `NoFreeAllowanceSlots`, which is unlikely to succeed before the period
    /// rolls over, so retry should not be the primary action. A pairing host
    /// always receives an opening state once the core has restored the
    /// persisted session, `Disconnected` included; later emissions happen
    /// only when the state changes.
    fn auth_state_changed(&self, state: AuthState);

    /// Read protected host bytes; only `None` means that the key is absent.
    async fn read_secret_core_storage(&self, key: SecretCoreStorageKey) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Return success only after the protected write is committed.
    async fn write_secret_core_storage(&self, key: SecretCoreStorageKey, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Return success only after persistent removal has completed.
    async fn clear_secret_core_storage(&self, key: SecretCoreStorageKey) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm one user-reviewed core action.
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, HostRejection>;

    /// Preserve the lifetime of consent for identity and account disclosures.
    async fn confirm_permission(
        &self,
        review: UserConfirmationReview,
    ) -> Result<PermissionDecision, HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Current host theme, named variant included. The native shim emits this
    /// as the current item in its subscription stream.
    fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection>;

    /// Locale the host currently presents its interface in. The native shim
    /// emits this as the current item in its subscription stream.
    fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection>;

    /// Answer a feature-support query.
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<bool, HostRejection>;

    /// Enumerate the chains this host serves (RFC 0026): its environment plus
    /// one entry per chain role. Invoked on the dispatcher thread; must return
    /// promptly.
    fn supported_chains(&self) -> Result<crate::platform::HostChainSet, HostRejection>;

    /// A device finished pairing with this signing host.
    ///
    /// The core has no chat of its own, so announcing the new device to the
    /// user's existing contacts is the host's to do. At least once per
    /// pairing, and the host keeps its own record of which devices it has
    /// already seen: a resumed pairing reports nothing and the core has no
    /// list to replay.
    ///
    /// Arrives on the thread answering the handshake, while the pairing call
    /// is still running: hand the device off rather than announcing it
    /// inline.
    fn device_paired(&self, device: PairedSsoPeer);


}

/// Native Chat storage and UI adapter. Hosts that support the Chat modality
/// return an implementation from [`NativeWorkerEngineHost::chat_callbacks`];
/// hosts without Chat return `None`. Callbacks run on the process-wide dispatch pool shared by
/// every product execution.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeChatCallbacks: Send + Sync {
    /// Create or resolve a native product Chat room.
    async fn create_room(
        &self,
        room_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatRoomRegistrationStatus, HostRejection>;

    /// Register or resolve a native product Chat bot.
    async fn register_bot(
        &self,
        bot_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatBotRegistrationStatus, HostRejection>;

    /// Persist a product-authored message in native Chat storage. A host that
    /// cannot render a given content variant returns a rejection for it.
    ///
    /// The returned id is [`HostChatPostMessageResponse`]'s `message_id`, which
    /// chat actions carry back for as long as the host stores this message.
    ///
    /// [`HostChatPostMessageResponse`]: truapi::latest::HostChatPostMessageResponse
    async fn post_message(
        &self,
        room_id: String,
        content: v01::ChatMessageContent,
    ) -> Result<String, HostRejection>;

    /// Return the current product-scoped native Chat room list.
    async fn list_rooms(&self) -> Result<Vec<v01::ChatRoom>, HostRejection>;
}

/// Native Pocket collection adapter. Hosts with a Pocket surface return an
/// implementation from [`NativeWorkerEngineHost::pocket_callbacks`]; hosts
/// without one return `None`. `list_cards` runs inline on the core runtime
/// shared by every product execution, so it must return promptly;
/// `remove_card` is awaited and may suspend while the host's storage works.
///
/// The host decides a removal and reports what it did, so the check and the
/// removal happen together under whatever lock it holds. A card cannot be
/// pinned between the two.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativePocketCallbacks: Send + Sync {
    /// Return the product's cards as this host currently holds them, each with
    /// the flag saying whether the host pinned it.
    fn list_cards(&self) -> Result<Vec<v01::PocketCard>, HostRejection>;

    /// Remove one of the product's cards, reporting whether the card was
    /// taken out, was already gone, or is pinned and stays.
    async fn remove_card(&self, card_id: String) -> Result<NativePocketRemoval, HostRejection>;
}

/// What a host did with a removal request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativePocketRemoval {
    /// The card was present and not pinned, and the host took it out.
    Removed,
    /// The host does not hold the card, so it is already gone.
    Absent,
    /// The host pins the card and keeps it.
    Privileged,
}

/// Native contacts adapter. A host with a contact list and a picker passes an
/// implementation to [`NativeTrUApiHostRuntime::set_contacts_callbacks`]; one
/// without leaves `contacts.pick` answering `Unsupported`.
///
/// The lookup runs inline, because it is a store read the host already has in
/// hand. Presenting the picker is async, because it is the user.
///
/// Nothing here reaches a product, and the list never reaches the core: it asks
/// only about the handles a transaction names, and the picker returns the one
/// person the user chose.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeContactsCallbacks: Send + Sync {
    /// Resolve `lookup.handles` to the contacts they name: one entry per
    /// handle, in order, `None` where no current, unblocked contact hashes to
    /// it. See [`crate::platform::HostContactLookup`] for the hash.
    fn contacts(
        &self,
        lookup: crate::platform::HostContactLookup,
    ) -> Result<crate::platform::HostContactMatches, HostRejection>;

    /// Present the picker on behalf of `product_id` and report what the user
    /// did. A host with no contacts answers `NoContacts`
    /// instead of drawing an empty overlay.
    async fn pick_contact(
        &self,
        product_id: String,
    ) -> Result<crate::platform::HostContactPick, HostRejection>;
}
