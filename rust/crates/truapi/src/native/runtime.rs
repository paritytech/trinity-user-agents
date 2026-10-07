use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::platform::{
    CoreAdmin, PermissionAuthorizationRequest, PermissionAuthorizationStatus, ProductContext,
    ProductExecutionKind,
};
use parity_scale_codec::Encode;
use truapi::{Bytes32, v01};

use super::reject_undecodable_deeplink;
use super::renderer::observe_renderer;
use super::renderer::{NativeRendererObserver, NativeRendererSubscription};
use super::ws_bridge::{BridgeLogger, SharedWsBridge, WsBridgeEndpoint, WsBridgeStartError};
use crate::host_internal::permissions::TemporaryPermissions;
use crate::host_internal::sso_messages::SsoRequestOutcome;
use crate::host_internal::sso_messages::{
    RemoteMessage, RemoteMessageData, decode_remote_message, v1,
};
use crate::runtime::AnnouncedPairing;
use crate::runtime::sso_remote::sso_message_id;
use crate::store::{Db, core_db_config};
use crate::subscription::Spawner;
use crate::{PairedSsoPeer, ResponderExit, SigningHostRuntime};

use super::callbacks::{
    HostCallbacks, NativeChatCallbacks, NativeContactsCallbacks, NativeGameCallbacks,
    NativePocketCallbacks,
};
use super::config::{
    HostRuntimeConfig, NativeResolvedHostRuntimeConfig, NativeRuntimeConfigError,
    ProductExecutionConfig,
};
use super::errors::{HostRejection, NativeCoreDatabaseError};
use super::executor::shared_native_executor;
use super::events::NativeEventBus;
use super::platform::{
    CallbackPlatform, ChatCallbackPlatform, ContactsCallbackPlatform, GameCallbackPlatform,
    PocketCallbackPlatform,
};
#[cfg(doc)]
use crate::WorkerTransition;
#[cfg(doc)]
use super::parse_pairing_deeplink;

/// Process-owned native TrUAPI runtime shared by all executable connections.
#[derive(uniffi::Object)]
pub struct NativeTrUApiHostRuntime {
    runtime: Arc<SigningHostRuntime>,
    events: Arc<NativeEventBus>,
    spawner: Spawner,
    ws_bridge: Arc<SharedWsBridge>,
    /// The one Worker execution per product; opening another replaces it.
    worker_executions: Mutex<HashMap<String, Weak<NativeProductExecution>>>,
}

impl NativeTrUApiHostRuntime {
    fn from_resolved(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: NativeResolvedHostRuntimeConfig,
        log_marker: &str,
        log_detail: &str,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        crate::logging::init();
        callbacks.on_core_log(log_marker.to_string(), log_detail.to_string());
        let executor = shared_native_executor().map_err(|err| {
            NativeRuntimeConfigError::RuntimeUnavailable {
                reason: err.to_string(),
            }
        })?;
        let directory = &runtime_config.database_directory;
        let core_db = futures::executor::block_on(Db::open(core_db_config(directory))).map_err(
            |err| NativeRuntimeConfigError::DatabaseUnavailable {
                reason: format!("{}: {err}", directory.display()),
            },
        )?;
        let events = Arc::new(NativeEventBus::default());
        let platform = Arc::new(CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
            storage_events: events.clone(),
        });
        let spawner = executor.spawner();
        let runtime = Arc::new(SigningHostRuntime::new(
            platform.clone(),
            runtime_config.signing,
            spawner.clone(),
        ));
        assert!(
            runtime
                .worker_ledger()
                .install_demand_observer(platform.clone()),
            "a freshly built runtime installs its worker demand observer once"
        );
        assert!(
            runtime.set_device_pairing_observer(platform),
            "a freshly built runtime installs its device pairing observer once"
        );
        assert!(
            runtime.set_core_db(core_db),
            "a freshly built runtime installs its core database once"
        );
        if let Some(secret) = runtime_config.local_session_secret {
            futures::executor::block_on(runtime.activate_local_session_with_identity(
                secret,
                runtime_config.local_session_lite_username,
            ))
            .map_err(|err| NativeRuntimeConfigError::LocalSessionActivation {
                reason: err.reason,
            })?;
        }
        Ok(Arc::new(Self {
            runtime,
            events,
            spawner,
            ws_bridge: Arc::new(SharedWsBridge::new(Arc::new(move |marker, detail| {
                callbacks.on_core_log(marker.to_string(), detail.to_string());
            }))),
            worker_executions: Mutex::new(HashMap::new()),
        }))
    }

    fn open_product_execution_with_callbacks(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
        pocket_callbacks: Option<Arc<dyn NativePocketCallbacks>>,
        game_callbacks: Option<Arc<dyn NativeGameCallbacks>>,
        product: ProductContext,
    ) -> Arc<NativeProductExecution> {
        let events = Arc::new(NativeEventBus::default());
        let callback_platform = Arc::new(CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
            storage_events: self.events.clone(),
        });
        let permission_status: Arc<dyn crate::platform::PermissionStatusHost> =
            callback_platform.clone();
        let platform: Arc<dyn crate::platform::Platform> = callback_platform;
        let chat: Option<Arc<dyn crate::platform::ChatPlatform>> =
            chat_callbacks.map(|chat| -> Arc<dyn crate::platform::ChatPlatform> {
                Arc::new(ChatCallbackPlatform {
                    chat,
                    events: events.clone(),
                })
            });
        let pocket: Option<Arc<dyn crate::platform::PocketPlatform>> =
            pocket_callbacks.map(|pocket| -> Arc<dyn crate::platform::PocketPlatform> {
                Arc::new(PocketCallbackPlatform {
                    pocket,
                    events: events.clone(),
                })
            });
        let game: Option<Arc<dyn crate::platform::GamePlatform>> =
            game_callbacks.map(|game| -> Arc<dyn crate::platform::GamePlatform> {
                Arc::new(GameCallbackPlatform { game })
            });
        let execution = Arc::new(NativeProductExecution {
            runtime: self.runtime.clone(),
            product: product.clone(),
            platform,
            chat,
            pocket,
            game,
            permission_status,
            permission_grants: Arc::new(TemporaryPermissions::default()),
            events,
            shared_events: self.events.clone(),
            spawner: self.spawner.clone(),
            callbacks,
            closed: AtomicBool::new(false),
            chat_connection: Arc::new(crate::runtime::ActionChannel::chat()),
            renderer_connection: Arc::new(crate::runtime::ActionChannel::renderer()),
            ws_bridge: self.ws_bridge.clone(),
            bridge_token: Mutex::new(None),
            product_control: Arc::new(Mutex::new(None)),
        });

        if product.execution_kind == ProductExecutionKind::Worker {
            let previous = self
                .worker_executions
                .lock()
                .expect("native worker execution registry mutex poisoned")
                .insert(product.product_id, Arc::downgrade(&execution))
                .and_then(|previous| previous.upgrade());
            if let Some(previous) = previous {
                previous.shutdown();
            }
        }

        execution
    }
}

