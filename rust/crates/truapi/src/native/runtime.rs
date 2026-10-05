use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::path::PathBuf;

use crate::platform::{
    CoreAdmin, PermissionAuthorizationRequest, PermissionAuthorizationStatus, ProductContext,
    ProductExecutionKind,
};
use parity_scale_codec::Encode;
use futures::StreamExt;
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
use crate::store::{Db, RuntimeStore, account_core_db_config};
use crate::subscription::Spawner;
use crate::{PairedSsoPeer, ResponderExit, SigningHostRuntime};

use super::callbacks::{
    HostCallbacks, NativeChatCallbacks, NativeContactsCallbacks, NativePocketCallbacks, NativeWalletSecretProvider,
};
use super::config::{
    HostRuntimeConfig, NativeResolvedHostRuntimeConfig, NativeRuntimeConfigError,
    ProductExecutionConfig,
};
use super::errors::{HostRejection, NativeCoreDatabaseError};
use super::executor::shared_native_executor;
use super::events::NativeEventBus;
use super::platform::{
    CallbackPlatform, ChatCallbackPlatform, ContactsCallbackPlatform, PocketCallbackPlatform,
};
#[cfg(doc)]
use crate::WorkerTransition;
#[cfg(doc)]
use super::parse_pairing_deeplink;

/// Process-owned native TrUAPI runtime shared by all executable connections.
#[derive(uniffi::Object)]
pub struct NativeTrUApiHostRuntime {
    callbacks: Arc<dyn HostCallbacks>,
    notifications: Arc<super::notifications::NativeNotifications>,
    workers: Arc<super::workers::NativeWorkers>,
    wallet_secrets: super::storage::WalletSecrets,
    storage: Arc<super::storage::NativeStorage>,
    database_directory: PathBuf,
    owner: Mutex<Option<[u8; 32]>>,
    lifecycle: futures::lock::Mutex<()>,
    records_observer: Mutex<Option<futures::future::AbortHandle>>,
    executions: Mutex<Vec<Weak<NativeProductExecution>>>,
    runtime: Arc<SigningHostRuntime>,
    events: Arc<NativeEventBus>,
    spawner: Spawner,
    ws_bridge: Arc<SharedWsBridge>,
}

impl NativeTrUApiHostRuntime {
    fn from_resolved(
        callbacks: Arc<dyn HostCallbacks>,
        wallet_secrets: Arc<dyn NativeWalletSecretProvider>,
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
        let storage = Arc::new(super::storage::NativeStorage::default());
        let notifications = Arc::new(super::notifications::NativeNotifications::new(storage.clone(), callbacks.clone()));
        let spawner = executor.spawner();
        let workers = super::workers::NativeWorkers::new(storage.clone(), spawner.clone());
        let events = Arc::new(NativeEventBus::default());
        let platform = Arc::new(CallbackPlatform {
            workers: workers.clone(),
            notifications: notifications.clone(),
            storage: storage.clone(),
            product_id: None,
            callbacks: callbacks.clone(),
            events: events.clone(),

        });
        let runtime = Arc::new(SigningHostRuntime::new(
            platform.clone(),
            runtime_config.signing,
            spawner.clone(),
        ));
        assert!(
            runtime
                .worker_ledger()
                .install_demand_observer(workers.clone()),
            "a freshly built runtime installs its worker demand observer once"
        );
        assert!(
            runtime.set_device_pairing_observer(platform),
            "a freshly built runtime installs its device pairing observer once"
        );
        let host = Arc::new(Self {
            callbacks: callbacks.clone(),
            notifications,
            workers: workers.clone(),
            wallet_secrets: super::storage::WalletSecrets(wallet_secrets),
            storage,
            database_directory: runtime_config.database_directory,
            owner: Mutex::new(None),
            lifecycle: futures::lock::Mutex::new(()),
            records_observer: Mutex::new(None),
            executions: Mutex::new(Vec::new()),
            runtime,
            events,
            spawner,
            ws_bridge: Arc::new(SharedWsBridge::new(Arc::new(move |marker, detail| {
                callbacks.on_core_log(marker.to_string(), detail.to_string());
            }))),
        });
        workers.attach(Arc::downgrade(&host));
        Ok(host)
    }

