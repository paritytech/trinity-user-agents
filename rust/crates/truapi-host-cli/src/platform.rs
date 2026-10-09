//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and a [`UserConfirmation`] that
//! either auto-accepts or prompts on the CLI (the web/iOS "sign?" modal).
//! Auth-state transitions are published on a channel so the CLI can print the
//! pairing deeplink and observe connection status.

use std::collections::HashMap;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use truapi::latest as api;
use truapi::platform::{
    AuthState, ChainProvider, CoreStorage, CoreStorageKey, CreateTransactionReview,
    DevicePermissionStatus, Features, JsonRpcConnection, LocaleHost, Navigation, Notifications,
    PermissionDecision, PermissionStatusHost, Permissions, PreimageHost, ProductContext,
    ProductOperations, ProductStorage, ProductStorageKey, ProviderError, SessionUiInfo,
    SignPayloadReview, SignRawReview, ThemeHost, UserConfirmation, UserConfirmationReview,
};
use truapi::v01;

use crate::bulletin_lookup::{BitswapRpc, BulletinLookup};
use crate::chain::WsChainProvider;
use crate::terminal_ui::{ApprovalKind, SystemEvent, UiHandle};

static NEXT_STORAGE_TEMP_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_OPERATION_ID: AtomicU32 = AtomicU32::new(1);

/// How the host answers confirmation prompts (the web/iOS "sign?" modals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        Self {
            state_dir,
            product_storage_dir,
            pairing_scope: None,
        }
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
            pairing_scope: Some(PairingStorageScope {
                network_dir,
                bootstrap_dir,
            }),
        }
    }
}

/// Headless-host platform shared by both roles.
pub struct CliPlatform {
    chain: WsChainProvider,
    /// Chain roles this host serves, answered by `Features::supported_chains`.
    chains: truapi::platform::HostChainSet,
    product_storage: Mutex<HashMap<String, HashMap<String, Vec<u8>>>>,
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
        self.chain.routed_url(genesis_hash)
    }

    /// Build a platform whose chain provider connects to the network's People
    /// chain and whose optional state directory backs product/core storage.
    pub fn new(
        network: crate::network::NetworkConfig,
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
        let product_storage = product_storage_dir
            .as_deref()
            .map(load_product_storage)
            .unwrap_or_default();
        let core_storage = core_storage_path
            .as_deref()
            .map(load_hex_key_map)
            .unwrap_or_default();
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
        let device_storage = device_storage_path
            .as_deref()
            .map(load_hex_key_map)
            .unwrap_or_default();

        Arc::new(Self {
            chain: WsChainProvider::new(network.people_ws, network.live_chain_endpoints),
            chains: network.host_chain_set(),
            product_storage: Mutex::new(product_storage),
            core_storage: Mutex::new(core_storage),
            device_storage: Mutex::new(device_storage),
            product_storage_dir: Mutex::new(product_storage_dir),
            core_storage_path: Mutex::new(core_storage_path),
            device_storage_path,
            state_dir: Mutex::new(storage.as_ref().map(|paths| paths.state_dir.clone())),
            pairing_scope: storage.and_then(|paths| paths.pairing_scope),
            bulletin: Arc::new(BulletinLookup::new(BitswapRpc::new(network.bulletin_ws))),
            next_notification_id: AtomicU32::new(1),
            scheduled_notifications: Arc::new(Mutex::new(HashMap::new())),
            approval: Mutex::new(approval),
            approvals_log: std::env::var_os("TRUAPI_APPROVALS_LOG").map(PathBuf::from),
            ui,
            prompt_lock: AsyncMutex::new(()),
        })
    }

    /// Return the policy used by future confirmation requests.
    pub fn approval_policy(&self) -> ApprovalPolicy {
        *self
            .approval
            .lock()
            .expect("approval policy mutex poisoned")
    }

    /// Change how future confirmation requests are decided.
    pub fn set_approval_policy(&self, approval: ApprovalPolicy) {
        *self
            .approval
            .lock()
            .expect("approval policy mutex poisoned") = approval;
    }

    fn core_key(key: &CoreStorageKey) -> Vec<u8> {
        use parity_scale_codec::Encode;
        key.encode()
    }

    fn persist_product_storage(
        &self,
        product_id: &str,
        values: &HashMap<String, Vec<u8>>,
    ) -> Result<(), String> {
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
    fn is_device_scoped(key: &CoreStorageKey) -> bool {
        matches!(key, CoreStorageKey::DeviceEncryptionKey)
    }

    fn persist_device_storage(&self) -> Result<(), String> {
        let Some(path) = self.device_storage_path.as_deref() else {
            return Ok(());
        };
        let storage = self
            .device_storage
            .lock()
            .expect("device storage mutex poisoned");
        save_hex_key_map(path, &storage)
    }

    fn persist_core_storage(&self) -> Result<(), String> {
        let Some(path) = self
            .core_storage_path
            .lock()
            .expect("core storage path mutex poisoned")
            .clone()
        else {
            return Ok(());
        };
        let storage = self
            .core_storage
            .lock()
            .expect("core storage mutex poisoned");
        save_hex_key_map(&path, &storage)
    }

    /// Current identity-owned state directory (or the pairing bootstrap before
    /// a user has connected).
    pub fn state_dir(&self) -> Option<PathBuf> {
        self.state_dir
            .lock()
            .expect("state path mutex poisoned")
            .clone()
    }

    fn switch_pairing_user_storage(&self, user_id: &str) -> Result<(), String> {
        let Some(scope) = &self.pairing_scope else {
            return Ok(());
        };
        let user_id = pairing_storage_name(user_id);
        let user_id = user_id.as_ref();
        let target_state = scope.network_dir.join(format!("{user_id}_pairing_host"));
        let current_state = self
            .state_dir
            .lock()
            .expect("state path mutex poisoned")
            .clone();
        if current_state.as_ref() == Some(&target_state) {
            persist_current_pairing_user(&scope.bootstrap_dir, user_id)?;
            return Ok(());
        }

        fs::create_dir_all(&target_state)
            .map_err(|error| format!("create {}: {error}", target_state.display()))?;
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
        .map(|key| Self::core_key(&key));
        let carried = {
            // Keep the same path -> storage lock order used by persistence so
            // an auth transition cannot deadlock with a concurrent core write.
            let current_path = self
                .core_storage_path
                .lock()
                .expect("core storage path mutex poisoned");
            let mut current = self
                .core_storage
                .lock()
                .expect("core storage mutex poisoned");
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
                self.product_storage
                    .lock()
                    .expect("product storage mutex poisoned")
                    .clone(),
            );
            for (product_id, values) in &target_products {
                save_product_storage(&target_product_dir, product_id, values)?;
            }
        }
        {
            let mut core_storage_path = self
                .core_storage_path
                .lock()
                .expect("core storage path mutex poisoned");
            let mut core_storage = self
                .core_storage
                .lock()
                .expect("core storage mutex poisoned");
            *core_storage = target_core;
            *core_storage_path = Some(target_core_path);
        }
        *self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned") = target_products;
        *self
            .product_storage_dir
            .lock()
            .expect("product storage path mutex poisoned") = Some(target_product_dir);
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
            }
            ApprovalPolicy::Prompt => {
                let _guard = self.prompt_lock.lock().await;
                if let Some(ui) = &self.ui {
                    ui.decide(action, detail, kind).await
                } else {
                    prompt_decision(action, &detail, kind).await
                }
            }
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
                "\n\u{2500}\u{2500} confirm: {action} \u{2500}\u{2500}\n{detail}\n{} (default: deny) ", kind.choices()
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