/// A refused pairing call on the signing host's responder side.
///
/// The two variants differ in what the host still owes the peer, which is why
/// they are told apart here rather than by reading `reason`.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativePairingError {
    /// The deeplink carries no pairing proposal.
    ///
    /// Purely local: nothing was announced, no renewal target was tracked and
    /// no peer is waiting, so there is nothing to undo and nobody to notify.
    #[error("{reason}")]
    UndecodableDeeplink {
        /// Human-readable rejection reason.
        reason: String,
    },
    /// The core refused a call it had already decoded the peer for.
    ///
    /// A peer that was announced is still waiting on a
    /// [`NativeTrUApiHostRuntime::notify_pairing_failed`], and a renewal
    /// target tracked for it is still tracked.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl From<v01::GenericError> for NativePairingError {
    fn from(error: v01::GenericError) -> Self {
        Self::Rejected {
            reason: error.reason,
        }
    }
}

/// Handle to a pairing this host has told the peer it is working on, taken
/// back by [`NativeTrUApiHostRuntime::notify_pairing_failed`].
///
/// Opaque on purpose. The value carries the responder statement secret the
/// first notice was signed with, so that the second is signed by the same
/// account even if this host's signer rotates in between. Exposing it as a
/// record would put that secret on the FFI surface for no caller that needs
/// to read it.
///
/// Nothing consumes the handle, so it holds that secret for as long as the
/// host keeps a reference: release it once the pairing settles, on the
/// succeeding path as well as the failing one.
#[derive(uniffi::Object)]
pub struct NativeAnnouncedPairing {
    inner: AnnouncedPairing,
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
    /// Construct one host-level runtime and optionally activate its local session.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: HostRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        let runtime_config: NativeResolvedHostRuntimeConfig = runtime_config.try_into()?;
        Self::from_resolved(
            callbacks,
            runtime_config,
            "truapi.native.host_runtime.boot",
            "host runtime ready",
        )
    }

    /// Install the host's contacts adapter, which owns the contact list and
    /// draws the picker.
    ///
    /// Set-once, so the picker cannot change hands under a running product.
    /// Answers whether this call installed it. Call it before opening any
    /// product execution; a runtime without one answers `contacts.pick` with
    /// `Unsupported`.
    pub fn set_contacts_callbacks(&self, callbacks: Arc<dyn NativeContactsCallbacks>) -> bool {
        self.runtime
            .set_contacts_platform(Arc::new(ContactsCallbackPlatform {
                contacts: callbacks,
            }))
    }

    /// Tell the core the host's contacts changed. Call it whenever a contact
    /// is removed or blocked, so a handle the core cached stops resolving.
    pub fn notify_contacts_changed(&self) {
        self.runtime.notify_contacts_changed();
    }

    /// Open a connection-scoped execution with immutable trusted context.
    /// `chat_callbacks` installs the host's Chat adapter; hosts without the
    /// Chat modality pass `None`. `pocket_callbacks` does the same for the
    /// card collection. `game_callbacks` does the same for game reminders.
    pub fn open_product_execution(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
        pocket_callbacks: Option<Arc<dyn NativePocketCallbacks>>,
        game_callbacks: Option<Arc<dyn NativeGameCallbacks>>,
        execution_config: ProductExecutionConfig,
    ) -> Result<Arc<NativeProductExecution>, NativeRuntimeConfigError> {
        let product: ProductContext = execution_config.try_into()?;
        Ok(self.open_product_execution_with_callbacks(
            callbacks,
            chat_callbacks,
            pocket_callbacks,
            game_callbacks,
            product,
        ))
    }

    /// Take one reference on the product's worker for a modality holder. The
    /// first one reports [`WorkerTransition::Start`] to the runtime's
    /// [`HostCallbacks::worker_demand_changed`]; pair every call with one
    /// [`Self::release_worker`].
    pub fn acquire_worker(&self, product_id: String) {
        self.runtime.worker_ledger().acquire(&product_id);
    }

    /// Release one reference. The last one reports
    /// [`WorkerTransition::Stop`], after which the host may stop the worker;
    /// releasing with none held is a no-op.
    pub fn release_worker(&self, product_id: String) {
        self.runtime.worker_ledger().release(&product_id);
    }

    /// Tell the pairing host behind `deeplink` that allowance allocation is
    /// under way, so it leaves its QR screen while the allocation runs.
    ///
    /// Answering at all needs this host's own statement-store allowance, so
    /// register the `WalletSso` renewal target first. The peer's own device
    /// statement account is the other target, read with
    /// [`parse_pairing_deeplink`] and tracked before
    /// [`Self::establish_pairing`] runs; the allocation this notice covers is
    /// what that call waits on. The returned handle is owed a
    /// [`Self::notify_pairing_failed`] if the pairing then fails: the peer has
    /// dropped its QR and waits without a deadline of its own. No handle comes
    /// back on [`NativePairingError::UndecodableDeeplink`], which is refused
    /// before anything reaches the peer.
    pub async fn notify_pairing_allowance_allocation(
        &self,
        deeplink: String,
    ) -> Result<Arc<NativeAnnouncedPairing>, NativePairingError> {
        reject_undecodable_deeplink(&deeplink)?;
        self.runtime
            .notify_pairing_allowance_allocation(&deeplink)
            .await
            .map(|inner| Arc::new(NativeAnnouncedPairing { inner }))
            .map_err(NativePairingError::from)
    }

    /// Tell a pairing host that already dropped its QR why pairing stopped.
    ///
    /// Takes the handle from [`Self::notify_pairing_allowance_allocation`], so
    /// the notice is signed by the account that already reached that peer even
    /// if this host's signer has rotated since.
    pub async fn notify_pairing_failed(
        &self,
        announced: Arc<NativeAnnouncedPairing>,
        reason: String,
    ) -> Result<(), NativePairingError> {
        self.runtime
            .notify_pairing_failed(&announced.inner, reason)
            .await
            .map_err(NativePairingError::from)
    }

    /// Answer a pairing host's handshake deeplink, without serving the session
    /// it opens.
    ///
    /// The answer is signed by this host's own SSO statement identity, so the
    /// `WalletSso` renewal target has to be allocated for it to reach the
    /// Statement Store at all. The peer's device statement account is the
    /// other tracked target, since this host allocates the allowance the peer
    /// authors its own session statements under; read it from the deeplink
    /// with [`parse_pairing_deeplink`]. A pairing that fails after that leaves
    /// the peer's target to untrack again, unless the device was already
    /// paired and the target still carries a live pairing. That is what
    /// [`NativePairingError::Rejected`] means here;
    /// [`NativePairingError::UndecodableDeeplink`] never reached the peer and
    /// leaves nothing tracked.
    ///
    /// A device that pairs here is reported to
    /// [`HostCallbacks::device_paired`]. Serving the session is
    /// [`Self::resume_pairing`], which the host calls with the peer it
    /// persisted.
    pub async fn establish_pairing(&self, deeplink: String) -> Result<(), NativePairingError> {
        reject_undecodable_deeplink(&deeplink)?;
        self.runtime
            .establish_pairing(&deeplink)
            .await
            .map_err(NativePairingError::from)
    }

    /// Serve a paired host's SSO session until it ends.
    ///
    /// Runs for the life of the session, so call it off the host's main
    /// thread. Only [`ResponderExit::PeerDisconnected`] authorizes dropping
    /// the stored pairing; after [`ResponderExit::SubscriptionEnded`] or an
    /// error the peer is still paired and the call can be made again.
    pub async fn resume_pairing(
        &self,
        peer: PairedSsoPeer,
    ) -> Result<ResponderExit, NativePairingError> {
        self.runtime
            .resume_pairing(peer)
            .await
            .map_err(NativePairingError::from)
    }

    /// Tell a paired host this signing host is ending their SSO session.
    ///
    /// Submits the disconnect notice and nothing else. The local side is the
    /// caller's: cancel that peer's [`Self::resume_pairing`] task, which
    /// otherwise keeps answering a host this one no longer considers paired,
    /// and untrack its device statement account, which otherwise keeps being
    /// renewed every period. Dropping the stored pairing alone leaves both
    /// running.
    pub async fn disconnect_paired_host(
        &self,
        peer: PairedSsoPeer,
    ) -> Result<(), NativePairingError> {
        self.runtime
            .disconnect_paired_host(peer)
            .await
            .map_err(NativePairingError::from)
    }

    /// Core-owned logout for the process-wide authentication session.
    pub fn disconnect(&self) {
        futures::executor::block_on(self.runtime.disconnect_session());
    }

    /// Record the accounts a renewal pass should keep allowed. The ledger
    /// persists, so this only has to be called when the set changes, not on
    /// every launch. Renewal has nothing to do until at least one target is
    /// tracked.
    pub fn track_statement_renewal_targets(
        &self,
        targets: Vec<crate::runtime::StatementRenewalTarget>,
    ) -> Result<(), HostRejection> {
        futures::executor::block_on(self.runtime.track_statement_renewal_targets(targets))
            .map_err(HostRejection::from)
    }

    /// Every account the ledger tracks, in the order it was tracked.
    ///
    /// Needs no active session. Slots per period are finite, so a host that
    /// tracked a wrong or stale account can see it here and drop it with
    /// [`Self::untrack_statement_renewal_account`].
    pub fn statement_renewal_targets(
        &self,
    ) -> Result<Vec<crate::runtime::TrackedStatementRenewalTarget>, HostRejection> {
        futures::executor::block_on(self.runtime.statement_renewal_targets())
            .map_err(HostRejection::from)
    }

    /// Root public key the active identity records its fixed entries under.
    ///
    /// Needs an active session, and fails with `Disconnected` without one.
    /// An entry from [`Self::statement_renewal_targets`] whose owner is this
    /// key, or which has no owner at all, is one a pass will renew; any other
    /// is one a pass will prune.
    pub fn statement_renewal_owner_key(&self) -> Result<Bytes32, HostRejection> {
        self.runtime
            .statement_renewal_owner_key()
            .map_err(HostRejection::from)
    }

    /// Stop renewing one fixed statement account, returning whether the ledger
    /// held it.
    ///
    /// Scoped to the active identity, so it never removes an entry another
    /// identity promised. Needs an active session to resolve that identity.
    pub fn untrack_statement_renewal_account(
        &self,
        account_id: Bytes32,
    ) -> Result<bool, HostRejection> {
        futures::executor::block_on(self.runtime.untrack_statement_renewal_account(&account_id))
            .map_err(HostRejection::from)
    }

    /// Run one renewal pass now and report what each tracked target got.
    ///
    /// This is the entry point for hosts whose process cannot stay alive
    /// between periods: drive it from WorkManager or BGTaskScheduler rather
    /// than [`Self::start_statement_allowance_renewal`]. It submits extrinsics
    /// and blocks until they are included, so call it from a background thread.
    ///
    /// Needs an active session, which is the whole difficulty of the scheduled
    /// case: an OS-woken cold start has none until the host restores one, and
    /// the pass then fails with the bare reason `Disconnected`. Restore the
    /// session before calling, and treat that reason as "not ready" rather than
    /// as a renewal failure. [`Self::start_statement_allowance_renewal`] does
    /// not need this care; its loop skips a tick with no session and retries.
    pub fn renew_statement_allowances(
        &self,
    ) -> Result<crate::statement_allowance::renewal::StatementRenewalReport, HostRejection> {
        futures::executor::block_on(self.runtime.renew_statement_allowances())
            .map_err(HostRejection::from)
    }

    /// Start the in-process renewal loop, for hosts that stay resident. Mobile
    /// hosts should schedule [`Self::renew_statement_allowances`] instead,
    /// because a suspended process stops ticking. Idempotent; the loop ends
    /// when this runtime is dropped.
    pub fn start_statement_allowance_renewal(&self) {
        self.runtime.start_statement_allowance_renewal();
    }

    /// The in-process loop's own cadence: at most an hour, tightening to land
    /// just after the next period boundary.
    ///
    /// The hourly cap is a retry rhythm, not a statement about when work is
    /// due. An allowance stays usable for `Resources.StmtStoreGraceWindow` past
    /// its boundary, 48 hours on `paseo-next-v2`, so a host scheduling one OS
    /// wake-up per period has ample slack and should treat any value under an
    /// hour as the boundary approaching, rather than requesting a wake every
    /// hour for a pass that will almost always report `AlreadyAllocated`.
    pub fn next_statement_renewal_delay(&self) -> std::time::Duration {
        self.runtime.next_statement_renewal_delay()
    }

    /// The most recent pass the in-process renewal loop ran.
    ///
    /// `None` until a pass has run, which is "not yet" rather than healthy.
    /// [`Self::start_statement_allowance_renewal`] has no return value, so a host
    /// driving the loop reads its result here: `slots_exhausted` on the last pass
    /// means a period filled up and an allowance went unrenewed, which is the one
    /// outcome retrying cannot fix and a person may need telling about.
    pub fn last_statement_renewal_report(
        &self,
    ) -> Option<crate::statement_allowance::renewal::StatementRenewalReport> {
        self.runtime.last_statement_renewal_report()
    }

    /// Activate or replace the process-wide local signing session.
    pub fn activate_local_session(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), HostRejection> {
        futures::executor::block_on(
            self.runtime
                .activate_local_session_with_identity(secret, lite_username),
        )
        .map_err(Into::into)
    }

    /// Reports the core database's SQLite version, schema version and file
    /// path.
    pub async fn core_database_status(
        &self,
    ) -> Result<crate::store::DbStatus, NativeCoreDatabaseError> {
        self.runtime
            .core_database_status()
            .await
            .map_err(Into::into)
    }

    /// Answer one decrypted SSO remote message from a wallet-managed
    /// statement-store session.
    ///
    /// `message` is one SCALE-encoded `RemoteMessage` exactly as decrypted from
    /// the session statement. The bytes are deliberately opaque at this
    /// boundary: the wallet forwards wire encodings verbatim and never
    /// constructs them. Session control and transport stay with the wallet —
    /// `Disconnected` is reported, never handled here. Confirmation-gated
    /// requests await `confirm_user_action`, so this can take arbitrarily long.
    ///
    /// A `Cancel` withdraws the request it names and returns at once. It
    /// reaches a running request only if the wallet passes it on as it
    /// arrives; queued behind that request it arrives too late to stop it.
    /// The withdrawn request, and the `Cancel` itself, answer `Ignored`.
    pub async fn handle_sso_request(
        &self,
        message: Vec<u8>,
    ) -> Result<SsoRequestOutcome, HostRejection> {
        let message =
            decode_remote_message(&message).map_err(|reason| HostRejection::Rejected { reason })?;
        Ok(self.runtime.answer_sso_request(message).await)
    }

    /// Build the SCALE-encoded `Disconnected` message a wallet posts over a
    /// session it is ending. Each call carries a fresh opaque message id,
    /// like every outgoing SSO message; receivers detect disconnect by
    /// message variant, not id. Posting and record cleanup stay with the
    /// wallet.
    pub fn prepare_disconnect_request(&self) -> Vec<u8> {
        RemoteMessage {
            message_id: sso_message_id(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        }
        .encode()
    }

    /// Notify the shared chain adapter of one JSON-RPC response.
    pub fn notify_chain_response(&self, connection_id: u32, json: String) {
        self.events.notify_chain_response(connection_id, json);
    }

    /// Notify the shared chain adapter that a connection closed.
    pub fn notify_chain_closed(&self, connection_id: u32) {
        self.events.notify_chain_closed(connection_id);
    }
}