    /// Open the supervisor's execution with product-bound modality adapters.
    pub fn open_worker_execution(&self, product: ProductContext, chat: Option<Arc<dyn NativeChatCallbacks>>, pocket: Option<Arc<dyn NativePocketCallbacks>>) -> Arc<NativeProductExecution> {
        self.open_product_execution_with_callbacks(self.callbacks.clone(), chat, pocket, product)
    }

    async fn deactivate(&self) -> Result<(), HostRejection> {
        self.stop_records_observer();
        let account_result = self.runtime.disconnect_session().await;
        let worker_result = self.workers.suspend().await;
        self.close_executions();
        let notification_result = if self.storage.current().is_ok() {
            self.notifications.suspend().await
        } else {
            Ok(())
        };
        let storage_result = self.storage.lock().await;
        account_result?;
        worker_result?;
        notification_result?;
        storage_result.map_err(rejection)
    }

    fn observe_records(&self, store: &RuntimeStore) {
        self.stop_records_observer();
        let mut records = store.observe_records();
        let callbacks = self.callbacks.clone();
        let (abort, registration) = futures::future::AbortHandle::new_pair();
        *self.records_observer.lock().expect("record observer mutex poisoned") = Some(abort);
        (self.spawner)(Box::pin(async move {
            let _ = futures::future::Abortable::new(async move {
                while let Some(result) = records.next().await {
                    if result.is_err() { break }
                    callbacks.runtime_records_changed();
                }
            }, registration).await;
        }));
    }

    fn stop_records_observer(&self) {
        if let Some(observer) = self.records_observer.lock().expect("record observer mutex poisoned").take() {
            observer.abort();
        }
    }

    fn close_executions(&self) {
        let executions = core::mem::take(&mut *self.executions.lock().expect("native execution registry mutex poisoned"));
        for execution in executions.into_iter().filter_map(|execution| execution.upgrade()) {
            execution.shutdown();
        }
    }

    fn close_product_executions(&self, product_id: &str) {
        let mut executions = self.executions.lock().expect("native execution registry mutex poisoned");
        executions.retain(|execution| {
            let Some(execution) = execution.upgrade() else { return false };
            if execution.product.product_id == product_id {
                execution.shutdown();
                false
            } else {
                true
            }
        });
    }