#[async_trait]
impl ProductStorage for CliPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })?;
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
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })?;
        let mut storage = self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned");
        let values = storage.entry(scoped.product_id().to_string()).or_default();
        values.insert(scoped.key().to_string(), value);
        self.persist_product_storage(scoped.product_id(), values)
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })?;
        let mut storage = self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned");
        let values = storage.entry(scoped.product_id().to_string()).or_default();
        values.remove(scoped.key());
        self.persist_product_storage(scoped.product_id(), values)
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
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
        Box::pin(stream::once(async move {
            Ok(api::HostLocalStorageChangeItem { value })
        }))
    }
}

#[async_trait]
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

#[async_trait]
impl CoreStorage for CliPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, api::GenericError> {
        let store = if Self::is_device_scoped(&key) {
            &self.device_storage
        } else {
            &self.core_storage
        };
        Ok(store
            .lock()
            .expect("core storage mutex poisoned")
            .get(&Self::core_key(&key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), api::GenericError> {
        let device_scoped = Self::is_device_scoped(&key);
        {
            let store = if device_scoped {
                &self.device_storage
            } else {
                &self.core_storage
            };
            store
                .lock()
                .expect("core storage mutex poisoned")
                .insert(Self::core_key(&key), value);
        }
        if device_scoped {
            self.persist_device_storage()
        } else {
            self.persist_core_storage()
        }
        .map_err(|reason| api::GenericError { reason })
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), api::GenericError> {
        let device_scoped = Self::is_device_scoped(&key);
        {
            let store = if device_scoped {
                &self.device_storage
            } else {
                &self.core_storage
            };
            store
                .lock()
                .expect("core storage mutex poisoned")
                .remove(&Self::core_key(&key));
        }
        if device_scoped {
            self.persist_device_storage()
        } else {
            self.persist_core_storage()
        }
        .map_err(|reason| api::GenericError { reason })
    }
}

#[async_trait]
impl ChainProvider for CliPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        self.chain.connect(genesis_hash).await
    }
}

#[async_trait]
impl Navigation for CliPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), api::HostNavigateToError> {
        tracing::debug!(%url, "navigate_to");
        Ok(())
    }
}