/// One native executable connection opened from a process-owned host runtime.
#[derive(uniffi::Object)]
pub struct NativeProductExecution {
    runtime: Arc<SigningHostRuntime>,
    product: ProductContext,
    platform: Arc<dyn crate::platform::Platform>,
    chat: Option<Arc<dyn crate::platform::ChatPlatform>>,
    pocket: Option<Arc<dyn crate::platform::PocketPlatform>>,
    game: Option<Arc<dyn crate::platform::GamePlatform>>,
    /// The same `CallbackPlatform` as `platform`, kept separately because
    /// `Arc<dyn Platform>` cannot be downcast to the optional capability.
    permission_status: Arc<dyn crate::platform::PermissionStatusHost>,
    /// One-use grants follow this execution across its product and admin connections.
    permission_grants: Arc<TemporaryPermissions>,
    events: Arc<NativeEventBus>,
    /// Host-runtime events back the process-wide services shared by every
    /// product execution (chain, Statement Store, and Bulletin). Native
    /// responses must reach this bus as well as the execution-scoped bus.
    shared_events: Arc<NativeEventBus>,
    spawner: Spawner,
    callbacks: Arc<dyn HostCallbacks>,
    /// Single Chat action buffer shared with every product connection this
    /// execution opens; survives bridge restarts until [`Self::shutdown`].
    chat_connection:
        Arc<crate::runtime::ActionChannel<truapi::versioned::chat::HostChatActionSubscribeItem>>,
    /// Single renderer action buffer shared with every product connection this
    /// execution opens; survives bridge restarts until [`Self::shutdown`].
    renderer_connection: Arc<
        crate::runtime::ActionChannel<truapi::versioned::renderer::HostRendererActionSubscribeItem>,
    >,
    closed: AtomicBool,
    ws_bridge: Arc<SharedWsBridge>,
    bridge_token: Mutex<Option<String>>,
    product_control: Arc<Mutex<Option<crate::ProductRuntimeControl>>>,
}