    fn open_product_execution_with_callbacks(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
        pocket_callbacks: Option<Arc<dyn NativePocketCallbacks>>,
        product: ProductContext,
    ) -> Arc<NativeProductExecution> {
        let events = Arc::new(NativeEventBus::default());
        let storage = self.storage.snapshot_for_product(&product.product_id);
        let callback_platform = Arc::new(CallbackPlatform {
            workers: self.workers.clone(),
            notifications: Arc::new(self.notifications.scoped(storage.clone())),
            storage,
            product_id: Some(product.product_id.clone()),
            callbacks: callbacks.clone(),
            events: events.clone(),

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
        let execution = Arc::new(NativeProductExecution {
            runtime: self.runtime.clone(),
            product: product.clone(),
            platform,
            chat,
            pocket,
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
            on_disconnect: Mutex::new(None),
        });

        self.executions.lock().expect("native execution registry mutex poisoned").push(Arc::downgrade(&execution));


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
    /// Pairing orchestration reports any failure-notification or renewal-cleanup error in the reason.
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
        wallet_secrets: Arc<dyn NativeWalletSecretProvider>,
        runtime_config: HostRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        let runtime_config: NativeResolvedHostRuntimeConfig = runtime_config.try_into()?;
        Self::from_resolved(
            callbacks,
            wallet_secrets,
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

    /// Open an App or Widget execution; the core supervisor creates workers.
    pub fn open_product_execution(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        execution_config: ProductExecutionConfig,
    ) -> Result<Arc<NativeProductExecution>, NativeRuntimeConfigError> {
        let product: ProductContext = execution_config.try_into()?;
        if product.execution_kind == ProductExecutionKind::Worker {
            return Err(NativeRuntimeConfigError::Invalid {
                reason: "worker executions are owned by the core supervisor".to_string(),
            });
        }
        Ok(self.open_product_execution_with_callbacks(
            callbacks,
            None,
            None,
            product,
        ))
    }

    /// Install OS engine adapters; Rust decides which workers run.
    pub fn set_worker_engine_host(&self, engine: Arc<dyn super::workers::NativeWorkerEngineHost>) -> bool {
        self.workers.set_engine(engine)
    }

    /// Persist a chat or card reference and reconcile its worker.
    pub async fn notify_worker_intent(&self, product_id: String, action: super::workers::WorkerIntentAction, modality: super::workers::WorkerModality) -> Result<(), HostRejection> {
        self.workers.intent(product_id, action, modality).await
    }

    /// Restore workers after foregrounding and check bundle updates.
    pub async fn update_workers(&self) -> Result<(), HostRejection> {
        self.workers.resume().await
    }

    /// Report asynchronous engine startup failure against the exact execution.
    pub fn notify_worker_failed(&self, execution: Arc<NativeProductExecution>, reason: String) {
        self.workers.engine_failed(execution, reason);
    }

    /// Suspend engines without treating OS suspension as an engine crash.
    pub async fn suspend_workers(&self) -> Result<(), HostRejection> {
        self.workers.suspend().await
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

    /// Allocate the wallet and peer statement accounts, announce progress and answer the handshake.
    /// Failed pairings notify the peer and remove only renewal targets added by this attempt.
    pub async fn establish_pairing(&self, deeplink: String) -> Result<(), NativePairingError> {
        reject_undecodable_deeplink(&deeplink)?;
        self.runtime.establish_pairing_with_allowances(&deeplink).await.map_err(NativePairingError::from)
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
    pub async fn disconnect(&self) -> Result<(), HostRejection> {
        self.lock_wallet().await
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

    /// Activate an unlocked wallet by its protected-store identifier.
    pub async fn activate_wallet(&self, wallet_id: String, lite_username: Option<String>) -> Result<(), HostRejection> {
        let _lifecycle = self.lifecycle.lock().await;
        self.deactivate().await?;
        let prepared = self.runtime.prepare_wallet(&self.wallet_secrets, &wallet_id, lite_username).await?;
        let owner = prepared.owner_public_key();
        let previous = *self.owner.lock().expect("native owner mutex poisoned");
        if previous.is_some_and(|previous| previous != owner) {
            return Err(HostRejection::Rejected { reason: "account switching requires a new native runtime".to_string() });
        }
        std::fs::create_dir_all(self.database_directory.join(hex::encode(owner))).map_err(rejection)?;
        let database = Db::open(account_core_db_config(&self.database_directory, &owner)).await
            .map_err(|error| HostRejection::Rejected { reason: error.to_string() })?;
        let store = Arc::new(RuntimeStore::open(database.clone(), Arc::new(super::storage::NativeSecrets(self.callbacks.clone())), owner).await
            .map_err(|error| HostRejection::Rejected { reason: error.to_string() })?);
        if previous.is_none() {
            assert!(self.runtime.set_core_db(database));
            *self.owner.lock().expect("native owner mutex poisoned") = Some(owner);
        }
        self.runtime.set_runtime_store(store.clone());
        self.observe_records(&store);
        self.storage.install(store);
        let activation = async {
            self.runtime.activate_wallet(prepared).await?;
            self.notifications.reconcile().await?;
            self.workers.resume().await
        }.await;
        if let Err(error) = activation {
            return match self.deactivate().await {
                Ok(()) => Err(error),
                Err(cleanup) => Err(rejection(format!("{error}; activation cleanup failed: {cleanup}"))),
            }
        }
        Ok(())
    }

    /// Clear wallet memory and stop product work while retaining durable grants.
    pub async fn lock_wallet(&self) -> Result<(), HostRejection> {
        let _lifecycle = self.lifecycle.lock().await;
        self.deactivate().await
    }

    /// Stop all listeners and engines before replacing the selected native runtime.
    pub async fn shutdown(&self) -> Result<(), HostRejection> {
        self.lock_wallet().await
    }

    /// Catalog metadata belongs to Rust; manifest authorization remains independent.
    pub async fn products(&self) -> Result<Vec<crate::store::ProductRecord>, HostRejection> {
        self.storage.current()?.products().await.map_err(rejection)
    }

    /// Update display metadata without granting product permissions.
    pub async fn save_product(&self, product: crate::store::ProductRecord) -> Result<(), HostRejection> {
        self.storage.current()?.save_product(product).await.map_err(rejection)
    }

    /// Cancel OS delivery before removing the owning product records.
    pub async fn remove_product(&self, product_id: String) -> Result<(), HostRejection> {
        let _lifecycle = self.lifecycle.lock().await;
        let product_id = crate::platform::normalize_product_identifier(&product_id).map_err(rejection)?;
        let store = self.storage.current()?;
        store.revoke_product(&product_id).await.map_err(rejection)?;
        self.close_product_executions(&product_id);
        self.workers.stop_product(&product_id).await?;
        self.notifications.cancel_product(product_id.clone()).await?;
        self.runtime.clear_product_state(&product_id).await?;
        store.remove_product(product_id.clone()).await.map_err(rejection)?;
        store.finish_product_removal(&product_id).map_err(rejection)?;
        self.workers.changed(product_id);
        Ok(())
    }

    /// Clear this owner's runtime state only after OS cancellation succeeds.
    pub async fn reset_account(&self) -> Result<(), HostRejection> {
        let _lifecycle = self.lifecycle.lock().await;
        self.stop_records_observer();
        self.workers.suspend().await?;
        self.close_executions();
        let store = self.storage.current()?;
        for product in store.products().await.map_err(rejection)? {
            self.notifications.cancel_product(product.product_id).await?;
        }
        self.runtime.reset_account_state().await?;
        store.reset_and_deactivate().await.map_err(rejection)?;
        self.storage.lock().await.map_err(rejection)
    }

    /// Saved product permission answers, using the canonical permission encoding.
    pub async fn permissions(&self) -> Result<Vec<crate::store::PermissionRecord>, HostRejection> {
        self.storage.current()?.permissions().await.map_err(rejection)
    }

    /// Resolve stored consent and current OS permission state for administration.
    pub async fn permission_authorization_status(&self, product_id: String, request: PermissionAuthorizationRequest) -> Result<PermissionAuthorizationStatus, HostRejection> {
        let product = ProductContext::new(product_id).map_err(rejection)?;
        self.runtime.product_admin(product).permission_authorization_status(request).await.map_err(Into::into)
    }

    /// Update the single Rust-owned permission record used by product calls.
    pub async fn set_permission_authorization_status(&self, product_id: String, request: PermissionAuthorizationRequest, status: PermissionAuthorizationStatus) -> Result<(), HostRejection> {
        let product = ProductContext::new(product_id).map_err(rejection)?;
        self.runtime.product_admin(product.clone()).set_permission_authorization_status(request.clone(), status).await?;
        if request == PermissionAuthorizationRequest::Device(crate::latest::HostDevicePermissionRequest::Notifications)
            && status != PermissionAuthorizationStatus::Authorized {
            self.notifications.cancel_product(product.product_id).await?;
        }
        Ok(())
    }

    /// Authenticated paired hosts and their display metadata.
    pub async fn paired_hosts(&self) -> Result<Vec<crate::store::PairedHostRecord>, HostRejection> {
        self.storage.current()?.paired_hosts().await.map_err(rejection)
    }

    /// Remove persisted pairing metadata and its replay state together.
    pub async fn remove_paired_host(&self, peer_statement: Bytes32, peer_encryption: Bytes32) -> Result<(), HostRejection> {
        self.storage.current()?.remove_paired_host(peer_statement, peer_encryption).await.map_err(rejection)
    }

    /// Public resource allocation history used by native administration.
    pub async fn allowance_records(&self) -> Result<Vec<crate::store::AllowanceRecord>, HostRejection> {
        self.storage.current()?.allowances().await.map_err(rejection)
    }

    /// Detailed statement slots remain authoritative for priority and renewal timing.
    pub async fn statement_slots(&self) -> Result<Vec<crate::store::StatementSlotRecord>, HostRejection> {
        self.storage.current()?.statement_slots().await.map_err(rejection)
    }

    /// Durable notification state, including OS work awaiting retry.
    pub async fn scheduled_notifications(&self) -> Result<Vec<crate::store::ScheduledNotificationRecord>, HostRejection> {
        self.storage.current()?.notifications().await.map_err(rejection)
    }

    /// Retry pending OS work without discarding failed registrations or cancellations.
    pub async fn reconcile_notifications(&self) -> Result<(), HostRejection> {
        self.notifications.reconcile().await.map_err(Into::into)
    }

    /// Durable worker operations, including unlabeled Android operations.
    pub async fn worker_operations(&self) -> Result<Vec<crate::store::WorkerOperationRecord>, HostRejection> {
        self.storage.current()?.worker_operations().await.map_err(rejection)
    }

    /// Installed worker bundles and deferred content updates.
    pub async fn workers(&self) -> Result<Vec<crate::store::ProductWorkerRecord>, HostRejection> {
        self.storage.current()?.workers().await.map_err(rejection)
    }

    /// Durable integrations retaining one product worker across restarts.
    pub async fn worker_reasons(&self, product_id: String) -> Result<Vec<crate::store::WorkerReason>, HostRejection> {
        self.storage.current()?.worker_reasons(product_id).await.map_err(rejection)
    }

    /// Keep a product worker alive for a native background operation.
    pub async fn begin_worker_operation(&self, product_id: String, label: Option<String>) -> Result<crate::store::WorkerOperationRecord, HostRejection> {
        self.workers.begin_operation(product_id, label).await
    }

    /// Finish the durable operation, preserving independent chat and card reasons.
    pub async fn end_worker_operation(&self, product_id: String, operation_id: u32) -> Result<(), HostRejection> {
        self.workers.end_operation(product_id, operation_id).await
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
    on_disconnect: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    runtime: Arc<SigningHostRuntime>,
    product: ProductContext,
    platform: Arc<dyn crate::platform::Platform>,
    chat: Option<Arc<dyn crate::platform::ChatPlatform>>,
    pocket: Option<Arc<dyn crate::platform::PocketPlatform>>,
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
    product_control: Arc<Mutex<Option<crate::ProductRuntimeControl<crate::runtime::WalletAccountHolder>>>>,
}

impl NativeProductExecution {
    /// Closed executions cannot count as running engines while OS teardown retries.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Observe bridge loss for core-owned worker restart.
    pub fn observe_disconnect(&self, observer: Arc<dyn Fn() + Send + Sync>) {
        *self.on_disconnect.lock().expect("disconnect observer mutex poisoned") = Some(observer);
    }

    fn adapters(&self) -> crate::host_core::ConnectionAdapters {
        crate::host_core::ConnectionAdapters {
            platform: self.platform.clone(),
            chat_platform: self.chat.clone(),
            permission_status: Some(self.permission_status.clone()),
            permission_grants: self.permission_grants.clone(),
            chat: self.chat_connection.clone(),
            renderer: self.renderer_connection.clone(),
            pocket_platform: self.pocket.clone(),
        }
    }

    fn admin(&self) -> crate::HostAdmin<crate::runtime::WalletAccountHolder> {
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
    /// Trusted product identity bound before executable code starts.
    pub fn product_id(&self) -> String {
        self.product.product_id.clone()
    }

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
            let observer = self.on_disconnect.lock().expect("disconnect observer mutex poisoned").clone();
            Arc::new(move |marker: &str, detail: &str| {
                if marker == "truapi.ws_bridge.connection_closed" && let Some(observer) = &observer { observer(); }
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
    fn direct_worker_open_cannot_replace_the_supervisors_execution() {
        use crate::native::workers::{NativeWorkerEngineHost, WorkerBundle, WorkerIntentAction, WorkerModality};

        #[derive(Default)]
        struct CapturingEngine(Mutex<Vec<Arc<NativeProductExecution>>>);

        #[async_trait::async_trait]
        impl NativeWorkerEngineHost for CapturingEngine {
            fn chat_callbacks(&self, _: String) -> Option<Arc<dyn NativeChatCallbacks>> { Some(Arc::new(EventCallbacks::new())) }
            fn pocket_callbacks(&self, _: String) -> Option<Arc<dyn NativePocketCallbacks>> { None }
            fn start_worker(&self, _: String, execution: Arc<NativeProductExecution>, _: WorkerBundle) -> Result<(), HostRejection> {
                self.0.lock().unwrap().push(execution);
                Ok(())
            }
            async fn stop_worker(&self, _: String) -> Result<(), HostRejection> {
                self.0.lock().unwrap().clear();
                Ok(())
            }
            async fn fetch_worker_bundle(&self, _: String, _: Option<Vec<u8>>) -> Result<WorkerBundle, HostRejection> {
                Ok(WorkerBundle { content_hash: vec![1], manifest: b"{}".to_vec(), local_path: "/bundles/test-worker".into() })
            }
        }

        let callbacks = Arc::new(EventCallbacks::new());
        let host = native_runtime(callbacks.clone(), native_host_runtime_config()).unwrap();
        let engine = Arc::new(CapturingEngine::default());
        assert!(host.set_worker_engine_host(engine.clone()));
        futures::executor::block_on(async {
            host.notify_worker_intent("chat.dot".into(), WorkerIntentAction::Add, WorkerModality::Chat).await.unwrap();
            let execution = engine.0.lock().unwrap()[0].clone();
            let direct = host.open_product_execution(
                callbacks,
                native_execution_config("chat.dot", ProductExecutionKind::Worker),
            );
            assert_eq!(
                (
                    matches!(direct, Err(NativeRuntimeConfigError::Invalid { .. })),
                    execution.publish_chat_action(text_chat_action("still-running")).is_ok(),
                    engine.0.lock().unwrap().len(),
                ),
                (true, true, 1),
            );
            host.lock_wallet().await.unwrap();
        });
    }

    #[test]
    fn a_worker_write_reaches_a_storage_subscription_in_the_products_other_execution() {
        let callbacks = Arc::new(EventCallbacks::new());
        let config = native_host_runtime_config();


        let host = native_runtime(callbacks.clone(), config)
            .expect("host runtime config should be valid");
        let screen = host
            .open_product_execution(
                callbacks.clone(),
                native_execution_config("myapp.dot", ProductExecutionKind::App),
            )
            .expect("open app execution");
        let worker = host.open_worker_execution(
            ProductContext::new_with_execution("myapp.dot".into(), ProductExecutionKind::Worker).unwrap(),
            None,
            None,
        );

        let key = crate::platform::ProductStorageKey::new("myapp.dot", "progress").unwrap().encode();
        let mut subscription = screen.platform.subscribe_storage(key.clone());
        assert_eq!(
            futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
            Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
            "a subscription opens on the key's current value"
        );

        futures::executor::block_on(worker.platform.write(key, vec![9])).expect("write");

        assert_eq!(
            futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
            Some(Ok(v01::HostLocalStorageChangeItem {
                value: Some(vec![9]),
            })),
            "the screen and the worker share one storage namespace, so a write in one \
             reaches a subscription in the other"
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

        let host = native_runtime(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");

        assert!(!host.runtime.set_device_pairing_observer(Arc::new(Inert)));
    }

    #[test]
    fn process_runtime_shares_authority_and_keeps_worker_actions_scoped() {
        let host = native_runtime(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let app = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                native_execution_config("shared.dot", ProductExecutionKind::App),
            )
            .expect("App execution should open");
        let chat_host = Arc::new(EventCallbacks::new());
        let chat = host.open_worker_execution(
            ProductContext::new_with_execution("shared.dot".into(), ProductExecutionKind::Worker).unwrap(),
            Some(chat_host),
            None,
        );

        assert!(Arc::ptr_eq(&app.runtime, &chat.runtime));
        assert!(matches!(
            app.publish_chat_action(text_chat_action("denied")),
            Err(crate::ProductRuntimeError::Denied)
        ));
        chat.publish_chat_action(text_chat_action("buffered"))
            .expect("Chat action should buffer before connection");

        chat.shutdown();
        assert!(matches!(
            chat.publish_chat_action(text_chat_action("closed")),
            Err(crate::ProductRuntimeError::Closed)
        ));
    }

    #[test]
    fn a_renderer_action_reaches_the_product_that_rendered_it() {
        // The channel is execution-scoped, so the admin handle built from this
        // execution reads what the execution published.
        let host = native_runtime(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let execution = host.open_worker_execution(
            ProductContext::new_with_execution("chat.dot".into(), ProductExecutionKind::Worker).unwrap(),
            None,
            None,
        );

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
        let host = native_runtime(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
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
        let host = native_runtime(
            callbacks.clone(),
            native_host_runtime_config(),
        )
        .unwrap();
        let open = || {
            host.open_product_execution(
                callbacks.clone(),
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
    fn activation_opens_the_owner_database() {
        let dir = tempfile::tempdir().unwrap();
        let host = native_runtime(
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
            .join(hex::encode(host.owner.lock().unwrap().unwrap()))
            .join(crate::store::CORE_DB_FILE);
        assert_eq!(
            status.path,
            Some(file.to_string_lossy().into_owned()),
            "the database lives in the configured directory"
        );
    }

    #[test]
    fn an_unwritable_database_directory_fails_wallet_activation() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("file");
        std::fs::write(&missing, b"not a directory").unwrap();

        let result = native_runtime(
            Arc::new(EventCallbacks::new()),
            HostRuntimeConfig {
                database_directory: missing.to_string_lossy().into_owned(),
                ..native_host_runtime_config()
            },
        );

        assert!(matches!(
            result,
            Err(NativeRuntimeConfigError::LocalSessionActivation { .. })
        ));
    }

    #[test]
    fn removed_product_cannot_restore_data_through_an_old_execution() {
        let callbacks = Arc::new(EventCallbacks::new());
        let host = native_runtime(callbacks.clone(), native_host_runtime_config()).unwrap();
        futures::executor::block_on(async {
            let old = host.open_product_execution(callbacks.clone(), native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            let key = crate::platform::ProductStorageKey::new("chess.dot", "save").unwrap().encode();
            old.platform.write(key.clone(), vec![1]).await.unwrap();
            host.remove_product("chess.dot".into()).await.unwrap();
            let current = host.open_product_execution(callbacks, native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            assert_eq!(current.platform.read(key.clone()).await.unwrap(), None);
            current.platform.write(key.clone(), vec![2]).await.unwrap();
            assert!(old.platform.write(key.clone(), vec![3]).await.is_err());
            assert_eq!(current.platform.read(key).await.unwrap(), Some(vec![2]));
            host.lock_wallet().await.unwrap();
        });
    }

    #[test]
    fn failed_notification_restore_leaves_wallet_and_storage_inactive() {
        let callbacks = Arc::new(EventCallbacks::new());
        let host = native_runtime(callbacks.clone(), native_host_runtime_config()).unwrap();
        futures::executor::block_on(async {
            let store = host.storage.current().unwrap();
            store.ensure_product("chess.dot".into()).await.unwrap();
            store.prepare_notification("chess.dot".into(), "Your turn".into(), None, Some(i64::MAX), 64).await.unwrap();
            host.lock_wallet().await.unwrap();
            *callbacks.notification_registration_error.lock().unwrap() = Some("OS unavailable".into());
            assert!(host.activate_wallet("fixture-wallet".into(), None).await.is_err());
            assert_eq!((host.runtime.has_active_session(), host.storage.current().is_ok()), (false, false));
            *callbacks.notification_registration_error.lock().unwrap() = None;
            host.activate_wallet("fixture-wallet".into(), None).await.unwrap();
            assert_eq!(host.scheduled_notifications().await.unwrap().len(), 1);
            host.lock_wallet().await.unwrap();
        });
    }

    #[test]
    fn owner_switching_isolates_records_and_reactivation_rejects_old_execution_writes() {
        struct Wallets;
        #[async_trait::async_trait]
        impl NativeWalletSecretProvider for Wallets {
            async fn read_wallet_root_entropy(&self, wallet_id: String) -> Result<Vec<u8>, HostRejection> {
                Ok(vec![if wallet_id == "first" { 7 } else { 8 }; 32])
            }
        }

        futures::executor::block_on(async {
            let callbacks = Arc::new(EventCallbacks::new());
            let config = native_host_runtime_config();
            let first = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), Arc::new(Wallets), config.clone()).unwrap();
            first.activate_wallet("first".into(), None).await.unwrap();
            let old_execution = first.open_product_execution(callbacks.clone(), native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            let key = crate::platform::ProductStorageKey::new("chess.dot", "save").unwrap().encode();
            old_execution.platform.write(key.clone(), vec![1, 2, 3]).await.unwrap();
            first.lock_wallet().await.unwrap();
            first.activate_wallet("first".into(), None).await.unwrap();
            assert!(old_execution.platform.write(key.clone(), vec![9]).await.is_err());
            let current = first.open_product_execution(callbacks.clone(), native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            assert_eq!(current.platform.read(key.clone()).await.unwrap(), Some(vec![1, 2, 3]));
            assert!(first.activate_wallet("second".into(), None).await.is_err());
            assert!(current.platform.read(key.clone()).await.is_err());

            let second = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), Arc::new(Wallets), config.clone()).unwrap();
            second.activate_wallet("second".into(), None).await.unwrap();
            let other = second.open_product_execution(callbacks.clone(), native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            assert_eq!(other.platform.read(key.clone()).await.unwrap(), None);
            other.platform.write(key.clone(), vec![4, 5, 6]).await.unwrap();
            second.lock_wallet().await.unwrap();

            let reopened = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), Arc::new(Wallets), config).unwrap();
            reopened.activate_wallet("first".into(), None).await.unwrap();
            let restored = reopened.open_product_execution(callbacks, native_execution_config("chess.dot", ProductExecutionKind::App)).unwrap();
            assert_eq!(restored.platform.read(key).await.unwrap(), Some(vec![1, 2, 3]));
            reopened.lock_wallet().await.unwrap();
        });
    }
}

fn rejection(reason: impl ToString) -> HostRejection {
    HostRejection::Rejected { reason: reason.to_string() }
}