#[async_trait]
impl Notifications for CliPlatform {
    async fn push_notification(
        &self,
        notification: api::HostPushNotificationRequest,
    ) -> Result<api::HostPushNotificationResponse, api::GenericError> {
        let id = self.next_notification_id.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if let Some(scheduled_at) = notification.scheduled_at.filter(|at| *at > now) {
            {
                let mut pending = self
                    .scheduled_notifications
                    .lock()
                    .expect("notification mutex poisoned");
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
                let notification = pending
                    .lock()
                    .expect("notification mutex poisoned")
                    .remove(&id);
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

#[async_trait]
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

#[async_trait]
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
            api::RemotePermission::Remote { .. } => format!(
                "{product_id} requested {request}. This covers all ports on each host, including local services."
            ),
            _ => format!("{product_id} requested {request}."),
        };
        Ok(self
            .decide_with("remote permission", detail, ApprovalKind::Permission)
            .await)
    }
}

#[async_trait]
impl Features for CliPlatform {
    async fn feature_supported(
        &self,
        request: api::HostFeatureSupportedRequest,
    ) -> Result<api::HostFeatureSupportedResponse, api::GenericError> {
        let api::HostFeatureSupportedRequest::Chain { genesis_hash } = request;
        let supported = self
            .chains
            .chains
            .iter()
            .any(|entry| entry.genesis_hash.as_slice() == genesis_hash.as_slice());
        Ok(api::HostFeatureSupportedResponse { supported })
    }

    async fn supported_chains(&self) -> Result<truapi::platform::HostChainSet, api::GenericError> {
        Ok(self.chains.clone())
    }
}

impl truapi::platform::AuthPresenter for CliPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        if let AuthState::Connected(info) = &state
            && let Some(user_id) = storage_user_id(info)
            && let Err(reason) = self.switch_pairing_user_storage(user_id)
        {
            tracing::warn!(%reason, %user_id, "could not switch pairing-host user storage");
        }
        let (connection, event) = match &state {
            AuthState::Pairing { deeplink } => (
                "pairing".to_string(),
                SystemEvent::PairingDeeplink {
                    url: deeplink.clone(),
                },
            ),
            AuthState::Authenticating => (
                "authenticating".to_string(),
                SystemEvent::PairingAuthenticating,
            ),
            AuthState::Connected(info) => (
                connected_user_id(info).unwrap_or("connected").to_string(),
                SystemEvent::PairingConnected {
                    user_id: connected_user_id(info).map(str::to_string),
                },
            ),
            AuthState::Disconnected => {
                ("disconnected".to_string(), SystemEvent::PairingDisconnected)
            }
            AuthState::LoginFailed { reason, .. } => (
                "failed".to_string(),
                SystemEvent::PairingFailed {
                    reason: reason.clone(),
                },
            ),
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
        .or_else(|| {
            info.lite_username
                .as_deref()
                .filter(|value| !value.is_empty())
        })
}

fn storage_user_id(info: &SessionUiInfo) -> Option<&str> {
    info.lite_username
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            info.full_username
                .as_deref()
                .filter(|value| !value.is_empty())
        })
}

#[async_trait]
impl UserConfirmation for CliPlatform {
    async fn confirm_permission(
        &self,
        review: UserConfirmationReview,
    ) -> Result<PermissionDecision, api::GenericError> {
        let (action, detail) = approval_summary(&review);
        Ok(self
            .decide_with(action, detail, ApprovalKind::Permission)
            .await)
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
fn asking(calling_product_id: Option<&str>) -> String {
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
                asking(calling_product_id.as_deref()),
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
                asking(calling_product_id.as_deref()),
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
                asking(review.calling_product_id.as_deref()),
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
                asking(calling_product_id.as_deref()),
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
        UserConfirmationReview::PreimageSubmit(review) => (
            "submit preimage",
            format!(
                "A product requested submission of a {}-byte preimage.",
                review.size
            ),
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
            Ok(api::HostLocaleSubscribeItem {
                language_tag: "en".to_string(),
            })
        }))
    }
}

impl PreimageHost for CliPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
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
    product_id: String,
    values: HashMap<String, String>,
}

fn load_product_storage(directory: &Path) -> HashMap<String, HashMap<String, Vec<u8>>> {
    let mut products = HashMap::<String, HashMap<String, Vec<u8>>>::new();

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
        }
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

fn load_product_storage_file(path: &Path) -> Option<(String, HashMap<String, Vec<u8>>)> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not read product storage");
            return None;
        }
    };
    let document = match serde_json::from_str::<ProductStorageDocument>(&text) {
        Ok(document) if document.version == 1 => document,
        Ok(document) => {
            tracing::warn!(
                path = %path.display(),
                version = document.version,
                "unsupported product storage version"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not decode product storage");
            return None;
        }
    };
    let normalized = match ProductStorageKey::new(&document.product_id, "") {
        Ok(scoped) => scoped.product_id().to_string(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "product storage contains an invalid product id"
            );
            return None;
        }
    };
    let values = document
        .values
        .into_iter()
        .map(|(key, value)| hex::decode(value).map(|bytes| (key, bytes)))
        .collect::<Result<HashMap<_, _>, _>>();
    let values = match values {
        Ok(values) => values,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "product storage contains an invalid value"
            );
            return None;
        }
    };
    Some((normalized, values))
}