impl NativeProductExecution {
    fn adapters(&self) -> crate::host_core::ConnectionAdapters {
        crate::host_core::ConnectionAdapters {
            platform: self.platform.clone(),
            chat_platform: self.chat.clone(),
            permission_status: Some(self.permission_status.clone()),
            permission_grants: self.permission_grants.clone(),
            chat: self.chat_connection.clone(),
            renderer: self.renderer_connection.clone(),
            pocket_platform: self.pocket.clone(),
            game_platform: self.game.clone(),
        }
    }

    fn admin(&self) -> crate::HostAdmin {
        self.runtime
            .product_admin_with(self.product.clone(), self.adapters())
    }

    fn require_chat(&self) -> Result<(), crate::ProductRuntimeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(crate::ProductRuntimeError::Closed);
        }
        crate::runtime::chat_platform_for(
            self.product.execution_kind,
            self.runtime.has_active_session(),
            self.chat.as_ref(),
        )
        .map(drop)
    }

    fn require_renderer(&self) -> Result<(), crate::ProductRuntimeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(crate::ProductRuntimeError::Closed);
        }
        crate::runtime::renderer_access_for(self.product.execution_kind)
    }

    fn stop_bridge(&self) {
        // Release the token lock before waiting for connection cancellation.
        let token = self
            .bridge_token
            .lock()
            .expect("native product bridge mutex poisoned")
            .take();
        if let Some(token) = token {
            self.ws_bridge.revoke(&token);
        }
        *self
            .product_control
            .lock()
            .expect("native product control mutex poisoned") = None;
    }
}