fn save_product_storage(
    directory: &Path,
    product_id: &str,
    values: &HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "create product storage directory {}: {error}",
            directory.display()
        )
    })?;
    let document = ProductStorageDocument {
        version: 1,
        product_id: product_id.to_string(),
        values: values
            .iter()
            .map(|(key, value)| (key.clone(), hex::encode(value)))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?;
    atomic_write(
        &product_storage_path(directory, product_id),
        text.as_bytes(),
    )
}

fn product_storage_path(directory: &Path, product_id: &str) -> PathBuf {
    let mut slug = product_id
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
    slug = slug
        .trim_matches(|character| matches!(character, '.' | '-'))
        .to_string();
    if slug.is_empty() {
        slug.push_str("product");
    }
    let digest = Sha256::digest(product_id.as_bytes());
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
                Ok(()) => tracing::warn!(
                    path = %path.display(),
                    preserved = %preserved.display(),
                    %err,
                    "could not read CLI storage; moved it aside and started empty"
                ),
                Err(rename_err) => tracing::warn!(
                    path = %path.display(),
                    %err,
                    %rename_err,
                    "could not read CLI storage, and could not move it aside; \
                     the next write will overwrite it"
                ),
            }
            HashMap::new()
        }
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

fn save_string_map(path: &Path, values: &HashMap<String, Vec<u8>>) -> Result<(), String> {
    let json = JsonMap {
        values: values
            .iter()
            .map(|(key, value)| (key.clone(), hex::encode(value)))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).map_err(|err| err.to_string())?;
    atomic_write(path, text.as_bytes())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("storage path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create storage dir: {error}"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    let temporary_id = NEXT_STORAGE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary =
        path.with_file_name(format!(".{name}.{}.{temporary_id}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", temporary.display()))?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("replace {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path).map_err(|error| format!("persist {}: {error}", path.display()))?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync storage dir {}: {error}", parent.display()))?;
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

fn persist_current_pairing_user(bootstrap_dir: &Path, user_id: &str) -> Result<(), String> {
    fs::create_dir_all(bootstrap_dir)
        .map_err(|error| format!("create {}: {error}", bootstrap_dir.display()))?;
    let path = bootstrap_dir.join(CURRENT_PAIRING_USER_FILE);
    let temporary = bootstrap_dir.join(format!(
        ".{CURRENT_PAIRING_USER_FILE}.{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{user_id}\n"))
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path).map_err(|error| format!("persist {}: {error}", path.display()))
}

fn save_hex_key_map(path: &Path, values: &HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
    let keyed: HashMap<String, Vec<u8>> = values
        .iter()
        .map(|(key, value)| (hex::encode(key), value.clone()))
        .collect();
    save_string_map(path, &keyed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The preset production builds from, so tests exercise the same config.
    fn test_network() -> crate::network::NetworkConfig {
        crate::network::Network::default().config()
    }

    /// Battery examples that preflight `getChainInfo` resolve the genesis they
    /// ask for through this set, so an error here fails every one of them.
    ///
    /// Serving the preset's three roles unblocks the preflight in every example
    /// that asks for one: `People` for account-alias, account-proof, ring-VRF
    /// registration and both create-transaction variants, and `AssetHub` for the
    /// other seventeen.
    ///
    /// `feature_supported` answers from the same set `supported_chains` serves, so
    /// the two cannot disagree. The negatives are the malformed inputs, a well-formed
    /// hash the host serves no role for, and the all-zero SSO sentinel — which the
    /// provider does route, to the People fallback, yet is still not a served role.
    #[test]
    fn feature_supported_resolves_against_the_served_chain_set() {
        let platform = CliPlatform::new(test_network(), None, ApprovalPolicy::AutoAccept, None);
        let config = crate::network::Network::default().config();

        let supported = |genesis: Vec<u8>| {
            futures::executor::block_on(platform.feature_supported(
                api::HostFeatureSupportedRequest::Chain {
                    genesis_hash: genesis,
                },
            ))
            .expect("feature_supported is wired")
            .supported
        };

        // Every role the host serves answers supported. The reverse direction is the
        // `unserved` assertion below, since every served role is now a real chain.
        for entry in config.host_chain_set().chains {
            assert!(
                supported(entry.genesis_hash.to_vec()),
                "{:?} is served but reported unsupported",
                entry.identifier
            );
        }

        // A well-formed hash the host does not serve is unsupported. Asset Hub used
        // to be this case; without a stand-in, "supported" could degrade to "is 32
        // bytes" and only the all-zero sentinel would notice.
        let unserved = [0xab; 32];
        assert!(
            !config
                .host_chain_set()
                .chains
                .iter()
                .any(|entry| entry.genesis_hash == unserved),
            "the stand-in must not be a served role"
        );
        assert!(!supported(unserved.to_vec()));

        // A malformed genesis is unsupported, never a panic or a truncated match.
        assert!(!supported(Vec::new()));
        assert!(!supported(config.people_genesis[..31].to_vec()));
        assert!(!supported(
            [config.people_genesis.as_slice(), &[0u8]].concat()
        ));
        assert!(!supported(vec![0u8; 32]));
    }

    #[test]
    fn supported_chains_answers_the_configured_network() {
        let platform = CliPlatform::new(test_network(), None, ApprovalPolicy::AutoAccept, None);
        let set = futures::executor::block_on(platform.supported_chains())
            .expect("the CLI host serves the preset's chains");

        let config = crate::network::Network::default().config();
        assert_eq!(set.network, config.id);
        let mut served = set
            .chains
            .iter()
            .map(|entry| (entry.identifier, hex::encode(entry.genesis_hash)))
            .collect::<Vec<_>>();
        served.sort_by_key(|(identifier, _)| format!("{identifier:?}"));
        let mut expected = vec![
            (
                api::ChainIdentifier::People,
                hex::encode(config.people_genesis),
            ),
            (
                api::ChainIdentifier::Bulletin,
                hex::encode(config.bulletin_genesis),
            ),
            (
                api::ChainIdentifier::AssetHub,
                hex::encode(config.asset_hub_genesis),
            ),
        ];
        expected.sort_by_key(|(identifier, _)| format!("{identifier:?}"));
        assert_eq!(served, expected);
    }
    use tempfile::tempdir;

    #[test]
    fn record_approval_appends_one_line_per_decision() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("approvals.log");
        record_approval(&path, true, "allocate resources");
        record_approval(&path, false, "sign VRF transcript");
        record_approval(&path, true, "sign VRF transcript");
        assert_eq!(
            std::fs::read_to_string(&path).expect("approvals transcript"),
            "approved allocate resources\ndenied sign VRF transcript\napproved sign VRF transcript\n",
        );
    }

    #[tokio::test]
    async fn approval_policy_changes_apply_to_future_confirmations() {
        let platform = CliPlatform::new(test_network(), None, ApprovalPolicy::Prompt, None);

        assert_eq!(platform.approval_policy(), ApprovalPolicy::Prompt);
        platform.set_approval_policy(ApprovalPolicy::AutoAccept);
        assert_eq!(platform.approval_policy(), ApprovalPolicy::AutoAccept);
        assert!(
            platform
                .decide("test action", "test detail".to_string())
                .await
        );

        platform.set_approval_policy(ApprovalPolicy::Prompt);
        assert_eq!(platform.approval_policy(), ApprovalPolicy::Prompt);
    }

    /// A user id `validate_name` rejects must not leave the previous identity's
    /// storage mounted. `storage_user_id` falls back to `full_username`, a
    /// free-form People-chain display name, and `validate_name` rejects uppercase,
    /// spaces and non-ASCII — so `"Tarik Gul"` reaches this path.
    #[test]
    fn a_rejected_user_id_does_not_leave_the_previous_users_storage_mounted() {
        let dir = tempdir().expect("tempdir");
        let network_dir = dir.path().join("paseo");
        std::fs::create_dir_all(&network_dir).expect("network dir");

        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );

        // First identity: a valid lite username.
        platform
            .switch_pairing_user_storage("alice")
            .expect("switch to alice");
        let alice_dir = platform.state_dir().expect("alice state dir");
        assert!(
            alice_dir.ends_with("alice_pairing_host"),
            "unexpected dir: {}",
            alice_dir.display()
        );

        // Second identity arrives with a display name validate_name rejects. It
        // must still get its own namespace rather than inheriting alice's.
        platform
            .switch_pairing_user_storage("Tarik Gul")
            .expect("an unusable id must still isolate, not fail");

        let after = platform.state_dir().expect("state dir after");
        assert_ne!(
            after,
            alice_dir,
            "the rejected identity is still mounted on alice's storage at {}",
            after.display()
        );

        // The derived name is the hex digest of the raw id, and is itself a legal
        // session name so the persisted pointer survives a restart.
        let expected = format!("{}_pairing_host", pairing_storage_name("Tarik Gul"));
        assert!(
            after.ends_with(&expected),
            "expected a derived namespace, got {}",
            after.display()
        );
        assert!(crate::sessions::validate_name(&pairing_storage_name("Tarik Gul")).is_ok());
    }

    /// The same thing through the lifecycle the core actually drives:
    /// `AuthPresenter::auth_state_changed` with a `Connected` state. That path
    /// only logs a `warn!` on failure, so a rejected id used to silently inherit
    /// the previous identity's namespace.
    ///
    /// Asserts the isolation itself, not just that the paths differ: a value
    /// written while alice is connected must not be readable by the next
    /// identity.
    #[tokio::test]
    async fn auth_state_changed_does_not_inherit_storage_on_a_rejected_username() {
        use truapi::platform::AuthPresenter;

        let dir = tempdir().expect("tempdir");
        let network_dir = dir.path().join("paseo");
        std::fs::create_dir_all(&network_dir).expect("network dir");

        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir)),
            ApprovalPolicy::AutoAccept,
            None,
        );

        let connect = |lite: Option<&str>, full: Option<&str>| {
            AuthState::Connected(SessionUiInfo {
                lite_username: lite.map(str::to_string),
                full_username: full.map(str::to_string),
                ..SessionUiInfo::default()
            })
        };

        platform.auth_state_changed(connect(Some("alice"), None));
        let alice_dir = platform.state_dir().expect("alice state dir");
        assert!(alice_dir.ends_with("alice_pairing_host"));

        // Alice's product KV. `switch_pairing_user_storage` intentionally carries
        // the transient pairing keys (AuthSession, PairingDeviceIdentity,
        // LastProcessedPairingStatement) across namespaces, because the core
        // writes a session before it knows the username. Product KV is the state
        // the comment there promises stays with its own user, so that is what
        // isolation is asserted on.
        let alice_key =
            ProductStorageKey::new("demo-product.dot", "secret").expect("product storage key");
        platform
            .write(alice_key.encode(), b"alice-only".to_vec())
            .await
            .expect("alice product write");

        // No lite username, so storage_user_id falls back to full_username.
        platform.auth_state_changed(connect(None, Some("Tarik Gul")));

        let after = platform.state_dir().expect("state dir after");
        assert_ne!(
            after,
            alice_dir,
            "second identity is serving out of alice's storage at {}",
            after.display()
        );

        // The isolation that matters: alice's product KV is not visible.
        assert_eq!(
            platform.read(alice_key.encode()).await.expect("read"),
            None,
            "the second identity can read alice's product storage"
        );
    }

    /// One undecodable value in `core-storage.json` must not cost the host the
    /// entries it never touched. Before the file was moved aside, a real
    /// `CliPlatform` discarded `PairingDeviceIdentity` and then overwrote it on
    /// the next unrelated write.
    ///
    /// Keys on disk are hex of the SCALE-encoded `CoreStorageKey`, so
    /// `AuthSession` is `00` and `PairingDeviceIdentity` is `01`.
    #[tokio::test]
    async fn cli_platform_preserves_unreadable_core_storage() {
        use truapi::platform::CoreStorage;

        let dir = tempdir().expect("tempdir");
        let state_dir = dir.path().join("state");
        let product_dir = dir.path().join("products");
        std::fs::create_dir_all(&state_dir).expect("state dir");
        let core_path = state_dir.join("core-storage.json");

        std::fs::write(
            &core_path,
            r#"{"values":{"00":"cafebabe","01":"deadbeef","ff":"zzzz"}}"#,
        )
        .expect("seed core storage");

        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::new(state_dir, product_dir)),
            ApprovalPolicy::AutoAccept,
            None,
        );

        // Nothing loaded: the good entries went with the bad one.
        assert_eq!(
            platform
                .read_core_storage(CoreStorageKey::AuthSession)
                .await
                .expect("read"),
            None,
            "AuthSession should have loaded but the file was discarded"
        );
        assert_eq!(
            platform
                .read_core_storage(CoreStorageKey::PairingDeviceIdentity)
                .await
                .expect("read"),
            None
        );

        // The host writes one unrelated key. That persists the whole map.
        platform
            .write_core_storage(CoreStorageKey::AuthSession, vec![0x42])
            .await
            .expect("write");

        let after = std::fs::read_to_string(&core_path).expect("read back");
        assert!(
            after.contains("42"),
            "the new AuthSession value should be persisted: {after}"
        );

        // The unreadable original is preserved beside it, so nothing is lost.
        let preserved = std::fs::read_to_string(core_path.with_extension(UNREADABLE_SUFFIX))
            .expect("the unreadable file should have been moved aside");
        assert!(
            preserved.contains("deadbeef") && preserved.contains("cafebabe"),
            "the moved-aside file should still hold the original entries: {preserved}"
        );
    }

    /// `read_string_map` collects into a `Result`, so one undecodable value fails
    /// the whole file and the load yields nothing. That is survivable only
    /// because the file is moved aside first — otherwise the next write persists
    /// the empty map over it and every intact entry is gone.
    #[test]
    fn an_unreadable_storage_file_is_moved_aside_before_the_next_write() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("core-storage.json");

        // Two entries the host cares about, plus one whose value is not hex.
        let raw = r#"{"values":{
            "6175746853657373696f6e":"cafebabe",
            "70616972696e674964656e74697479":"deadbeef",
            "62726f6b656e":"zzzz"
        }}"#;
        std::fs::write(&path, raw).expect("seed");

        // The two good entries are readable in isolation, so nothing about them
        // is malformed — they are collateral.
        assert!(raw.contains("cafebabe") && raw.contains("deadbeef"));

        // Load: everything is dropped, not just the bad entry.
        let loaded = load_hex_key_map(&path);
        assert!(
            loaded.is_empty(),
            "expected the whole file to be discarded, got {} entries",
            loaded.len()
        );

        // A later write persists the empty map plus whatever is new. The original
        // is no longer at `path`, so this must not be able to destroy it.
        let mut next: HashMap<Vec<u8>, Vec<u8>> = loaded;
        next.insert(b"fresh".to_vec(), b"\x01".to_vec());
        save_hex_key_map(&path, &next).expect("save");

        let after = std::fs::read_to_string(&path).expect("read back");
        assert!(after.contains(&hex::encode(b"fresh")));

        let preserved = std::fs::read_to_string(path.with_extension(UNREADABLE_SUFFIX))
            .expect("the unreadable file should have been moved aside");
        assert!(
            preserved.contains("cafebabe") && preserved.contains("deadbeef"),
            "the moved-aside file should still hold the original entries: {preserved}"
        );
    }

    fn test_storage_paths(root: &Path, session: &str) -> CliStoragePaths {
        CliStoragePaths::new(root.to_path_buf(), root.join("storage").join(session))
    }

    #[test]
    fn connected_user_id_prefers_the_full_username() {
        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            full_username: Some("Alice".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_user_id(&info), Some("Alice"));

        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_user_id(&info), Some("alice.dot"));
        assert_eq!(connected_user_id(&SessionUiInfo::default()), None);
    }

    #[test]
    fn pairing_storage_switches_with_the_connected_username() {
        let temporary = tempdir().expect("create pairing storage root");
        let network_dir = temporary.path().join("testnet");
        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let product_key =
            ProductStorageKey::new("product.dot", "theme").expect("product storage key");

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("select alice");
        futures::executor::block_on(platform.write(product_key.encode(), b"dark".to_vec()))
            .expect("write alice product value");

        platform
            .switch_pairing_user_storage("bob.dot")
            .expect("select bob");
        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read bob product value"),
            None
        );

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("restore alice");
        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read alice product value"),
            Some(b"dark".to_vec())
        );
        assert_eq!(
            platform.state_dir().as_deref(),
            Some(network_dir.join("alice.dot_pairing_host").as_path())
        );
        assert_eq!(
            read_current_pairing_user(&network_dir.join("pairing-host")).as_deref(),
            Some("alice.dot")
        );
    }

    #[test]
    fn device_encryption_key_outlives_pairing_user_switches() {
        let temporary = tempdir().expect("create pairing storage root");
        let network_dir = temporary.path().join("testnet");
        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("select alice");
        futures::executor::block_on(
            platform.write_core_storage(CoreStorageKey::DeviceEncryptionKey, vec![7; 32]),
        )
        .expect("write device key");

        // Peers address this install by the matching public key, so switching
        // users must not strand them on a regenerated one.
        platform
            .switch_pairing_user_storage("bob.dot")
            .expect("select bob");
        assert_eq!(
            futures::executor::block_on(
                platform.read_core_storage(CoreStorageKey::DeviceEncryptionKey)
            )
            .expect("read device key as bob"),
            Some(vec![7; 32])
        );

        // A user-scoped slot stays isolated, so the routing is not simply
        // making every slot global.
        futures::executor::block_on(
            platform.write_core_storage(CoreStorageKey::AutoSigningKeys, vec![1, 2, 3]),
        )
        .expect("write bob auto-signing keys");
        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("restore alice");
        assert_eq!(
            futures::executor::block_on(
                platform.read_core_storage(CoreStorageKey::AutoSigningKeys)
            )
            .expect("read alice auto-signing keys"),
            None
        );

        // It also survives a fresh process reading the same directories.
        let restarted = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir)),
            ApprovalPolicy::AutoAccept,
            None,
        );
        assert_eq!(
            futures::executor::block_on(
                restarted.read_core_storage(CoreStorageKey::DeviceEncryptionKey)
            )
            .expect("read device key after restart"),
            Some(vec![7; 32])
        );
    }

    /// A pairing login writes product KV before its username is known, so the
    /// bootstrap directory's products must follow the first resolved user
    /// instead of being stranded outside every identity namespace.
    #[test]
    fn product_storage_written_before_the_username_carries_into_the_resolved_user() {
        let temporary = tempdir().expect("create pairing storage root");
        let network_dir = temporary.path().join("testnet");
        let product_key =
            ProductStorageKey::new("product.dot", "theme").expect("product storage key");
        save_product_storage(
            &network_dir.join("pairing-host/storage"),
            "product.dot",
            &HashMap::from([("theme".to_string(), b"dark".to_vec())]),
        )
        .expect("write bootstrap pairing product storage");
        let platform = CliPlatform::new(
            test_network(),
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("resolve the bootstrap storage owner");

        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read carried product value"),
            Some(b"dark".to_vec())
        );
        // Persisted, not merely carried in memory: a restart must find it too.
        assert_eq!(
            load_product_storage(&network_dir.join("alice.dot_pairing_host/storage")),
            HashMap::from([(
                "product.dot".to_string(),
                HashMap::from([("theme".to_string(), b"dark".to_vec())]),
            )])
        );
    }

    #[test]
    fn approval_summaries_are_concise_and_do_not_dump_payloads() {
        let review =
            UserConfirmationReview::PreimageSubmit(truapi::platform::PreimageSubmitReview {
                size: 4_096,
            });

        let (action, detail) = approval_summary(&review);

        assert_eq!(action, "submit preimage");
        assert_eq!(
            detail,
            "A product requested submission of a 4096-byte preimage."
        );
        assert!(!detail.contains("["));
    }

    /// Both products by name, because a signature made with an account the
    /// caller does not own is the thing the user has to be able to see.
    #[test]
    fn statement_proof_approval_names_both_products_without_dumping_payload() {
        let review = UserConfirmationReview::StatementStoreProductSign(
            truapi::platform::StatementStoreProductSignReview {
                calling_product_id: Some("dim2next.paseo".to_string()),
                account: api::ProductAccountId {
                    dot_ns_identifier: "dim2.paseo".to_string(),
                    derivation_index: api::DerivationIndex::Index(0),
                },
                payload: vec![0x42; 128],
            },
        );

        let (action, detail) = approval_summary(&review);

        assert_eq!(action, "sign statement proof");
        assert_eq!(
            detail,
            "Product dim2next.paseo requested a Statement Store proof signature for the \
             dim2.paseo account over a 128-byte payload."
        );
        assert!(!detail.contains("[66"));
    }

    #[test]
    fn vrf_approval_names_both_products_without_dumping_transcript_values() {
        let review = UserConfirmationReview::SignVrf(truapi::platform::SignVrfReview {
            calling_product_id: "caller.dot".to_string(),
            request: truapi::v01::HostAccountSignVrfRequest {
                account: truapi::v01::ProductAccountId {
                    dot_ns_identifier: "target.dot".to_string(),
                    derivation_index: truapi::v01::DerivationIndex::Raw([7; 32]),
                },
                transcript_label: b"lottery".to_vec(),
                items: vec![truapi::v01::VrfTranscriptItem {
                    label: b"round".to_vec(),
                    value: vec![0x42; 128],
                }],
            },
        });

        let (action, detail) = approval_summary(&review);

        assert_eq!(action, "sign VRF transcript");
        assert_eq!(
            detail,
            "Product caller.dot requested a VRF signature for the target.dot account over 1 transcript items."
        );
        assert!(!detail.contains("[66"));
    }

    #[test]
    fn cli_notifications_return_stable_ids_and_cancel_idempotently() {
        let platform = CliPlatform::new(test_network(), None, ApprovalPolicy::AutoAccept, None);
        let first = futures::executor::block_on(platform.push_notification(
            api::HostPushNotificationRequest {
                text: "Hello".to_string(),
                deeplink: None,
                scheduled_at: None,
            },
        ))
        .expect("immediate notification");
        let second = futures::executor::block_on(platform.push_notification(
            api::HostPushNotificationRequest {
                text: "Again".to_string(),
                deeplink: Some("polkadot://example".to_string()),
                scheduled_at: None,
            },
        ))
        .expect("second notification");

        assert_eq!(first.id, 1);
        assert_eq!(second.id, 2);
        futures::executor::block_on(platform.cancel_notification(first.id))
            .expect("already-fired cancellation is idempotent");
        futures::executor::block_on(platform.cancel_notification(999))
            .expect("unknown cancellation is idempotent");
    }

    #[test]
    fn product_storage_uses_safe_per_product_files() {
        let temporary = tempdir().expect("create product storage root");
        let first = ProductStorageKey::new("first.dot", "theme").expect("first product key");
        let localhost =
            ProductStorageKey::new("localhost:3000", "theme").expect("localhost product key");
        let platform = CliPlatform::new(
            test_network(),
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        futures::executor::block_on(async {
            platform
                .write(first.encode(), b"dark".to_vec())
                .await
                .expect("write first product");
            platform
                .write(localhost.encode(), b"light".to_vec())
                .await
                .expect("write localhost product");
        });

        let directory = temporary.path().join("storage").join("test");
        let files = fs::read_dir(&directory)
            .expect("list product storage")
            .map(|entry| entry.expect("product storage entry").path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|path| {
            path.parent() == Some(directory.as_path())
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(':'))
        }));

        drop(platform);
        let restored = CliPlatform::new(
            test_network(),
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let (first_value, localhost_value) = futures::executor::block_on(async {
            (
                restored.read(first.encode()).await.expect("read first"),
                restored
                    .read(localhost.encode())
                    .await
                    .expect("read localhost"),
            )
        });
        assert_eq!(first_value, Some(b"dark".to_vec()));
        assert_eq!(localhost_value, Some(b"light".to_vec()));
    }

    #[test]
    fn product_storage_is_isolated_by_session_then_product() {
        let temporary = tempdir().expect("create session storage root");
        let key = ProductStorageKey::new("same.dot", "value").expect("product key");
        let first = CliPlatform::new(
            test_network(),
            Some(test_storage_paths(temporary.path(), "first")),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let second = CliPlatform::new(
            test_network(),
            Some(test_storage_paths(temporary.path(), "second")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        futures::executor::block_on(async {
            first
                .write(key.encode(), b"one".to_vec())
                .await
                .expect("write first session");
            second
                .write(key.encode(), b"two".to_vec())
                .await
                .expect("write second session");
        });

        let first_path =
            product_storage_path(&temporary.path().join("storage").join("first"), "same.dot");
        let second_path =
            product_storage_path(&temporary.path().join("storage").join("second"), "same.dot");
        assert!(first_path.is_file());
        assert!(second_path.is_file());
        assert_ne!(
            fs::read_to_string(first_path).expect("read first session file"),
            fs::read_to_string(second_path).expect("read second session file")
        );
    }
}