#[uniffi::export]
impl NativeProductExecution {
    /// Authorize one native operation using this execution's saved and one-use permissions.
    pub async fn authorize_remote_permission(
        &self,
        request: truapi::latest::RemotePermissionRequest,
    ) -> Result<bool, HostRejection> {
        use truapi::api::Permissions;
        use truapi::versioned::IntoLatest;

        if self.closed.load(Ordering::Acquire) {
            return Err(HostRejection::Rejected {
                reason: "product execution is closed".to_string(),
            });
        }
        let response = self
            .admin()
            .product_runtime()
            .authorize_remote_permission(
                &truapi::CallContext::default(),
                truapi::versioned::permissions::RemotePermissionRequest::V1(request),
            )
            .await
            .map_err(|error| HostRejection::Rejected {
                reason: format!("{error:?}"),
            })?;
        if self.closed.load(Ordering::Acquire) {
            return Err(HostRejection::Rejected {
                reason: "product execution is closed".to_string(),
            });
        }
        Ok(response.into_latest().granted)
    }

    /// Read a product-scoped permission authorization without prompting.
    ///
    /// A device capability resolves the host application's OS gate as well as
    /// storage, which means calling `device_permission_status` on the host. It
    /// is async for that reason: blocking a thread on a host callback
    /// deadlocks any implementation that hops to the same thread to answer.
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, HostRejection> {
        Ok(self
            .admin()
            .permission_authorization_status(request)
            .await?)
    }

    /// Update a product-scoped permission authorization.
    pub fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), HostRejection> {
        futures::executor::block_on(
            self.admin()
                .set_permission_authorization_status(request, status),
        )?;
        Ok(())
    }

    /// Read the active session's X25519 chat identity private key, or `None`
    /// when no session is active.
    pub fn session_chat_identity_key(&self) -> Result<Option<Bytes32>, HostRejection> {
        Ok(futures::executor::block_on(
            self.admin().get_session_chat_identity_key(),
        )?)
    }

    /// Read this device's X25519 encryption secret, for device sync against a
    /// peer's `deviceEncPublicKey`. Generated and persisted on first read.
    pub fn device_encryption_key(&self) -> Result<Bytes32, HostRejection> {
        Ok(futures::executor::block_on(
            self.admin().get_device_encryption_key(),
        )?)
    }

    /// Resolve a product's hard-subtree public key for hosts naming the account
    /// a review will sign with. Answers from the cache, the persisted slot, or
    /// the Account Holder, and `timeout_ms` bounds that wait. Exceeding it is
    /// an error; `None` means no active session.
    pub fn product_subtree_public_key(
        &self,
        product_id: String,
        timeout_ms: Option<u32>,
    ) -> Result<Option<Bytes32>, HostRejection> {
        Ok(futures::executor::block_on(
            self.admin()
                .get_product_subtree_public_key(product_id, timeout_ms),
        )?)
    }

    /// Push a host theme replacement to this execution's subscriptions.
    pub fn notify_theme_changed(&self, theme: v01::HostThemeSubscribeItem) {
        self.events.notify_theme_changed(theme);
    }

    /// Push a host locale replacement to this execution's subscriptions.
    pub fn notify_locale_changed(&self, locale: v01::HostLocaleSubscribeItem) {
        self.events.notify_locale_changed(locale);
    }

    /// Push a preimage lookup replacement to this execution's subscriptions.
    pub fn notify_preimage_changed(&self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.events.notify_preimage_changed(&key, value);
    }

    /// Push a host storage change to the product's subscriptions for `key`.
    ///
    /// Storage is one namespace per product rather than per execution, so this
    /// reaches every execution of the product, not only this one.
    pub fn notify_storage_changed(&self, key: String, value: Option<Vec<u8>>) {
        self.shared_events.notify_storage_changed(&key, value);
    }

    /// Notify this execution's chain adapter of one JSON-RPC response.
    pub fn notify_chain_response(&self, connection_id: u32, json: String) {
        self.shared_events
            .notify_chain_response(connection_id, json.clone());
        self.events.notify_chain_response(connection_id, json);
    }

    /// Notify this execution's chain adapter that a connection closed.
    pub fn notify_chain_closed(&self, connection_id: u32) {
        self.shared_events.notify_chain_closed(connection_id);
        self.events.notify_chain_closed(connection_id);
    }

    /// Push a complete native Chat room-list replacement to this execution.
    pub fn notify_chat_rooms_changed(&self, rooms: Vec<v01::ChatRoom>) {
        self.events.notify_chat_rooms_changed(rooms);
    }

    /// Publish one native Chat action, buffering it until the product
    /// connection subscribes.
    pub fn publish_chat_action(
        &self,
        action: v01::HostChatActionSubscribeItem,
    ) -> Result<(), crate::ProductRuntimeError> {
        self.require_chat()?;
        self.chat_connection
            .publish(truapi::versioned::chat::HostChatActionSubscribeItem::V1(
                action,
            ))
    }

    /// Ask the product to draw one body, delivering each replacement tree to
    /// `observer` until the returned subscription is cancelled.
    pub fn render(
        &self,
        request: v01::ProductRendererRenderRequest,
        observer: Box<dyn NativeRendererObserver>,
    ) -> Result<Arc<NativeRendererSubscription>, crate::ProductRuntimeError> {
        self.require_renderer()?;
        let control = self
            .product_control
            .lock()
            .expect("native product control mutex poisoned")
            .clone()
            .ok_or(crate::ProductRuntimeError::NotConnected)?;
        let stream = control.render(request)?;
        let observer: Arc<dyn NativeRendererObserver> = observer.into();
        Ok(observe_renderer(stream, observer, self.spawner.clone()))
    }

    /// Publish one action triggered inside a product-rendered body, buffering
    /// it until the product connection subscribes.
    pub fn publish_renderer_action(
        &self,
        item: v01::HostRendererActionSubscribeItem,
    ) -> Result<(), crate::ProductRuntimeError> {
        self.require_renderer()?;
        self.renderer_connection
            .publish(truapi::versioned::renderer::HostRendererActionSubscribeItem::V1(item))
    }

    /// Push a complete native Pocket card-list replacement to this execution.
    pub fn notify_pocket_cards_changed(&self, cards: Vec<v01::PocketCard>) {
        self.events.notify_pocket_cards_changed(cards);
    }

    /// Permanently shut down this executable and all of its connection state.
    ///
    /// This is named `shutdown` rather than `close` because UniFFI Kotlin
    /// objects already implement `AutoCloseable.close()` for releasing the
    /// foreign object handle.
    pub fn shutdown(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.stop_bridge();
        self.permission_grants.clear();
        self.chat_connection.close();
        self.renderer_connection.close();
    }
}

#[uniffi::export]
impl NativeProductExecution {
    /// Register this execution with its own token on the host's shared listener.
    /// `bind_port` applies only when the listener first starts.
    pub fn start_ws_bridge(&self, bind_port: u16) -> Result<WsBridgeEndpoint, WsBridgeStartError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(WsBridgeStartError::Io(
                "product execution is closed".to_string(),
            ));
        }
        let mut guard = self
            .bridge_token
            .lock()
            .expect("native product bridge mutex poisoned");
        if guard.is_some() {
            return Err(WsBridgeStartError::AlreadyRunning);
        }
        let logger: BridgeLogger = {
            let callbacks = self.callbacks.clone();
            Arc::new(move |marker: &str, detail: &str| {
                callbacks.on_core_log(marker.to_string(), detail.to_string());
            })
        };
        let runtime = self.runtime.clone();
        let product = self.product.clone();
        let adapters = self.adapters();
        let product_control = self.product_control.clone();
        let runtime_factory = Arc::new(move |sink| {
            let product_runtime =
                runtime.product_runtime_with(product.clone(), adapters.clone(), sink);
            *product_control
                .lock()
                .expect("native product control mutex poisoned") = Some(product_runtime.control());
            product_runtime
        });
        let endpoint = self
            .ws_bridge
            .register(bind_port, runtime_factory, logger)?;
        *guard = Some(endpoint.token.clone());
        Ok(endpoint)
    }

    /// Revoke this execution's bridge registration while leaving it reusable.
    pub fn stop_ws_bridge(&self) {
        self.stop_bridge();
    }
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
    /// Rebind the shared bridge listener on its port, keeping every
    /// execution's endpoint valid. iOS reclaims a suspended app's listening
    /// sockets, so call this each time the app returns to the foreground.
    pub fn relisten_ws_bridge(&self) {
        self.ws_bridge.relisten();
    }
}

impl Drop for NativeProductExecution {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PairedSsoPeer;
    use crate::native::tests::*;
    use crate::platform::{
        PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionDecision,
        ProductExecutionKind,
    };
    use futures::stream::StreamExt;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use truapi::v01;

    #[test]
    fn a_worker_write_reaches_a_storage_subscription_in_the_products_other_execution() {
        let callbacks = Arc::new(EventCallbacks::new());
        let mut config = native_host_runtime_config();
        config.local_session_secret = None;
        config.local_session_lite_username = None;
        let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
            .expect("host runtime config should be valid");
        let screen = host
            .open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("myapp.dot", ProductExecutionKind::App),
            )
            .expect("open app execution");
        let worker = host
            .open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("myapp.dot", ProductExecutionKind::Worker),
            )
            .expect("open worker execution");

        let key = "myapp.dot/progress".to_string();
        let mut subscription = screen.platform.subscribe_storage(key.clone());
        assert_eq!(
            futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
            Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
            "a subscription opens on the key's current value"
        );

        futures::executor::block_on(worker.platform.write(key, vec![9])).expect("write");

        assert_eq!(
            futures::FutureExt::now_or_never(futures::StreamExt::next(&mut subscription)),
            Some(Some(Ok(v01::HostLocalStorageChangeItem {
                value: Some(vec![9]),
            }))),
            "the screen and the worker share one storage namespace, so a write in one \
             reaches a subscription in the other"
        );
    }

    #[test]
    fn a_host_pushed_storage_change_reaches_the_products_subscription() {
        let callbacks = Arc::new(EventCallbacks::new());
        let mut config = native_host_runtime_config();
        config.local_session_secret = None;
        config.local_session_lite_username = None;
        let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
            .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("myapp.dot", ProductExecutionKind::App),
            )
            .expect("open app execution");

        let key = "myapp.dot/progress".to_string();
        let mut subscription = execution.platform.subscribe_storage(key.clone());
        assert_eq!(
            futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
            Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
            "a subscription opens on the key's current value"
        );

        execution.notify_storage_changed(key, Some(vec![7]));

        assert_eq!(
            futures::FutureExt::now_or_never(futures::StreamExt::next(&mut subscription)),
            Some(Some(Ok(v01::HostLocalStorageChangeItem {
                value: Some(vec![7]),
            }))),
            "a change the host made itself still reaches the product"
        );
    }

    /// The runtime installs its own observer, so a host cannot be left with a
    /// pairing nobody forwards.
    #[test]
    fn the_native_runtime_installs_the_pairing_observer() {
        struct Inert;
        impl crate::DevicePairingObserver for Inert {
            fn device_paired(&self, _device: PairedSsoPeer) {}
        }

        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");

        assert!(!host.runtime.set_device_pairing_observer(Arc::new(Inert)));
    }

    #[test]
    fn process_runtime_shares_authority_and_replaces_one_chat_execution_per_product() {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let app = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                None,
                None,
                None,
                native_execution_config("shared.dot", ProductExecutionKind::App),
            )
            .expect("App execution should open");
        let chat_host = Arc::new(EventCallbacks::new());
        let chat = host
            .open_product_execution(
                chat_host.clone(),
                Some(chat_host.clone()),
                None,
                None,
                native_execution_config("shared.dot", ProductExecutionKind::Worker),
            )
            .expect("Chat execution should open");

        assert!(Arc::ptr_eq(&app.runtime, &chat.runtime));
        assert!(matches!(
            app.publish_chat_action(text_chat_action("denied")),
            Err(crate::ProductRuntimeError::Denied)
        ));
        chat.publish_chat_action(text_chat_action("buffered"))
            .expect("Chat action should buffer before connection");

        let replacement = host
            .open_product_execution(
                chat_host.clone(),
                Some(chat_host.clone()),
                None,
                None,
                native_execution_config("shared.dot", ProductExecutionKind::Worker),
            )
            .expect("replacement Chat execution should open");
        assert!(matches!(
            chat.publish_chat_action(text_chat_action("closed")),
            Err(crate::ProductRuntimeError::Closed)
        ));
        assert!(!replacement.closed.load(Ordering::Acquire));
        replacement
            .publish_chat_action(text_chat_action("fresh"))
            .expect("replacement execution has a fresh buffer");
    }

    #[test]
    fn a_renderer_action_reaches_the_product_that_rendered_it() {
        // The channel is execution-scoped, so the admin handle built from this
        // execution reads what the execution published.
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                None,
                None,
                None,
                native_execution_config("chat.dot", ProductExecutionKind::Worker),
            )
            .expect("Worker execution should open");

        let admin = execution.admin();
        let mut actions = futures::executor::block_on(truapi::api::Renderer::action_subscribe(
            admin.product_runtime().as_ref(),
            &truapi::CallContext::with_request_id("renderer-1".to_string()),
            truapi::versioned::renderer::HostRendererActionSubscribeRequest::V1,
        ));

        let published = v01::HostRendererActionSubscribeItem {
            context: v01::RenderContext::ChatMessage {
                room_id: "support".to_string(),
                message_id: "message-1".to_string(),
                message_type: "vote".to_string(),
            },
            action_id: "approve".to_string(),
            payload: Vec::new(),
        };
        execution
            .publish_renderer_action(published.clone())
            .expect("a Worker execution may publish renderer actions");

        let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
        let delivered = match actions.poll_next_unpin(&mut cx) {
            core::task::Poll::Ready(Some(item)) => item,
            other => panic!("a published renderer action must be ready, got {other:?}"),
        };
        let Ok(truapi::versioned::renderer::HostRendererActionSubscribeItem::V1(delivered)) =
            delivered
        else {
            panic!("expected a renderer action item")
        };
        assert_eq!(delivered, published);
    }

    #[test]
    fn product_execution_routes_chain_events_to_shared_and_scoped_services() {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                None,
                None,
                None,
                native_execution_config("chain.dot", ProductExecutionKind::App),
            )
            .expect("App execution should open");
        let mut shared_responses = host.events.register_chain(41, 0).unwrap();
        let mut scoped_responses = execution.events.register_chain(41, 0).unwrap();
        let response = r#"{"jsonrpc":"2.0","id":"truapi:1","result":true}"#.to_string();

        execution.notify_chain_response(41, response.clone());

        assert_eq!(
            futures::executor::block_on(shared_responses.next()),
            Some(response.clone())
        );
        assert_eq!(
            futures::executor::block_on(scoped_responses.next()),
            Some(response)
        );

        execution.notify_chain_closed(41);
        assert_eq!(futures::executor::block_on(shared_responses.next()), None);
        assert_eq!(futures::executor::block_on(scoped_responses.next()), None);
    }

    #[test]
    fn native_execution_shares_one_use_permissions_across_connections_only() {
        use truapi::api::Permissions;

        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: Ok(PermissionDecision::AllowOnce),
            ..EventCallbacks::new()
        });
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            callbacks.clone(),
            native_host_runtime_config(),
        )
        .unwrap();
        let open = || {
            host.open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("fetch.dot", ProductExecutionKind::App),
            )
            .unwrap()
        };
        let execution = open();
        let other = open();
        futures::executor::block_on(async {
            let admin = execution.admin();
            let request = truapi::latest::RemotePermissionRequest {
                permission: truapi::latest::RemotePermission::Remote {
                    domains: vec!["api.example.com".to_string()],
                },
            };
            let context = truapi::CallContext::default();
            let sdk_request = || {
                admin.product_runtime().request_remote_permission(
                    &context,
                    truapi::versioned::permissions::RemotePermissionRequest::V1(request.clone()),
                )
            };
            let granted = sdk_request().await.unwrap();
            let permission = PermissionAuthorizationRequest::Remote(request.clone());
            let other_status = other
                .permission_authorization_status(permission.clone())
                .await
                .unwrap();
            let consumed = execution
                .authorize_remote_permission(request.clone())
                .await
                .unwrap();
            let after_use = execution
                .permission_authorization_status(permission.clone())
                .await
                .unwrap();
            let prompts_after_use = callbacks.remote_permission_calls.load(Ordering::SeqCst);
            let next_operation = execution
                .authorize_remote_permission(request.clone())
                .await
                .unwrap();
            let other_operation = other
                .authorize_remote_permission(request.clone())
                .await
                .unwrap();
            let prompts_after_operations = callbacks.remote_permission_calls.load(Ordering::SeqCst);
            sdk_request().await.unwrap();
            execution.shutdown();
            let after_shutdown = admin
                .permission_authorization_status(permission)
                .await
                .unwrap();
            assert_eq!(
                (
                    granted,
                    other_status,
                    consumed,
                    after_use,
                    prompts_after_use,
                    next_operation,
                    other_operation,
                    prompts_after_operations,
                    after_shutdown
                ),
                (
                    truapi::versioned::permissions::RemotePermissionResponse::V1(
                        truapi::latest::RemotePermissionResponse { granted: true },
                    ),
                    PermissionAuthorizationStatus::NotDetermined,
                    true,
                    PermissionAuthorizationStatus::NotDetermined,
                    1,
                    true,
                    true,
                    3,
                    PermissionAuthorizationStatus::NotDetermined,
                )
            );
        });
    }

    #[test]
    fn the_runtime_opens_the_core_database_at_startup() {
        // Durable work resumes as soon as the runtime exists, so the database
        // has to be open before the first call reaches it.
        let dir = tempfile::tempdir().unwrap();
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            HostRuntimeConfig {
                database_directory: dir.path().to_string_lossy().into_owned(),
                ..native_host_runtime_config()
            },
        )
        .expect("host runtime config should be valid");

        let status = futures::executor::block_on(host.core_database_status())
            .expect("the core database is open");

        let file = dir
            .path()
            .canonicalize()
            .unwrap()
            .join(crate::store::CORE_DB_FILE);
        assert_eq!(
            status.path,
            Some(file.to_string_lossy().into_owned()),
            "the database lives in the configured directory"
        );
    }

    #[test]
    fn a_missing_database_directory_fails_runtime_creation() {
        // A wrong directory must stop the host at startup, not surface later
        // as durable work that cannot be recorded.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent");

        let result = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            HostRuntimeConfig {
                database_directory: missing.to_string_lossy().into_owned(),
                ..native_host_runtime_config()
            },
        );

        assert!(matches!(
            result,
            Err(NativeRuntimeConfigError::DatabaseUnavailable { .. })
        ));
    }
}
