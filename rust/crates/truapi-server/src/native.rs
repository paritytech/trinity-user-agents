//! UniFFI-facing native bridge. Exposes [`NativeTrUApiHostRuntime`],
//! [`NativeProductExecution`], and the [`HostCallbacks`] callback interface
//! that iOS and Android call into.
//!
//! The native side builds a `CallbackPlatform` that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`SigningHostRuntime`] so the rest of the
//! dispatcher pipeline behaves identically to the WS-bridge and wasm flavors.
//! A native host therefore owns the signer: there is no pairing flow here, and
//! the pairing-host-only entry points are inert.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use parity_scale_codec::Encode;
use truapi::{Bytes32, latest::HostPlatform, v01};
use truapi_platform::{
    AuthPresenter, AuthState, ChainProvider, CoinageWalletHost, CoreAdmin, CoreStorage,
    CoreStorageKey, Features, HopProvider, HostInfo, JsonRpcConnection, LocaleHost,
    NativeChatFileExportRequest, NativeChatFilePickRequest, NativeChatFilesHost,
    NativeChatPickedFile, NativeCoinageRequest, NativeCoinageResponse, Navigation, Notifications,
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, Permissions, PlatformInfo,
    PreimageHost, ProductContext, ProductExecutionKind, ProductStorage,
    RuntimeConfigValidationError, SigningHostConfig, ThemeHost, UserConfirmation,
    UserConfirmationReview, async_trait, normalize_product_identifier,
};

use crate::SigningHostRuntime;
use crate::host_logic::dotns;
pub use crate::host_logic::dotns::{NavigateDecision, PocketDeeplinkAction};
use crate::host_logic::sso::messages::{
    RemoteMessage, RemoteMessageData, SsoRequestOutcome as CoreSsoRequestOutcome,
    decode_remote_message, v1,
};
use crate::host_logic::worker::WorkerTransition;
#[cfg(feature = "ws-bridge")]
use crate::native_renderer::observe_renderer;
use crate::native_renderer::{NativeRendererObserver, NativeRendererSubscription};
use crate::runtime::sso_remote::sso_message_id;
use crate::subscription::Spawner;
#[cfg(feature = "ws-bridge")]
use crate::ws_bridge::{BridgeLogger, WsBridge, WsBridgeEndpoint, WsBridgeStartError};

/// Host-thrown storage failure wrapping the canonical error payload, so its
/// variants remain defined once in `truapi`.
///
/// [UniFFI 0.32 exposes `Result` failures as error enums or `Arc`-backed error
/// objects](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html). Although
/// the canonical enum can be exposed as an external error, Kotlin foreign-trait
/// callbacks must lower thrown errors into this namespace's `RustBuffer`; the
/// external converter returns the canonical namespace's distinct `RustBuffer`
/// type. There is no derive-based bridge between them. `uniffi::remote(Error)`
/// would instead duplicate every canonical variant and field, so this local
/// one-variant wrapper preserves the canonical definition.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostStorageError {
    /// Canonical storage failure payload.
    #[error("{0}")]
    Storage(v01::HostLocalStorageReadError),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostStorageError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        HostStorageError::Storage(v01::HostLocalStorageReadError::Unknown { reason: err.reason })
    }
}

impl From<HostStorageError> for v01::HostLocalStorageReadError {
    fn from(err: HostStorageError) -> Self {
        let HostStorageError::Storage(err) = err;
        err
    }
}

/// Native-friendly rejection error returned by callback methods that map onto
/// [`truapi::v01::GenericError`].
///
/// [`uniffi::Error` is the value-style error mapping and only supports enums;
/// UniFFI's struct alternative is an `Arc`-backed object
/// error](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html). Making the
/// canonical SCALE value an object would require Rust-owned handles and foreign
/// construction solely to carry one string. This local enum keeps the native
/// exception value-like without changing the canonical wire representation.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostRejection {
    /// Caller rejected the operation.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl From<HostRejection> for v01::GenericError {
    fn from(err: HostRejection) -> Self {
        let HostRejection::Rejected { reason } = err;
        v01::GenericError { reason }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostRejection {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        HostRejection::Rejected { reason: err.reason }
    }
}

impl From<v01::GenericError> for HostRejection {
    fn from(err: v01::GenericError) -> Self {
        HostRejection::Rejected { reason: err.reason }
    }
}

/// Host-thrown navigation failure wrapping the canonical error payload.
///
/// As described for [`HostStorageError`], [UniFFI's supported error
/// representations](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html)
/// do not provide a derive-based way to bridge namespace-specific Kotlin
/// `RustBuffer` types when an external error is thrown by a foreign-trait
/// callback. The one-variant wrapper avoids mirroring the canonical navigation
/// error enum locally.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostNavigateRejection {
    /// Canonical navigation failure payload.
    #[error("{0}")]
    Navigate(v01::HostNavigateToError),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostNavigateRejection {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        HostNavigateRejection::Navigate(v01::HostNavigateToError::Unknown { reason: err.reason })
    }
}

/// FFI projection of the canonical
/// [`SsoRequestOutcome`](crate::host_logic::sso::messages::SsoRequestOutcome),
/// concrete because UniFFI cannot export generics.
///
/// Variants carry SCALE-encoded wire bytes rather than decoded Rust types because
/// the wallet forwards encodings verbatim and never constructs them — the opaque
/// bytes are the correct boundary representation here.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SsoRequestOutcome {
    /// SCALE-encoded response to post back over the session.
    Response {
        /// SCALE-encoded `RemoteMessage` response ready to submit over the
        /// session statement store.
        message: Vec<u8>,
    },
    /// The peer ended the session; the wallet tears down its transport and
    /// records (host entry, device record, device-removed broadcast).
    Disconnected,
    /// Not a request; nothing to post.
    Ignored,
}

/// Process-owned native host configuration shared by every product execution.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeHostRuntimeConfig {
    /// Host name shown by the wallet during SSO pairing.
    pub host_name: String,
    /// Optional host icon URL shown by the wallet during SSO pairing.
    pub host_icon: Option<String>,
    /// Optional host version shown by the wallet during SSO pairing.
    pub host_version: Option<String>,
    /// Platform category this host runs on, reported to products via
    /// `System::host_info`.
    pub host_platform: HostPlatform,
    /// Optional platform/browser name shown by the wallet during SSO pairing.
    pub platform_type: Option<String>,
    /// Optional platform/browser version shown by the wallet during SSO pairing.
    pub platform_version: Option<String>,
    /// People-chain genesis hash. Must be exactly 32 bytes.
    pub people_chain_genesis_hash: Vec<u8>,
    /// Bulletin-chain genesis hash. Must be exactly 32 bytes.
    pub bulletin_chain_genesis_hash: Vec<u8>,
    /// The network's dotNS TLD without the leading dot (`dot`, `paseo`,
    /// `testnet`). The wallet's reserved identities are derived under it:
    /// `uid.<suffix>` for the identity account, `peopl.<suffix>` for the person
    /// ring-VRF keys. Read it from the network the host is configured for, the
    /// way the host's own onboarding does; a wrong value derives a different
    /// person from the same seed.
    pub network_suffix: String,
    /// Optional local signing-host secret material (raw BIP-39 entropy).
    pub local_session_secret: Option<Vec<u8>>,
    /// Optional lite username attached to the local signing-host session.
    pub local_session_lite_username: Option<String>,
    /// Asset Hub genesis hash, where the dotNS contracts are deployed. Must be
    /// exactly 32 bytes.
    ///
    /// Product manifests are read from dotNS, so this is what makes a
    /// `trustedProducts` grant resolvable. 32 zero bytes says this host has no
    /// Asset Hub; grants already in the manifest cache stay honoured until they
    /// expire.
    ///
    /// Appended rather than placed with its sibling hashes: record fields are
    /// positional over the FFI and the checksum does not cover their order, so
    /// an insert shifts every field below it.
    pub asset_hub_chain_genesis_hash: Vec<u8>,
    /// Trusted Coinage asset instance from the wallet's network configuration.
    /// Required for instance-scoped Coinage runtimes; omit only for legacy use.
    /// Appended to preserve the order of existing positional FFI record fields.
    pub coinage_instance_id: Option<u32>,
}

/// Trusted identity attached by a native host to one executable connection.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeProductExecutionConfig {
    /// Canonical product identifier used for policy, storage, and derivation.
    pub product_id: String,
    /// Trusted executable kind selected before product code starts.
    pub execution_kind: ProductExecutionKind,
}

#[derive(Debug)]
struct NativeResolvedHostRuntimeConfig {
    signing: SigningHostConfig,
    local_session_secret: Option<Vec<u8>>,
    local_session_lite_username: Option<String>,
}

/// Native runtime config validation error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeRuntimeConfigError {
    /// Required string field was empty or whitespace-only.
    #[error("{field} must not be empty")]
    EmptyField {
        /// Field name.
        field: String,
    },
    /// People-chain genesis hash was not exactly 32 bytes.
    #[error("people_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidPeopleChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Bulletin-chain genesis hash was not exactly 32 bytes.
    #[error("bulletin_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidBulletinChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Host icon URL could not be parsed.
    #[error("host_icon must be an absolute HTTPS URL: {reason}")]
    InvalidHostIcon {
        /// Parse failure reason.
        reason: String,
    },
    /// Host icon URL used a non-HTTPS scheme.
    #[error("host_icon must use https scheme, got {scheme:?}")]
    InsecureHostIcon {
        /// Actual URL scheme.
        scheme: String,
    },
    /// Pairing deeplink scheme included a URL separator.
    #[error("pairing_deeplink_scheme must not include ://, got {scheme:?}")]
    InvalidDeeplinkScheme {
        /// Actual deeplink scheme value.
        scheme: String,
    },
    /// Network suffix was not a supported dotNS TLD.
    #[error("network_suffix must be a supported dotNS TLD, got {network_suffix:?}")]
    InvalidNetworkSuffix {
        /// Actual network suffix value.
        network_suffix: String,
    },
    /// Product id was not a valid host-spec product identifier.
    #[error("invalid product_id: {product_id}")]
    InvalidProductId {
        /// Actual product id value.
        product_id: String,
    },
    /// Local signing-host session activation failed.
    #[error("failed to activate local signing session: {reason}")]
    LocalSessionActivation {
        /// Activation failure reason.
        reason: String,
    },
    /// Asset Hub genesis hash was not exactly 32 bytes.
    ///
    /// Appended, not grouped with the sibling genesis-hash variants: declaration
    /// order is the FFI discriminant and the checksum does not cover it, so an
    /// insert renumbers every variant below it.
    #[error("asset_hub_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidAssetHubChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Product id was longer than `PRODUCT_ID_MAX_BYTES` after normalization.
    ///
    /// Appended for the same reason as the variant above. Carries lengths and
    /// not the id: an id that trips this is unbounded in size, and this error
    /// reaches the wire and the logs.
    #[error("product_id must be at most {limit} bytes, got {actual}")]
    ProductIdTooLong {
        /// Accepted maximum, in bytes.
        limit: u64,
        /// Normalized length, in bytes.
        actual: u64,
    },
}

impl TryFrom<NativeHostRuntimeConfig> for NativeResolvedHostRuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeHostRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            <[u8; 32]>::try_from(config.people_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidPeopleChainGenesisHash {
                    actual: config.people_chain_genesis_hash.len() as u64,
                }
            })?;
        let bulletin_chain_genesis_hash =
            <[u8; 32]>::try_from(config.bulletin_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidBulletinChainGenesisHash {
                    actual: config.bulletin_chain_genesis_hash.len() as u64,
                }
            })?;
        let asset_hub_chain_genesis_hash =
            <[u8; 32]>::try_from(config.asset_hub_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidAssetHubChainGenesisHash {
                    actual: config.asset_hub_chain_genesis_hash.len() as u64,
                }
            })?;
        let mut signing = SigningHostConfig::new(
            HostInfo {
                name: config.host_name,
                icon: config.host_icon,
                version: config.host_version,
                platform: config.host_platform,
            },
            PlatformInfo {
                kind: config.platform_type,
                version: config.platform_version,
            },
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
            asset_hub_chain_genesis_hash,
            config.network_suffix,
        )?;
        signing.coinage_instance_id = config.coinage_instance_id;
        Ok(Self {
            signing,
            local_session_secret: config.local_session_secret,
            local_session_lite_username: config.local_session_lite_username,
        })
    }
}

impl TryFrom<NativeProductExecutionConfig> for ProductContext {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeProductExecutionConfig) -> Result<Self, Self::Error> {
        ProductContext::new_with_execution(config.product_id, config.execution_kind)
            .map_err(NativeRuntimeConfigError::from)
    }
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        match err {
            RuntimeConfigValidationError::EmptyField { field } => Self::EmptyField {
                field: field.to_string(),
            },
            // `url::ParseError` cannot cross the UniFFI boundary, so the native
            // error keeps a rendered string.
            RuntimeConfigValidationError::InvalidHostIcon { source } => Self::InvalidHostIcon {
                reason: source.to_string(),
            },
            RuntimeConfigValidationError::InsecureHostIcon { scheme } => {
                Self::InsecureHostIcon { scheme }
            }
            RuntimeConfigValidationError::InvalidDeeplinkScheme { scheme } => {
                Self::InvalidDeeplinkScheme { scheme }
            }
            RuntimeConfigValidationError::InvalidProductId { product_id } => {
                Self::InvalidProductId { product_id }
            }
            RuntimeConfigValidationError::ProductIdTooLong { limit, actual } => {
                Self::ProductIdTooLong {
                    limit: limit as u64,
                    actual: actual as u64,
                }
            }
            RuntimeConfigValidationError::InvalidNetworkSuffix { network_suffix } => {
                Self::InvalidNetworkSuffix { network_suffix }
            }
        }
    }
}

impl From<HostNavigateRejection> for v01::HostNavigateToError {
    fn from(err: HostNavigateRejection) -> Self {
        let HostNavigateRejection::Navigate(err) = err;
        err
    }
}

/// Classify a navigation input exactly like the core's internal navigate host
/// call: dotNS first, then `localhost`, then normalized external, with
/// everything else rejected. Pure and stateless; hosts call it on every
/// webview-internal navigation.
#[uniffi::export]
pub fn parse_navigate(input: String) -> NavigateDecision {
    dotns::parse_navigate(&input)
}

/// Strictly decoded storage metadata for host-private namespace routing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeCoreStorageKeyDescription {
    /// Stable core storage slot kind.
    pub kind: String,
    /// Product owning this slot, absent for wallet-owned state.
    pub product_id: Option<String>,
}

/// Describe exactly one encoded storage key without duplicating SCALE in hosts.
#[uniffi::export]
pub fn native_describe_core_storage_key(
    encoded: Vec<u8>,
) -> Result<NativeCoreStorageKeyDescription, HostRejection> {
    let description = truapi_platform::describe_core_storage_key(&encoded).map_err(|error| {
        HostRejection::Rejected {
            reason: error.to_string(),
        }
    })?;
    Ok(NativeCoreStorageKeyDescription {
        kind: description.kind.to_owned(),
        product_id: description.product_id,
    })
}

/// Whether `product_id` is a first-party product the host grants every
/// [`truapi::latest::RemotePermission`] without prompting.
///
/// Pure and stateless: it reads the compiled-in list and nothing else. **A
/// stored user decision wins over the list**, so this is only the answer for
/// the branch where the host's own store reads undetermined. Consulting it
/// first would let a revoked grant keep working.
///
/// [`NativeProductExecution::permission_authorization_status`] is the stateful
/// answer — it folds the list and the stored decision together — and a host
/// holding an execution should ask that instead.
///
/// This exists for the path where a host mediates product network access in its
/// own code — a webview interceptor, a `fetch` shim — and has already found
/// nothing stored. Without it a first-party product is prompted by the host for
/// access the core would have granted.
///
/// Covers remote permissions only. Device capabilities, identity disclosure and
/// cross-product account access always prompt, whoever asks.
///
/// Normalizes before matching, and answers `false` for an id that does not
/// normalize, so an unknown spelling is never read as trusted.
#[uniffi::export]
pub fn has_trusted_remote_permissions(product_id: String) -> bool {
    truapi_platform::normalizes_to_trusted_remote_permissions(&product_id)
}

/// OS status of a device capability, as a native host reports it.
///
/// Mirrors [`truapi_platform::DevicePermissionStatus`], which cannot be used
/// directly: an async callback method returning a type from another UniFFI
/// namespace lowers into that namespace's `RustBuffer`, and the generated
/// Kotlin then fails to compile. The conversion is total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativeDevicePermissionStatus {
    /// The OS grants this capability to the host application.
    Granted,
    /// The OS refuses it; only system settings can change that.
    Denied,
    /// The OS has not been asked, or reset its own answer.
    NotDetermined,
    /// This platform has no OS gate for the capability.
    NotApplicable,
}

impl From<NativeDevicePermissionStatus> for truapi_platform::DevicePermissionStatus {
    fn from(status: NativeDevicePermissionStatus) -> Self {
        match status {
            NativeDevicePermissionStatus::Granted => Self::Granted,
            NativeDevicePermissionStatus::Denied => Self::Denied,
            NativeDevicePermissionStatus::NotDetermined => Self::NotDetermined,
            NativeDevicePermissionStatus::NotApplicable => Self::NotApplicable,
        }
    }
}

/// UniFFI-local callback envelope: Kotlin async returns must lower into this
/// namespace's RustBuffer, not the platform namespace's distinct RustBuffer.
/// The nested field retains the canonical platform type without mirroring it.
/// Do not derive Debug: a prepared response can contain bearer memo material.
#[derive(Clone, uniffi::Record)]
pub struct NativeCoinageCallbackResult {
    /// Typed result, including Host-private memo material when preparing a payment.
    pub response: NativeCoinageResponse,
}

/// Optional process-wide native custody, installed only at runtime construction.
/// Absence selects built-in Rust custody. A registered wallet's failure or
/// unavailability never changes that dependency or permits fallback.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeCoinageCallbacks: Send + Sync {
    /// Host-private wallet operation; bearer material must never reach a product.
    async fn native_coinage(
        &self,
        request: NativeCoinageRequest,
    ) -> Result<NativeCoinageCallbackResult, HostRejection>;
}

/// Callback surface that iOS and Android implement.
///
/// Threading contract: every callback executes on the shared bridge
/// executor's worker threads, and blocking one of those threads can stall
/// the entire bridge — not just the request being served. Async callbacks
/// (`navigate_to`, `push_notification`, `device_permission`,
/// `remote_permission`, `feature_supported`, `confirm_user_action`,
/// `lookup_preimage`) are awaited by the core — implementations hop to the
/// main thread for any UI and may keep the future pending arbitrarily long,
/// but must suspend rather than block the polling thread (foreign
/// implementations bridged through UniFFI suspend naturally; the rule
/// chiefly binds Rust implementations). Dropping the returned future
/// cancels the foreign task. The remaining sync callbacks run inline on the
/// dispatcher thread and must return promptly without blocking; in
/// particular `auth_state_changed` should only hand the state to the host
/// UI thread, never wait for the user.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait HostCallbacks: Send + Sync {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification.
    async fn push_notification(
        &self,
        request: v01::HostPushNotificationRequest,
    ) -> Result<u32, HostRejection>;

    /// Cancel a notification by id.
    fn cancel_notification(&self, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...);
    /// the host returns whether the permission was granted.
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<bool, HostRejection>;

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
    ) -> Result<NativeDevicePermissionStatus, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission.
    async fn remote_permission(
        &self,
        request: v01::RemotePermission,
    ) -> Result<bool, HostRejection>;

    /// Observe an auth state change, in transition order: render `Pairing` as
    /// the pairing QR UI, `Connected`/`Disconnected` as the account badge,
    /// `LoginFailed` as a retryable error unless its `kind` is
    /// `NoFreeAllowanceSlots`, which is unlikely to succeed before the period
    /// rolls over, so retry should not be the primary action. A pairing host
    /// always receives an opening state once the core has restored the
    /// persisted session, `Disconnected` included; later emissions happen
    /// only when the state changes.
    fn auth_state_changed(&self, state: AuthState);

    /// Read a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_read(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Persist a core-owned host-private storage slot. `key` is a
    /// SCALE-encoded [`CoreStorageKey`].
    fn core_storage_write(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Clear a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_clear(&self, key: Vec<u8>) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Current trusted HOP WSS URLs for the configured Bulletin chain.
    /// An embedding with no HOP configuration returns an empty allowlist.
    async fn allowed_hop_endpoints(
        &self,
        bulletin_genesis_hash: Vec<u8>,
    ) -> Result<Vec<String>, HostRejection>;

    /// Open only an exact endpoint from the current trusted Bulletin allowlist.
    /// HOP ids share the chain send/close and response/closed event namespace.
    fn hop_connect(
        &self,
        bulletin_genesis_hash: Vec<u8>,
        endpoint: String,
    ) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm one user-reviewed core action.
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, HostRejection>;

    /// Trusted file selection into durable immutable Host custody.
    async fn pick_chat_files(
        &self,
        request: NativeChatFilePickRequest,
    ) -> Result<Vec<NativeChatPickedFile>, HostRejection>;

    /// Exact bounded read from a Host-private immutable source.
    async fn read_chat_file(
        &self,
        source_id: String,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, HostRejection>;

    /// Idempotently release a durable private source.
    async fn release_chat_file(&self, source_id: String) -> Result<(), HostRejection>;

    /// Trusted export consent; `None` means user cancellation.
    async fn begin_chat_file_export(
        &self,
        request: NativeChatFileExportRequest,
    ) -> Result<Option<String>, HostRejection>;

    /// Contiguous bounded write into a partial private export.
    async fn write_chat_file_export(
        &self,
        export_id: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), HostRejection>;

    /// Publish an exact-size export through safe native UI.
    async fn finish_chat_file_export(&self, export_id: String) -> Result<(), HostRejection>;

    /// Idempotently discard a partial export, never a completed user export.
    async fn cancel_chat_file_export(&self, export_id: String) -> Result<(), HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Exact-name AccountId32 candidates from the host's configured,
    /// authenticated native username service. Every item must contain 32 bytes;
    /// the core verifies finalized dotNS ownership and the canonical People key.
    async fn identity_username_candidates(
        &self,
        username: String,
        people_chain_genesis_hash: Vec<u8>,
    ) -> Result<Vec<Vec<u8>>, HostRejection>;

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
    fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, HostRejection>;

    /// Observe demand on a product's worker crossing zero. `Start` means the
    /// host runs the worker now, `Stop` that nothing wants it any more. Every
    /// transition arrives here, in ledger order: the ones the host asks for
    /// through [`NativeTrUApiHostRuntime::acquire_worker`] and
    /// [`NativeTrUApiHostRuntime::release_worker`], and the ones the core
    /// causes on a product's behalf, such as an open render stream.
    ///
    /// Demand is runtime-wide, so this is invoked only on the callbacks the
    /// runtime was built with, never on the per-execution callbacks passed to
    /// [`NativeTrUApiHostRuntime::open_product_execution`]. Can arrive on any
    /// thread, including synchronously on the calling thread during
    /// [`NativeTrUApiHostRuntime::acquire_worker`] and
    /// [`NativeTrUApiHostRuntime::release_worker`], often the caller's own
    /// thread and re-entrantly: hand the transition off rather than blocking
    /// on another thread from inside it.
    fn worker_demand_changed(&self, product_id: String, transition: WorkerTransition);

    /// Read a value from the host's scoped key-value store.
    fn local_storage_read(&self, key: String) -> Result<Option<Vec<u8>>, HostStorageError>;
    /// Write a value to the host's scoped key-value store.
    fn local_storage_write(&self, key: String, value: Vec<u8>) -> Result<(), HostStorageError>;
    /// Clear a value from the host's scoped key-value store.
    fn local_storage_clear(&self, key: String) -> Result<(), HostStorageError>;
}

/// Native Chat storage and UI adapter. Hosts that support the Chat modality
/// pass an implementation to
/// [`NativeTrUApiHostRuntime::open_product_execution`]; hosts that do not
/// simply pass `None`. Callbacks run inline on the process-wide dispatch pool
/// shared by every product execution, so one that blocks stalls the others.
#[uniffi::export(rust, foreign)]
pub trait NativeChatCallbacks: Send + Sync {
    /// Create or resolve a native product Chat room.
    fn create_room(
        &self,
        room_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatRoomRegistrationStatus, HostRejection>;

    /// Register or resolve a native product Chat bot.
    fn register_bot(
        &self,
        bot_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatBotRegistrationStatus, HostRejection>;

    /// Persist a product-authored message in native Chat storage. A host that
    /// cannot render a given content variant returns a rejection for it.
    ///
    /// The returned id is what [`ActionTrigger::message_id`] carries back, so
    /// it must name this message for as long as the host stores it.
    ///
    /// [`ActionTrigger::message_id`]: truapi::latest::ActionTrigger
    fn post_message(
        &self,
        room_id: String,
        content: v01::ChatMessageContent,
    ) -> Result<String, HostRejection>;

    /// Return the current product-scoped native Chat room list.
    fn list_rooms(&self) -> Result<Vec<v01::ChatRoom>, HostRejection>;
}

/// Native Pocket collection adapter. Hosts with a Pocket surface pass an
/// implementation to [`NativeTrUApiHostRuntime::open_product_execution`]; hosts
/// without one pass `None`. Callbacks run inline on the process-wide dispatch
/// pool shared by every product execution, so one that blocks stalls the
/// others.
///
/// The host decides a removal and reports what it did, so the check and the
/// removal happen together under whatever lock it holds. A card cannot be
/// pinned between the two.
#[uniffi::export(rust, foreign)]
pub trait NativePocketCallbacks: Send + Sync {
    /// Return the product's cards as this host currently holds them, each with
    /// the flag saying whether the host pinned it.
    fn list_cards(&self) -> Result<Vec<v01::PocketCard>, HostRejection>;

    /// Remove one of the product's cards, reporting whether the card was
    /// taken out, was already gone, or is pinned and stays.
    fn remove_card(&self, card_id: String) -> Result<NativePocketRemoval, HostRejection>;
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

/// Process-owned native TrUAPI runtime shared by all executable connections.
#[derive(uniffi::Object)]
pub struct NativeTrUApiHostRuntime {
    runtime: Arc<SigningHostRuntime>,
    events: Arc<NativeEventBus>,
    #[cfg(feature = "ws-bridge")]
    spawner: Spawner,
    /// The one Worker execution per product; opening another replaces it.
    worker_executions: Mutex<HashMap<String, Weak<NativeProductExecution>>>,
}

impl NativeTrUApiHostRuntime {
    fn from_resolved(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: NativeResolvedHostRuntimeConfig,
        native_wallet: Option<Arc<dyn NativeCoinageCallbacks>>,
        log_marker: &str,
        log_detail: &str,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        crate::logging::init();
        callbacks.on_core_log(log_marker.to_string(), log_detail.to_string());
        let events = Arc::new(NativeEventBus::default());
        let platform = Arc::new(CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
        });
        let spawner = native_thread_pool_spawner(&callbacks);
        let native_wallet = native_wallet.map(|callbacks| -> Arc<dyn CoinageWalletHost> {
            Arc::new(NativeCoinageCallbackPlatform { callbacks })
        });
        let runtime = Arc::new(SigningHostRuntime::with_chat_platform(
            platform.clone(),
            runtime_config.signing,
            spawner.clone(),
            None,
            native_wallet,
        ));
        runtime.set_identity_backend_host(platform.clone());
        assert!(
            runtime.worker_ledger().install_demand_observer(platform),
            "a freshly built runtime installs its worker demand observer once"
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
            #[cfg(feature = "ws-bridge")]
            spawner,
            worker_executions: Mutex::new(HashMap::new()),
        }))
    }

    fn open_product_execution_with_callbacks(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
        pocket_callbacks: Option<Arc<dyn NativePocketCallbacks>>,
        product: ProductContext,
    ) -> Arc<NativeProductExecution> {
        let events = Arc::new(NativeEventBus::default());
        let callback_platform = Arc::new(CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
        });
        let permission_status: Arc<dyn truapi_platform::PermissionStatusHost> =
            callback_platform.clone();
        let platform: Arc<dyn truapi_platform::Platform> = callback_platform;
        let chat: Option<Arc<dyn truapi_platform::ChatPlatform>> =
            chat_callbacks.map(|chat| -> Arc<dyn truapi_platform::ChatPlatform> {
                Arc::new(ChatCallbackPlatform {
                    chat,
                    events: events.clone(),
                })
            });
        let pocket: Option<Arc<dyn truapi_platform::PocketPlatform>> =
            pocket_callbacks.map(|pocket| -> Arc<dyn truapi_platform::PocketPlatform> {
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
            events,
            shared_events: self.events.clone(),
            #[cfg(feature = "ws-bridge")]
            spawner: self.spawner.clone(),
            #[cfg(feature = "ws-bridge")]
            callbacks,
            closed: AtomicBool::new(false),
            chat_connection: Arc::new(crate::runtime::ActionChannel::chat()),
            renderer_connection: Arc::new(crate::runtime::ActionChannel::renderer()),
            #[cfg(feature = "ws-bridge")]
            bridge: Mutex::new(None),
            #[cfg(feature = "ws-bridge")]
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

/// An account the host wants kept allowed on the Statement Store across
/// periods. Mirrors [`crate::runtime::StatementRenewalTarget`] with a
/// length-checked `account_id`, because UniFFI carries byte arrays as `Vec<u8>`
/// rather than a fixed width.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum NativeStatementRenewalTarget {
    /// The statement-store allowance account derived for one product.
    ProductStatementAllowance {
        /// Product the allowance account belongs to.
        product_id: String,
    },
    /// The wallet's own SSO account.
    WalletSso,
    /// A fixed account, such as a pairing peer's device statement key.
    Account {
        /// Account to keep allowed; exactly 32 bytes.
        account_id: Vec<u8>,
        /// Human-readable name used in logs and reports.
        label: String,
    },
}

/// A refused renewal-ledger call: tracking, untracking or reading it back.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeRenewalTargetError {
    /// `account_id` was not exactly 32 bytes.
    #[error("account_id must be exactly 32 bytes, got {actual}")]
    InvalidAccountId {
        /// Supplied byte length.
        actual: u64,
    },
    /// `product_id` is not a usable product identifier.
    #[error("product_id {product_id} is not a valid product identifier")]
    InvalidProductId {
        /// The identifier as supplied.
        product_id: String,
    },
    /// The core refused to record the targets.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl TryFrom<NativeStatementRenewalTarget> for crate::runtime::StatementRenewalTarget {
    type Error = NativeRenewalTargetError;

    fn try_from(target: NativeStatementRenewalTarget) -> Result<Self, Self::Error> {
        Ok(match target {
            NativeStatementRenewalTarget::ProductStatementAllowance { product_id } => {
                // The renewal account is derived from this string, and a product
                // connection derives its own from the normalized form. Skipping
                // the normalization here renews an account no product uses, and
                // the real one lapses at the next boundary.
                Self::ProductStatementAllowance {
                    product_id: normalize_product_identifier(&product_id)
                        .map_err(|_| NativeRenewalTargetError::InvalidProductId { product_id })?,
                }
            }
            NativeStatementRenewalTarget::WalletSso => Self::WalletSso,
            NativeStatementRenewalTarget::Account { account_id, label } => {
                let account_id: [u8; 32] = account_id.as_slice().try_into().map_err(|_| {
                    NativeRenewalTargetError::InvalidAccountId {
                        actual: account_id.len() as u64,
                    }
                })?;
                Self::Account { account_id, label }
            }
        })
    }
}

/// One entry the renewal ledger holds, as a host reads it back.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeTrackedStatementRenewalTarget {
    /// The account, or the recipe for one, that the host promised to renew.
    pub target: NativeStatementRenewalTarget,
    /// Root public key that promised a raw account id. A recipe carries none
    /// and resolves under whichever identity is active.
    pub owner: Option<Bytes32>,
}

impl From<crate::runtime::StatementRenewalTarget> for NativeStatementRenewalTarget {
    fn from(target: crate::runtime::StatementRenewalTarget) -> Self {
        match target {
            crate::runtime::StatementRenewalTarget::ProductStatementAllowance { product_id } => {
                Self::ProductStatementAllowance { product_id }
            }
            crate::runtime::StatementRenewalTarget::WalletSso => Self::WalletSso,
            crate::runtime::StatementRenewalTarget::Account { account_id, label } => {
                Self::Account {
                    account_id: account_id.to_vec(),
                    label,
                }
            }
        }
    }
}

impl From<crate::runtime::TrackedStatementRenewalTarget> for NativeTrackedStatementRenewalTarget {
    fn from(entry: crate::runtime::TrackedStatementRenewalTarget) -> Self {
        Self {
            target: entry.target.into(),
            owner: entry.owner,
        }
    }
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
    /// Construct one host-level runtime and optionally activate its local session.
    /// Pass native custody once here; omitting it selects the built-in Rust wallet.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: NativeHostRuntimeConfig,
        native_wallet: Option<Arc<dyn NativeCoinageCallbacks>>,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        let runtime_config: NativeResolvedHostRuntimeConfig = runtime_config.try_into()?;
        Self::from_resolved(
            callbacks,
            runtime_config,
            native_wallet,
            "truapi.native.host_runtime.boot",
            "host runtime ready",
        )
    }

    /// Open a connection-scoped execution with immutable trusted context.
    /// `chat_callbacks` installs the host's Chat adapter; hosts without the
    /// Chat modality pass `None`. `pocket_callbacks` does the same for the
    /// card collection.
    pub fn open_product_execution(
        &self,
        callbacks: Arc<dyn HostCallbacks>,
        chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
        pocket_callbacks: Option<Arc<dyn NativePocketCallbacks>>,
        execution_config: NativeProductExecutionConfig,
    ) -> Result<Arc<NativeProductExecution>, NativeRuntimeConfigError> {
        let product: ProductContext = execution_config.try_into()?;
        Ok(self.open_product_execution_with_callbacks(
            callbacks,
            chat_callbacks,
            pocket_callbacks,
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
        targets: Vec<NativeStatementRenewalTarget>,
    ) -> Result<(), NativeRenewalTargetError> {
        let targets = targets
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        futures::executor::block_on(self.runtime.track_statement_renewal_targets(targets))
            .map_err(|err| NativeRenewalTargetError::Rejected { reason: err.reason })
    }

    /// Every account the ledger tracks, in the order it was tracked.
    ///
    /// Needs no active session. Slots per period are finite, so a host that
    /// tracked a wrong or stale account can see it here and drop it with
    /// [`Self::untrack_statement_renewal_account`].
    pub fn statement_renewal_targets(
        &self,
    ) -> Result<Vec<NativeTrackedStatementRenewalTarget>, NativeRenewalTargetError> {
        futures::executor::block_on(self.runtime.statement_renewal_targets())
            .map(|entries| entries.into_iter().map(Into::into).collect())
            .map_err(|err| NativeRenewalTargetError::Rejected { reason: err.reason })
    }

    /// Root public key the active identity records its fixed entries under.
    ///
    /// Needs an active session, and fails with `Disconnected` without one.
    /// An entry from [`Self::statement_renewal_targets`] whose owner is this
    /// key, or which has no owner at all, is one a pass will renew; any other
    /// is one a pass will prune.
    pub fn statement_renewal_owner_key(&self) -> Result<Bytes32, NativeRenewalTargetError> {
        self.runtime
            .statement_renewal_owner_key()
            .map_err(|err| NativeRenewalTargetError::Rejected { reason: err.reason })
    }

    /// Stop renewing one fixed statement account, returning whether the ledger
    /// held it.
    ///
    /// Scoped to the active identity, so it never removes an entry another
    /// identity promised. Needs an active session to resolve that identity.
    pub fn untrack_statement_renewal_account(
        &self,
        account_id: Vec<u8>,
    ) -> Result<bool, NativeRenewalTargetError> {
        let account_id: [u8; 32] = account_id.as_slice().try_into().map_err(|_| {
            NativeRenewalTargetError::InvalidAccountId {
                actual: account_id.len() as u64,
            }
        })?;
        futures::executor::block_on(self.runtime.untrack_statement_renewal_account(&account_id))
            .map_err(|err| NativeRenewalTargetError::Rejected { reason: err.reason })
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

    /// Answer one decrypted SSO remote message from a wallet-managed
    /// statement-store session.
    ///
    /// `message` is one SCALE-encoded `RemoteMessage` exactly as decrypted from
    /// the session statement. The bytes are deliberately opaque at this
    /// boundary: the wallet forwards wire encodings verbatim and never
    /// constructs them. Session control and transport stay with the wallet —
    /// `Disconnected` is reported, never handled here. Confirmation-gated
    /// requests await `confirm_user_action`, so this can take arbitrarily long.
    pub async fn handle_sso_request(
        &self,
        message: Vec<u8>,
    ) -> Result<SsoRequestOutcome, HostRejection> {
        let message =
            decode_remote_message(&message).map_err(|reason| HostRejection::Rejected { reason })?;
        Ok(match self.runtime.answer_sso_request(message).await {
            CoreSsoRequestOutcome::Response(response) => SsoRequestOutcome::Response {
                message: response.encode(),
            },
            CoreSsoRequestOutcome::Disconnected => SsoRequestOutcome::Disconnected,
            CoreSsoRequestOutcome::Ignored => SsoRequestOutcome::Ignored,
        })
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
    platform: Arc<dyn truapi_platform::Platform>,
    chat: Option<Arc<dyn truapi_platform::ChatPlatform>>,
    pocket: Option<Arc<dyn truapi_platform::PocketPlatform>>,
    /// The same `CallbackPlatform` as `platform`, kept separately because
    /// `Arc<dyn Platform>` cannot be downcast to the optional capability.
    permission_status: Arc<dyn truapi_platform::PermissionStatusHost>,
    events: Arc<NativeEventBus>,
    /// Host-runtime events back the process-wide services shared by every
    /// product execution (chain, Statement Store, and Bulletin). Native
    /// responses must reach this bus as well as the execution-scoped bus.
    shared_events: Arc<NativeEventBus>,
    #[cfg(feature = "ws-bridge")]
    spawner: Spawner,
    #[cfg(feature = "ws-bridge")]
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
    #[cfg(feature = "ws-bridge")]
    bridge: Mutex<Option<WsBridge>>,
    #[cfg(feature = "ws-bridge")]
    product_control: Arc<Mutex<Option<crate::ProductRuntimeControl>>>,
}

impl NativeProductExecution {
    fn adapters(&self) -> crate::host_core::ConnectionAdapters {
        crate::host_core::ConnectionAdapters {
            platform: self.platform.clone(),
            chat_platform: self.chat.clone(),
            permission_status: Some(self.permission_status.clone()),
            chat: self.chat_connection.clone(),
            renderer: self.renderer_connection.clone(),
            pocket_platform: self.pocket.clone(),
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

    #[cfg(feature = "ws-bridge")]
    fn stop_bridge(&self) {
        if let Some(mut bridge) = self
            .bridge
            .lock()
            .expect("native product bridge mutex poisoned")
            .take()
        {
            bridge.stop();
        }
        *self
            .product_control
            .lock()
            .expect("native product control mutex poisoned") = None;
    }
}

#[uniffi::export]
impl NativeProductExecution {
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
        #[cfg(feature = "ws-bridge")]
        {
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
        #[cfg(not(feature = "ws-bridge"))]
        {
            let _ = (request, observer);
            Err(crate::ProductRuntimeError::NotConnected)
        }
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
        #[cfg(feature = "ws-bridge")]
        self.stop_bridge();
        self.chat_connection.close();
        self.renderer_connection.close();
    }
}

#[cfg(feature = "ws-bridge")]
#[uniffi::export]
impl NativeProductExecution {
    /// Start this execution's independently authenticated localhost bridge.
    pub fn start_ws_bridge(&self, bind_port: u16) -> Result<WsBridgeEndpoint, WsBridgeStartError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(WsBridgeStartError::Io(
                "product execution is closed".to_string(),
            ));
        }
        let mut guard = self
            .bridge
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
        let (bridge, endpoint) = WsBridge::start(bind_port, runtime_factory, logger)?;
        *guard = Some(bridge);
        Ok(endpoint)
    }

    /// Stop the active bridge while leaving the execution reusable.
    pub fn stop_ws_bridge(&self) {
        self.stop_bridge();
    }
}

impl Drop for NativeProductExecution {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`) for
/// the `tracing` output, which on native routes to stderr (system logs on
/// iOS/Android). Most native diagnostics flow through `on_core_log` instead;
/// this controls the cross-platform `tracing` events shared with wasm.
#[uniffi::export]
pub fn set_log_level(level: String) {
    crate::logging::set_level_from_str(&level);
}

/// Build a [`Spawner`] backed by a shared `futures::executor::ThreadPool`.
/// The pool is sized at the default (one worker per logical CPU). Falls
/// back to a thread-per-subscription spawner if the pool fails to build,
/// which only ever happens if the host has no available threads at all.
fn native_thread_pool_spawner(callbacks: &Arc<dyn HostCallbacks>) -> Spawner {
    match ThreadPool::new() {
        Ok(pool) => {
            let callbacks = callbacks.clone();
            Arc::new(move |fut: BoxFuture<'static, ()>| {
                if let Err(err) = pool.spawn(fut) {
                    callbacks.on_core_log(
                        "truapi.native.core.subscription.spawn_failed".to_string(),
                        format!("{err}"),
                    );
                }
            })
        }
        Err(err) => {
            callbacks.on_core_log(
                "truapi.native.core.subscription.pool_unavailable".to_string(),
                format!("{err}; falling back to thread-per-subscription"),
            );
            crate::subscription::thread_per_subscription_spawner()
        }
    }
}

struct CallbackPlatform {
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
}

impl crate::host_logic::worker::WorkerDemandObserver for CallbackPlatform {
    fn worker_demand_changed(&self, product_id: &str, transition: WorkerTransition) {
        self.callbacks
            .worker_demand_changed(product_id.to_string(), transition);
    }
}

#[derive(Default)]
struct NativeEventBus {
    theme_changes:
        Mutex<Vec<mpsc::UnboundedSender<Result<v01::HostThemeSubscribeItem, v01::GenericError>>>>,
    locale_changes:
        Mutex<Vec<mpsc::UnboundedSender<Result<v01::HostLocaleSubscribeItem, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    chain_responses: Mutex<HashMap<u32, mpsc::UnboundedSender<String>>>,
    chat_room_changes: Mutex<Vec<mpsc::UnboundedSender<v01::HostChatListSubscribeItem>>>,
    pocket_card_changes: Mutex<
        Vec<mpsc::UnboundedSender<Result<v01::HostPocketListSubscribeItem, v01::GenericError>>>,
    >,
}

struct PreimageSubscription {
    key: Vec<u8>,
    tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

impl NativeEventBus {
    fn subscribe_theme(
        &self,
        current: Result<v01::HostThemeSubscribeItem, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_theme_changed(&self, theme: v01::HostThemeSubscribeItem) {
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(theme.clone())).is_ok());
    }

    fn subscribe_locale(
        &self,
        current: Result<v01::HostLocaleSubscribeItem, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.locale_changes
            .lock()
            .expect("native locale subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_locale_changed(&self, locale: v01::HostLocaleSubscribeItem) {
        self.locale_changes
            .lock()
            .expect("native locale subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(locale.clone())).is_ok());
    }

    fn subscribe_preimage_changes(
        &self,
        key: Vec<u8>,
    ) -> mpsc::UnboundedReceiver<Result<Option<Vec<u8>>, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .push(PreimageSubscription { key, tx });
        rx
    }

    fn notify_preimage_changed(&self, key: &[u8], value: Option<Vec<u8>>) {
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .retain(|sub| {
                if sub.key != key {
                    return true;
                }
                sub.tx.unbounded_send(Ok(value.clone())).is_ok()
            });
    }

    fn register_chain(&self, connection_id: u32) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded();
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .insert(connection_id, tx);
        rx
    }

    fn notify_chain_response(&self, connection_id: u32, json: String) {
        let mut responses = self
            .chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned");
        let Some(tx) = responses.get(&connection_id) else {
            return;
        };
        if tx.unbounded_send(json).is_err() {
            responses.remove(&connection_id);
        }
    }

    fn notify_chain_closed(&self, connection_id: u32) {
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .remove(&connection_id);
    }

    fn subscribe_chat_rooms(
        &self,
        current: v01::HostChatListSubscribeItem,
    ) -> BoxStream<'static, v01::HostChatListSubscribeItem> {
        let (tx, rx) = mpsc::unbounded();
        self.chat_room_changes
            .lock()
            .expect("native Chat room subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_chat_rooms_changed(&self, rooms: Vec<v01::ChatRoom>) {
        let item = v01::HostChatListSubscribeItem { rooms };
        self.chat_room_changes
            .lock()
            .expect("native Chat room subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(item.clone()).is_ok());
    }

    /// Subscribe to the host's card collection. `snapshot` reads the host's
    /// cards while the subscriber mutex is held, so a replacement cannot land
    /// between the read and the registration: the snapshot is always the first
    /// item and every later change follows it in order. `snapshot` must not
    /// call back into [`NativeProductExecution::notify_pocket_cards_changed`],
    /// which takes the same mutex.
    fn subscribe_pocket_cards(
        &self,
        snapshot: impl FnOnce() -> Result<v01::HostPocketListSubscribeItem, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        let mut subscribers = self
            .pocket_card_changes
            .lock()
            .expect("native Pocket card subscribers mutex poisoned");
        let current = snapshot();
        // Subscribers are dropped when their product stops listening, and
        // nothing else prunes them between notifications.
        subscribers.retain(|tx| !tx.is_closed());
        subscribers.push(tx);
        drop(subscribers);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_pocket_cards_changed(&self, cards: Vec<v01::PocketCard>) {
        self.send_pocket_cards(Ok(v01::HostPocketListSubscribeItem { cards }));
    }

    /// Report that the host can no longer say what the collection holds. The
    /// product reads it as a failed stream rather than as an empty collection.
    fn notify_pocket_cards_failed(&self, error: v01::GenericError) {
        self.send_pocket_cards(Err(error));
    }

    fn send_pocket_cards(&self, item: Result<v01::HostPocketListSubscribeItem, v01::GenericError>) {
        self.pocket_card_changes
            .lock()
            .expect("native Pocket card subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(item.clone()).is_ok());
    }
}

struct NativeCoinageCallbackPlatform {
    callbacks: Arc<dyn NativeCoinageCallbacks>,
}

#[async_trait]
impl CoinageWalletHost for NativeCoinageCallbackPlatform {
    async fn native_coinage(
        &self,
        request: NativeCoinageRequest,
    ) -> Result<NativeCoinageResponse, v01::GenericError> {
        self.callbacks
            .native_coinage(request)
            .await
            .map(|result| result.response)
            .map_err(|_| v01::GenericError {
                reason: "Native Coinage wallet operation failed".into(),
            })
    }
}

#[async_trait]
impl NativeChatFilesHost for CallbackPlatform {
    async fn pick_chat_files(
        &self,
        request: NativeChatFilePickRequest,
    ) -> Result<Vec<NativeChatPickedFile>, v01::GenericError> {
        self.callbacks
            .pick_chat_files(request)
            .await
            .map_err(Into::into)
    }

    async fn read_chat_file(
        &self,
        source_id: String,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, v01::GenericError> {
        self.callbacks
            .read_chat_file(source_id, offset, length)
            .await
            .map_err(Into::into)
    }

    async fn release_chat_file(&self, source_id: String) -> Result<(), v01::GenericError> {
        self.callbacks
            .release_chat_file(source_id)
            .await
            .map_err(Into::into)
    }

    async fn begin_chat_file_export(
        &self,
        request: NativeChatFileExportRequest,
    ) -> Result<Option<String>, v01::GenericError> {
        self.callbacks
            .begin_chat_file_export(request)
            .await
            .map_err(Into::into)
    }

    async fn write_chat_file_export(
        &self,
        export_id: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.callbacks
            .write_chat_file_export(export_id, offset, data)
            .await
            .map_err(Into::into)
    }

    async fn finish_chat_file_export(&self, export_id: String) -> Result<(), v01::GenericError> {
        self.callbacks
            .finish_chat_file_export(export_id)
            .await
            .map_err(Into::into)
    }

    async fn cancel_chat_file_export(&self, export_id: String) -> Result<(), v01::GenericError> {
        self.callbacks
            .cancel_chat_file_export(export_id)
            .await
            .map_err(Into::into)
    }
}

#[async_trait]
impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        self.callbacks.navigate_to(url).await.map_err(Into::into)
    }
}

#[async_trait]
impl Notifications for CallbackPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.push_notification".to_string(),
            notification.text.clone(),
        );

        let id = self
            .callbacks
            .push_notification(notification)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.cancel_notification".to_string(),
            id.to_string(),
        );
        self.callbacks
            .cancel_notification(id)
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl truapi_platform::PermissionStatusHost for CallbackPlatform {
    async fn device_permission_status(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<truapi_platform::DevicePermissionStatus, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission_status".to_string(),
            format!("{request}"),
        );

        self.callbacks
            .device_permission_status(request)
            .await
            .map(Into::into)
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl Permissions for CallbackPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .device_permission(request)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.remote_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .remote_permission(request.permission)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

#[async_trait]
impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.feature_supported".to_string(),
            format!("{request:?}"),
        );

        let supported = self
            .callbacks
            .feature_supported(request)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostFeatureSupportedResponse { supported })
    }

    async fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.supported_chains".to_string(),
            String::new(),
        );

        self.callbacks
            .supported_chains()
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl ProductStorage for CallbackPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_read(key).map_err(Into::into)
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks
            .local_storage_write(key, value)
            .map_err(Into::into)
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_clear(key).map_err(Into::into)
    }
}

#[async_trait]
impl CoreStorage for CallbackPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .core_storage_read(key.encode())
            .map_err(v01::GenericError::from)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_write(key.encode(), value)
            .map_err(v01::GenericError::from)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_clear(key.encode())
            .map_err(v01::GenericError::from)
    }
}

struct NativeJsonRpcConnection {
    id: u32,
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
    response_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    closed: AtomicBool,
    private_rpc: bool,
}

impl JsonRpcConnection for NativeJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if let Err(err) = self.callbacks.chain_send(self.id, request) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_send_failed".to_string(),
                if self.private_rpc {
                    "HOP send failed".to_string()
                } else {
                    err.to_string()
                },
            );
            self.close();
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                self.callbacks.on_core_log(
                    "truapi.native.chain.responses_reused".to_string(),
                    "responses() called more than once".to_string(),
                );
                stream::empty().boxed()
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        self.events.notify_chain_closed(self.id);
        if let Err(err) = self.callbacks.chain_close(self.id) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_close_failed".to_string(),
                if self.private_rpc {
                    "HOP close failed".to_string()
                } else {
                    err.to_string()
                },
            );
        }
    }
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[async_trait]
impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash.to_vec())
            .map_err(v01::GenericError::from)?
        else {
            return Err(v01::GenericError {
                reason: "chain provider unavailable".to_string(),
            });
        };
        let response_rx = self.events.register_chain(connection_id);
        Ok(Box::new(NativeJsonRpcConnection {
            id: connection_id,
            callbacks: self.callbacks.clone(),
            events: self.events.clone(),
            response_rx: Mutex::new(Some(response_rx)),
            closed: AtomicBool::new(false),
            private_rpc: false,
        }))
    }
}

#[async_trait]
impl HopProvider for CallbackPlatform {
    async fn allowed_hop_endpoints(
        &self,
        bulletin_genesis_hash: [u8; 32],
    ) -> Result<Vec<String>, v01::GenericError> {
        self.callbacks
            .allowed_hop_endpoints(bulletin_genesis_hash.to_vec())
            .await
            .map_err(v01::GenericError::from)
    }

    async fn connect_hop(
        &self,
        bulletin_genesis_hash: [u8; 32],
        endpoint: String,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let allowed = self.allowed_hop_endpoints(bulletin_genesis_hash).await?;
        truapi_platform::ensure_allowed_hop_endpoint(&endpoint, &allowed)?;
        let Some(connection_id) = self
            .callbacks
            .hop_connect(bulletin_genesis_hash.to_vec(), endpoint)
            .map_err(v01::GenericError::from)?
        else {
            return Err(v01::GenericError {
                reason: "HOP provider unavailable".to_string(),
            });
        };
        let response_rx = self.events.register_chain(connection_id);
        Ok(Box::new(NativeJsonRpcConnection {
            id: connection_id,
            callbacks: self.callbacks.clone(),
            events: self.events.clone(),
            response_rx: Mutex::new(Some(response_rx)),
            closed: AtomicBool::new(false),
            private_rpc: true,
        }))
    }
}

impl AuthPresenter for CallbackPlatform {
    fn auth_state_changed(&self, state: truapi_platform::AuthState) {
        self.callbacks.on_core_log(
            "truapi.native.callback.auth_state_changed".to_string(),
            String::new(),
        );
        self.callbacks.auth_state_changed(state);
    }
}

#[async_trait]
impl UserConfirmation for CallbackPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_user_action".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_user_action(review)
            .await
            .map_err(v01::GenericError::from)
    }
}

impl ThemeHost for CallbackPlatform {
    fn subscribe_theme(
        &self,
    ) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
        let current = self
            .callbacks
            .current_theme()
            .map_err(v01::GenericError::from);
        self.events.subscribe_theme(current)
    }
}

impl LocaleHost for CallbackPlatform {
    fn subscribe_locale(
        &self,
    ) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
        let current = self
            .callbacks
            .current_locale()
            .map_err(v01::GenericError::from);
        self.events.subscribe_locale(current)
    }
}

impl PreimageHost for CallbackPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        // Register the change receiver first so no event between the lookup
        // and the subscription is lost, then await the current value lazily.
        let rx = self.events.subscribe_preimage_changes(key.clone());
        let callbacks = self.callbacks.clone();
        let current = async move {
            callbacks
                .lookup_preimage(key)
                .await
                .map_err(v01::GenericError::from)
        };
        stream::once(current).chain(rx).boxed()
    }
}

#[async_trait]
impl truapi_platform::IdentityBackendHost for CallbackPlatform {
    async fn identity_username_candidates(
        &self,
        username: String,
        people_chain_genesis_hash: [u8; 32],
    ) -> Result<Vec<[u8; 32]>, v01::GenericError> {
        let candidates = self
            .callbacks
            .identity_username_candidates(username, people_chain_genesis_hash.to_vec())
            .await
            .map_err(v01::GenericError::from)?;
        decode_identity_candidates(candidates)
    }
}

fn decode_identity_candidates(
    candidates: Vec<Vec<u8>>,
) -> Result<Vec<[u8; 32]>, v01::GenericError> {
    if candidates.len() > 32 {
        return Err(v01::GenericError {
            reason: "too many username candidates".into(),
        });
    }
    candidates
        .into_iter()
        .map(|candidate| {
            candidate.try_into().map_err(|_| v01::GenericError {
                reason: "username candidate is not AccountId32".into(),
            })
        })
        .collect()
}

/// [`truapi_platform::ChatPlatform`] served by host-provided
/// [`NativeChatCallbacks`]; constructed only when the host passed one.
struct ChatCallbackPlatform {
    chat: Arc<dyn NativeChatCallbacks>,
    events: Arc<NativeEventBus>,
}

#[async_trait]
impl truapi_platform::ChatPlatform for ChatCallbackPlatform {
    async fn create_chat_room(
        &self,
        _product: &ProductContext,
        request: v01::HostChatCreateRoomRequest,
    ) -> Result<v01::HostChatCreateRoomResponse, v01::HostChatCreateRoomError> {
        let status = self
            .chat
            .create_room(request.room_id, request.name, request.icon)
            .map_err(|error| v01::HostChatCreateRoomError::Unknown {
                reason: error.to_string(),
            })?;

        if status == v01::ChatRoomRegistrationStatus::New
            && let Ok(rooms) = self.chat.list_rooms()
        {
            self.events.notify_chat_rooms_changed(rooms);
        }

        Ok(v01::HostChatCreateRoomResponse { status })
    }

    async fn register_chat_bot(
        &self,
        _product: &ProductContext,
        request: v01::HostChatRegisterBotRequest,
    ) -> Result<v01::HostChatRegisterBotResponse, v01::HostChatRegisterBotError> {
        let status = self
            .chat
            .register_bot(request.bot_id, request.name, request.icon)
            .map_err(|error| v01::HostChatRegisterBotError::Unknown {
                reason: error.to_string(),
            })?;

        // No room-list republish: a bot identity is not a room. A host that
        // joins the bot to one signals that via `notify_chat_rooms_changed`.
        Ok(v01::HostChatRegisterBotResponse { status })
    }

    async fn post_chat_message(
        &self,
        _product: &ProductContext,
        request: v01::HostChatPostMessageRequest,
    ) -> Result<v01::HostChatPostMessageResponse, v01::HostChatPostMessageError> {
        let message_id = self
            .chat
            .post_message(request.room_id, request.payload)
            .map_err(|error| v01::HostChatPostMessageError::Unknown {
                reason: error.to_string(),
            })?;
        Ok(v01::HostChatPostMessageResponse { message_id })
    }

    fn subscribe_chat_rooms(
        &self,
        _product: &ProductContext,
    ) -> BoxStream<'static, Result<v01::HostChatListSubscribeItem, v01::GenericError>> {
        let current = v01::HostChatListSubscribeItem {
            rooms: self.chat.list_rooms().unwrap_or_default(),
        };
        Box::pin(self.events.subscribe_chat_rooms(current).map(Ok))
    }
}

/// [`truapi_platform::PocketPlatform`] served by host-provided
/// [`NativePocketCallbacks`]; constructed only when the host passed one.
struct PocketCallbackPlatform {
    pocket: Arc<dyn NativePocketCallbacks>,
    events: Arc<NativeEventBus>,
}

#[async_trait]
impl truapi_platform::PocketPlatform for PocketCallbackPlatform {
    fn subscribe_pocket_cards(
        &self,
        _product: &ProductContext,
    ) -> BoxStream<'static, Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
        let pocket = self.pocket.clone();
        Box::pin(self.events.subscribe_pocket_cards(move || {
            pocket
                .list_cards()
                .map(|cards| v01::HostPocketListSubscribeItem { cards })
                .map_err(|error| v01::GenericError {
                    reason: error.to_string(),
                })
        }))
    }

    async fn remove_pocket_card(
        &self,
        _product: &ProductContext,
        request: v01::HostPocketRemoveCardRequest,
    ) -> Result<(), v01::HostPocketRemoveCardError> {
        let unknown = |error: HostRejection| v01::HostPocketRemoveCardError::Unknown {
            reason: error.to_string(),
        };
        match self.pocket.remove_card(request.card_id).map_err(unknown)? {
            NativePocketRemoval::Privileged => Err(v01::HostPocketRemoveCardError::Privileged),
            // A card this host does not hold is already removed.
            NativePocketRemoval::Absent => Ok(()),
            NativePocketRemoval::Removed => {
                // The removal stands either way. A host that can no longer
                // list its cards says so on the stream rather than leaving the
                // product on a list that still holds the removed card.
                match self.pocket.list_cards() {
                    Ok(cards) => self.events.notify_pocket_cards_changed(cards),
                    Err(error) => self.events.notify_pocket_cards_failed(v01::GenericError {
                        reason: error.to_string(),
                    }),
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use truapi::Bytes32;
    use truapi::v01::LegacyAccountTxPayload;
    use truapi_platform::CreateTransactionReview;

    #[test]
    fn native_identity_candidates_reject_malformed_accounts_and_oversized_sets() {
        assert_eq!(
            decode_identity_candidates(vec![vec![3; 32]]).unwrap(),
            vec![[3; 32]]
        );
        assert!(decode_identity_candidates(vec![vec![3; 31]]).is_err());
        assert!(decode_identity_candidates(vec![vec![3; 33]]).is_err());
        assert!(decode_identity_candidates(vec![vec![3; 32]; 33]).is_err());
    }

    type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

    fn pocket_card(card_id: &str, privileged: bool) -> v01::PocketCard {
        v01::PocketCard {
            card_id: card_id.to_string(),
            privileged,
        }
    }

    /// Everything a Pocket stream has already queued, so a missing item reads
    /// as pending here rather than hanging the test.
    fn drain_pocket(
        stream: &mut BoxStream<
            'static,
            Result<v01::HostPocketListSubscribeItem, v01::GenericError>,
        >,
    ) -> Vec<Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
        let mut seen = Vec::new();
        while let Some(Some(item)) = stream.next().now_or_never() {
            seen.push(item);
        }
        seen
    }

    /// The snapshot is read while the subscriber mutex is held, so it is the
    /// first item and every later change follows it in order. Delivering a
    /// queued change first would leave the product on the older list, with the
    /// snapshot overwriting it.
    #[test]
    fn a_pocket_subscriber_sees_its_snapshot_before_later_changes() {
        let bus = NativeEventBus::default();
        let mut stream = bus.subscribe_pocket_cards(|| {
            Ok(v01::HostPocketListSubscribeItem {
                cards: vec![pocket_card("loyalty", false)],
            })
        });
        bus.notify_pocket_cards_changed(vec![
            pocket_card("loyalty", false),
            pocket_card("humanity", true),
        ]);

        assert_eq!(
            drain_pocket(&mut stream),
            vec![
                Ok(v01::HostPocketListSubscribeItem {
                    cards: vec![pocket_card("loyalty", false)],
                }),
                Ok(v01::HostPocketListSubscribeItem {
                    cards: vec![pocket_card("loyalty", false), pocket_card("humanity", true)],
                }),
            ]
        );
    }

    /// A host that cannot say what it holds reaches the product as a failed
    /// stream. Reporting an empty list instead would read as "you own no
    /// cards" and wipe the product's view of its own collection.
    #[test]
    fn a_pocket_host_failure_reaches_the_product_instead_of_an_empty_list() {
        let bus = NativeEventBus::default();
        let mut opening = bus.subscribe_pocket_cards(|| {
            Err(v01::GenericError {
                reason: "card store unavailable".to_string(),
            })
        });
        assert_eq!(
            drain_pocket(&mut opening),
            vec![Err(v01::GenericError {
                reason: "card store unavailable".to_string(),
            })]
        );

        let mut live = bus.subscribe_pocket_cards(|| {
            Ok(v01::HostPocketListSubscribeItem {
                cards: vec![pocket_card("loyalty", false)],
            })
        });
        bus.notify_pocket_cards_failed(v01::GenericError {
            reason: "card store went away".to_string(),
        });
        assert_eq!(
            drain_pocket(&mut live),
            vec![
                Ok(v01::HostPocketListSubscribeItem {
                    cards: vec![pocket_card("loyalty", false)],
                }),
                Err(v01::GenericError {
                    reason: "card store went away".to_string(),
                }),
            ]
        );
    }

    /// A cancelled subscriber is pruned when the next one registers, so a
    /// product that subscribes and drops repeatedly cannot grow the list
    /// without bound between host notifications.
    #[test]
    fn a_cancelled_pocket_subscriber_is_pruned_on_the_next_registration() {
        let bus = NativeEventBus::default();
        let snapshot = || Ok(v01::HostPocketListSubscribeItem { cards: Vec::new() });
        for _ in 0..5 {
            drop(bus.subscribe_pocket_cards(snapshot));
        }
        let _live = bus.subscribe_pocket_cards(snapshot);

        assert_eq!(
            bus.pocket_card_changes
                .lock()
                .expect("native Pocket card subscribers mutex poisoned")
                .len(),
            1
        );
    }

    /// UniFFI hands `account_id` over as a length-free `Vec<u8>`, so the width
    /// the ledger depends on is only enforced here. A short id that converted
    /// anyway would renew an allowance for the wrong account.
    #[test]
    fn a_renewal_target_account_id_must_be_exactly_32_bytes() {
        let target = |len: usize| NativeStatementRenewalTarget::Account {
            account_id: vec![0x11; len],
            label: "device".to_string(),
        };

        assert!(matches!(
            crate::runtime::StatementRenewalTarget::try_from(target(32)),
            Ok(crate::runtime::StatementRenewalTarget::Account { account_id, .. })
                if account_id == [0x11; 32]
        ));
        for len in [0, 31, 33] {
            assert!(
                matches!(
                    crate::runtime::StatementRenewalTarget::try_from(target(len)),
                    Err(NativeRenewalTargetError::InvalidAccountId { actual }) if actual == len as u64
                ),
                "a {len}-byte account id must be rejected, and report its length"
            );
        }
    }

    #[test]
    fn an_unexpected_foreign_error_converts_instead_of_panicking() {
        // A host that throws an exception its trait does not declare lands in
        // `try_convert_unexpected_callback_error`. Without a `From` impl the
        // generic converter panics, and `panic = "abort"` turns that into a
        // process abort on the shipping build.
        let reason = "android.database.sqlite.SQLiteFullException";
        let rejection = <HostRejection as uniffi::ConvertError<crate::UniFfiTag>>::
            try_convert_unexpected_callback_error(
                uniffi::UnexpectedUniFFICallbackError::new(reason),
            )
            .expect("an unexpected foreign error must convert");
        let HostRejection::Rejected { reason: converted } = rejection;
        assert_eq!(converted, reason);

        let storage = <HostStorageError as uniffi::ConvertError<crate::UniFfiTag>>::
            try_convert_unexpected_callback_error(
                uniffi::UnexpectedUniFFICallbackError::new(reason),
            )
            .expect("an unexpected foreign error must convert");
        assert_eq!(
            v01::HostLocalStorageReadError::from(storage),
            v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            }
        );

        let navigate = <HostNavigateRejection as uniffi::ConvertError<crate::UniFfiTag>>::
            try_convert_unexpected_callback_error(
                uniffi::UnexpectedUniFFICallbackError::new(reason),
            )
            .expect("an unexpected foreign error must convert");
        let HostNavigateRejection::Navigate(navigate) = navigate;
        assert_eq!(
            navigate,
            v01::HostNavigateToError::Unknown {
                reason: reason.to_string(),
            }
        );
    }

    /// The renewal account is derived from `product_id`, and a product
    /// connection derives its own from the normalized form, so an unnormalized
    /// id here renews an account no product uses while the real one lapses.
    #[test]
    fn a_product_target_normalizes_its_identifier() {
        for supplied in [
            "  truapi-playground.dot  ",
            "TruAPI-Playground.dot",
            "TRUAPI-PLAYGROUND.DOT",
        ] {
            let converted = crate::runtime::StatementRenewalTarget::try_from(
                NativeStatementRenewalTarget::ProductStatementAllowance {
                    product_id: supplied.to_string(),
                },
            );
            assert!(
                matches!(
                    converted,
                    Ok(crate::runtime::StatementRenewalTarget::ProductStatementAllowance {
                        ref product_id
                    }) if product_id == "truapi-playground.dot"
                ),
                "{supplied:?} did not normalize: {converted:?}"
            );
        }

        assert!(matches!(
            crate::runtime::StatementRenewalTarget::try_from(
                NativeStatementRenewalTarget::ProductStatementAllowance {
                    product_id: "not a product".to_string(),
                }
            ),
            Err(NativeRenewalTargetError::InvalidProductId { .. })
        ));
    }

    // The read direction is the one a host audits its slots through, so a
    // dropped account id or owner there is a silent wrong answer rather than a
    // compile error.
    #[test]
    fn a_tracked_entry_survives_the_trip_out_to_the_native_boundary() {
        let entry = crate::runtime::TrackedStatementRenewalTarget {
            target: crate::runtime::StatementRenewalTarget::Account {
                account_id: [7; 32],
                label: "device".to_string(),
            },
            owner: Some([9; 32]),
        };

        let native = NativeTrackedStatementRenewalTarget::from(entry);

        assert_eq!(native.owner, Some([9; 32]));
        assert!(matches!(
            native.target,
            NativeStatementRenewalTarget::Account { account_id, label }
                if account_id == vec![7; 32] && label == "device"
        ));
    }

    #[test]
    fn a_recipe_entry_reports_no_owner_across_the_boundary() {
        let entry = crate::runtime::TrackedStatementRenewalTarget {
            target: crate::runtime::StatementRenewalTarget::WalletSso,
            owner: None,
        };

        let native = NativeTrackedStatementRenewalTarget::from(entry);

        assert!(native.owner.is_none());
        assert!(matches!(
            native.target,
            NativeStatementRenewalTarget::WalletSso
        ));
    }

    /// The other two variants carry no bytes to validate, so they must convert
    /// rather than share the `Account` arm's failure path.
    #[test]
    fn byteless_renewal_targets_convert() {
        assert!(matches!(
            crate::runtime::StatementRenewalTarget::try_from(
                NativeStatementRenewalTarget::WalletSso
            ),
            Ok(crate::runtime::StatementRenewalTarget::WalletSso)
        ));
        assert!(matches!(
            crate::runtime::StatementRenewalTarget::try_from(
                NativeStatementRenewalTarget::ProductStatementAllowance {
                    product_id: "truapi-playground.dot".to_string(),
                }
            ),
            Ok(crate::runtime::StatementRenewalTarget::ProductStatementAllowance { product_id })
                if product_id == "truapi-playground.dot"
        ));
    }

    fn text_chat_action(text: &str) -> v01::HostChatActionSubscribeItem {
        v01::HostChatActionSubscribeItem {
            room_id: "room".to_string(),
            peer: "native".to_string(),
            payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
                text: text.to_string(),
            }),
        }
    }

    struct EventCallbacks {
        chat_room_status: Mutex<v01::ChatRoomRegistrationStatus>,
        chat_created_rooms: Mutex<Vec<(String, String, String)>>,
        chat_bot_status: Mutex<v01::ChatBotRegistrationStatus>,
        chat_registered_bots: Mutex<Vec<(String, String, String)>>,
        chat_bot_rejection: Mutex<Option<String>>,
        chat_post_rejection: Mutex<Option<String>>,
        chat_posted: Mutex<Vec<(String, v01::ChatMessageContent)>>,
        pocket_cards: Mutex<Vec<v01::PocketCard>>,
        pocket_removed: Mutex<Vec<String>>,
        theme: Mutex<v01::HostThemeSubscribeItem>,
        locale: Mutex<v01::HostLocaleSubscribeItem>,
        preimages: Mutex<PreimageFixtureEntries>,
        auth_states: Mutex<Vec<AuthState>>,
        chain_id: Mutex<Option<u32>>,
        chain_connects: Mutex<Vec<Vec<u8>>>,
        chain_sends: Mutex<Vec<(u32, String)>>,
        chain_closes: Mutex<Vec<u32>>,
        /// Worker demand transitions, in arrival order.
        worker_demand: Mutex<Vec<(String, WorkerTransition)>>,
        /// Capability this host reports as refused by the OS, if any.
        os_refused: Option<v01::HostDevicePermissionRequest>,
    }

    impl EventCallbacks {
        /// Same as [`Self::new`], reporting `capability` as refused by the OS.
        fn refusing(capability: v01::HostDevicePermissionRequest) -> Self {
            Self {
                os_refused: Some(capability),
                ..Self::new()
            }
        }

        fn new() -> Self {
            Self {
                chat_room_status: Mutex::new(v01::ChatRoomRegistrationStatus::New),
                chat_created_rooms: Mutex::new(Vec::new()),
                chat_bot_status: Mutex::new(v01::ChatBotRegistrationStatus::New),
                chat_registered_bots: Mutex::new(Vec::new()),
                chat_bot_rejection: Mutex::new(None),
                chat_post_rejection: Mutex::new(None),
                chat_posted: Mutex::new(Vec::new()),
                pocket_cards: Mutex::new(Vec::new()),
                pocket_removed: Mutex::new(Vec::new()),
                theme: Mutex::new(v01::HostThemeSubscribeItem {
                    name: v01::ThemeName::Default,
                    variant: v01::ThemeVariant::Light,
                }),
                locale: Mutex::new(v01::HostLocaleSubscribeItem {
                    language_tag: "en".to_string(),
                }),
                preimages: Mutex::new(Vec::new()),
                auth_states: Mutex::new(Vec::new()),
                chain_id: Mutex::new(None),
                chain_connects: Mutex::new(Vec::new()),
                chain_sends: Mutex::new(Vec::new()),
                chain_closes: Mutex::new(Vec::new()),
                worker_demand: Mutex::new(Vec::new()),
                os_refused: None,
            }
        }
    }

    fn unavailable_chat_files<T>() -> Result<T, HostRejection> {
        Err(HostRejection::Rejected {
            reason: "native Chat files unavailable in this fixture".into(),
        })
    }

    #[async_trait::async_trait]
    impl HostCallbacks for EventCallbacks {
        async fn pick_chat_files(
            &self,
            _: NativeChatFilePickRequest,
        ) -> Result<Vec<NativeChatPickedFile>, HostRejection> {
            unavailable_chat_files()
        }
        async fn read_chat_file(
            &self,
            _: String,
            _: u64,
            _: u32,
        ) -> Result<Vec<u8>, HostRejection> {
            unavailable_chat_files()
        }
        async fn release_chat_file(&self, _: String) -> Result<(), HostRejection> {
            unavailable_chat_files()
        }
        async fn begin_chat_file_export(
            &self,
            _: NativeChatFileExportRequest,
        ) -> Result<Option<String>, HostRejection> {
            unavailable_chat_files()
        }
        async fn write_chat_file_export(
            &self,
            _: String,
            _: u64,
            _: Vec<u8>,
        ) -> Result<(), HostRejection> {
            unavailable_chat_files()
        }
        async fn finish_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
            unavailable_chat_files()
        }
        async fn cancel_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
            unavailable_chat_files()
        }
        async fn identity_username_candidates(
            &self,
            _: String,
            _: Vec<u8>,
        ) -> Result<Vec<Vec<u8>>, HostRejection> {
            Err(HostRejection::Rejected {
                reason: "no identity provider in event fixture".into(),
            })
        }
        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn worker_demand_changed(&self, product_id: String, transition: WorkerTransition) {
            self.worker_demand
                .lock()
                .expect("worker demand mutex poisoned")
                .push((product_id, transition));
        }
        async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        async fn push_notification(
            &self,
            _request: v01::HostPushNotificationRequest,
        ) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn device_permission(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn device_permission_status(
            &self,
            request: v01::HostDevicePermissionRequest,
        ) -> Result<NativeDevicePermissionStatus, HostRejection> {
            Ok(if self.os_refused == Some(request) {
                NativeDevicePermissionStatus::Denied
            } else {
                NativeDevicePermissionStatus::NotApplicable
            })
        }
        async fn remote_permission(
            &self,
            _request: v01::RemotePermission,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn auth_state_changed(&self, state: AuthState) {
            self.auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .push(state);
        }
        fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn core_storage_write(&self, _key: Vec<u8>, _value: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn allowed_hop_endpoints(&self, _: Vec<u8>) -> Result<Vec<String>, HostRejection> {
            Ok(Vec::new())
        }
        fn hop_connect(&self, _: Vec<u8>, _: String) -> Result<Option<u32>, HostRejection> {
            Ok(None)
        }
        fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
            self.chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .push(genesis_hash);
            Ok(*self.chain_id.lock().expect("chain id mutex poisoned"))
        }
        fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection> {
            self.chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .push((connection_id, request));
            Ok(())
        }
        fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection> {
            self.chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .push(connection_id);
            Ok(())
        }
        async fn confirm_user_action(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(self
                .preimages
                .lock()
                .expect("preimage map mutex poisoned")
                .iter()
                .find(|(stored_key, _)| stored_key == &key)
                .and_then(|(_, value)| value.clone()))
        }
        fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
            Ok(self.theme.lock().expect("theme mutex poisoned").clone())
        }
        fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
            Ok(self.locale.lock().expect("locale mutex poisoned").clone())
        }
        async fn feature_supported(
            &self,
            _request: v01::HostFeatureSupportedRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, HostRejection> {
            Ok(truapi_platform::HostChainSet {
                network: "paseo".to_string(),
                chains: Vec::new(),
            })
        }
        fn local_storage_read(&self, _key: String) -> Result<Option<Vec<u8>>, HostStorageError> {
            Ok(None)
        }
        fn local_storage_write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), HostStorageError> {
            Ok(())
        }
        fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
            Ok(())
        }
    }

    impl NativePocketCallbacks for EventCallbacks {
        fn list_cards(&self) -> Result<Vec<v01::PocketCard>, HostRejection> {
            Ok(self
                .pocket_cards
                .lock()
                .expect("pocket cards mutex poisoned")
                .clone())
        }

        fn remove_card(&self, card_id: String) -> Result<NativePocketRemoval, HostRejection> {
            let mut cards = self
                .pocket_cards
                .lock()
                .expect("pocket cards mutex poisoned");
            let outcome = match cards.iter().find(|card| card.card_id == card_id) {
                None => NativePocketRemoval::Absent,
                Some(card) if card.privileged => NativePocketRemoval::Privileged,
                Some(_) => {
                    cards.retain(|card| card.card_id != card_id);
                    NativePocketRemoval::Removed
                }
            };
            drop(cards);
            self.pocket_removed
                .lock()
                .expect("pocket removed mutex poisoned")
                .push(card_id);
            Ok(outcome)
        }
    }

    impl NativeChatCallbacks for EventCallbacks {
        fn create_room(
            &self,
            room_id: String,
            name: String,
            icon: String,
        ) -> Result<v01::ChatRoomRegistrationStatus, HostRejection> {
            self.chat_created_rooms
                .lock()
                .expect("created rooms mutex poisoned")
                .push((room_id, name, icon));
            Ok(*self
                .chat_room_status
                .lock()
                .expect("room status mutex poisoned"))
        }

        fn register_bot(
            &self,
            bot_id: String,
            name: String,
            icon: String,
        ) -> Result<v01::ChatBotRegistrationStatus, HostRejection> {
            if let Some(reason) = self
                .chat_bot_rejection
                .lock()
                .expect("bot rejection mutex poisoned")
                .clone()
            {
                return Err(HostRejection::Rejected { reason });
            }
            self.chat_registered_bots
                .lock()
                .expect("registered bots mutex poisoned")
                .push((bot_id, name, icon));
            Ok(*self
                .chat_bot_status
                .lock()
                .expect("bot status mutex poisoned"))
        }

        fn post_message(
            &self,
            room_id: String,
            content: v01::ChatMessageContent,
        ) -> Result<String, HostRejection> {
            if let Some(reason) = self
                .chat_post_rejection
                .lock()
                .expect("post rejection mutex poisoned")
                .clone()
            {
                return Err(HostRejection::Rejected { reason });
            }
            let mut posted = self
                .chat_posted
                .lock()
                .expect("posted messages mutex poisoned");
            posted.push((room_id, content));
            // Distinct per message: a correlation assertion must not pass on a
            // constant the host happens to return every time.
            Ok(format!("message-{}", posted.len()))
        }

        fn list_rooms(&self) -> Result<Vec<v01::ChatRoom>, HostRejection> {
            let mut room_ids: Vec<String> = self
                .chat_created_rooms
                .lock()
                .expect("created rooms mutex poisoned")
                .iter()
                .map(|(room_id, _, _)| room_id.clone())
                .collect();
            room_ids.sort();
            room_ids.dedup();
            Ok(room_ids
                .into_iter()
                .map(|room_id| v01::ChatRoom {
                    room_id,
                    participating_as: v01::ChatRoomParticipation::RoomHost,
                })
                .collect())
        }
    }

    fn event_platform() -> (Arc<EventCallbacks>, Arc<NativeEventBus>, CallbackPlatform) {
        let callbacks = Arc::new(EventCallbacks::new());
        let events = Arc::new(NativeEventBus::default());
        let platform = CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
        };
        (callbacks, events, platform)
    }

    fn native_host_runtime_config() -> NativeHostRuntimeConfig {
        NativeHostRuntimeConfig {
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            host_platform: HostPlatform::Unknown,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0xa2; 32],
            bulletin_chain_genesis_hash: vec![0xbb; 32],
            asset_hub_chain_genesis_hash: vec![0xcc; 32],
            network_suffix: "paseo".to_string(),
            local_session_secret: Some(vec![7; 32]),
            local_session_lite_username: Some("alice".to_string()),
            coinage_instance_id: None,
        }
    }

    fn native_execution_config(
        product_id: &str,
        execution_kind: ProductExecutionKind,
    ) -> NativeProductExecutionConfig {
        NativeProductExecutionConfig {
            product_id: product_id.to_string(),
            execution_kind,
        }
    }

    #[cfg(feature = "ws-bridge")]
    fn native_product_execution(
        callbacks: Arc<dyn HostCallbacks>,
        product_id: &str,
    ) -> Arc<NativeProductExecution> {
        let mut config = native_host_runtime_config();
        config.local_session_secret = None;
        config.local_session_lite_username = None;
        let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config, None)
            .expect("host runtime config should be valid");
        host.open_product_execution(
            callbacks,
            None,
            None,
            native_execution_config(product_id, ProductExecutionKind::App),
        )
        .expect("product execution config should be valid")
    }

    #[test]
    fn process_runtime_counts_worker_references_per_product() {
        let callbacks = Arc::new(EventCallbacks::new());
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            callbacks.clone(),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let product = || "shared.dot".to_string();

        host.acquire_worker(product());
        host.acquire_worker(product());
        host.release_worker(product());
        host.release_worker(product());

        assert_eq!(
            *callbacks
                .worker_demand
                .lock()
                .expect("worker demand mutex poisoned"),
            vec![
                (product(), WorkerTransition::Start),
                (product(), WorkerTransition::Stop),
            ]
        );
    }

    #[test]
    fn process_runtime_shares_authority_and_replaces_one_chat_execution_per_product() {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let app = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
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
    fn native_wallet_dependency_survives_failure_and_product_replacement() {
        use parking_lot::Mutex;
        use std::sync::atomic::AtomicU8;
        use truapi::api::Payment;
        use truapi::versioned::payment::{HostPaymentTopUpError, HostPaymentTopUpRequest};
        use truapi_platform::{
            NativeCoinageFailure, NativeCoinageOperation, NativeCoinageTopUpOutcome,
        };

        struct Wallet {
            state: AtomicU8,
            products: Mutex<Vec<String>>,
        }
        #[async_trait::async_trait]
        impl NativeCoinageCallbacks for Wallet {
            async fn native_coinage(
                &self,
                request: NativeCoinageRequest,
            ) -> Result<NativeCoinageCallbackResult, HostRejection> {
                let request = zeroize::Zeroizing::new(request);
                if matches!(request.operation, NativeCoinageOperation::Reconcile) {
                    return Ok(NativeCoinageCallbackResult {
                        response: NativeCoinageResponse::Done,
                    });
                }
                let NativeCoinageOperation::TopUp { product_id, .. } = &request.operation else {
                    panic!("top-up must reach native custody without a Rust inventory scan");
                };
                self.products.lock().push(product_id.clone());
                let response = match self.state.load(Ordering::Acquire) {
                    0 => NativeCoinageResponse::Failed {
                        reason: NativeCoinageFailure::Unavailable,
                    },
                    1 => {
                        return Err(HostRejection::Rejected {
                            reason: "private-native-error".into(),
                        });
                    }
                    _ => NativeCoinageResponse::TopUp {
                        outcome: NativeCoinageTopUpOutcome::NotClaimed,
                    },
                };
                Ok(NativeCoinageCallbackResult { response })
            }
        }

        let wallet = Arc::new(Wallet {
            state: AtomicU8::new(0),
            products: Mutex::new(Vec::new()),
        });
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            Some(wallet.clone()),
        )
        .expect("runtime installs native custody before activation");
        futures::executor::block_on(async {
            let open = || {
                host.open_product_execution(
                    Arc::new(EventCallbacks::new()),
                    None,
                    None,
                    native_execution_config("wallet.dot", ProductExecutionKind::Worker),
                )
                .expect("product opens without supplying wallet callbacks")
            };
            let execution = open();
            let source = schnorrkel::MiniSecretKey::from_bytes(&[9; 32])
                .unwrap()
                .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
            let request = || {
                HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
                    into: None,
                    amount: 1,
                    source: v01::PaymentTopUpSource::Coins {
                        sr25519_secret_keys: vec![source.secret.to_bytes()],
                    },
                })
            };
            let admin = execution.admin();
            for state in [0, 1] {
                wallet.state.store(state, Ordering::Release);
                let result = admin
                    .product_runtime()
                    .top_up(&truapi::CallContext::default(), request())
                    .await;
                let Err(truapi::CallError::Domain(HostPaymentTopUpError::V1(
                    v01::HostPaymentTopUpError::Unknown { reason },
                ))) = result
                else {
                    panic!("unavailable or failing native custody must fail closed");
                };
                assert!(!reason.contains("private-native-error"));
            }
            // Replacing all product callbacks cannot reset or replace native custody.
            let replacement = open();
            wallet.state.store(2, Ordering::Release);
            assert!(matches!(
                replacement
                    .admin()
                    .product_runtime()
                    .top_up(&truapi::CallContext::default(), request())
                    .await,
                Err(truapi::CallError::Domain(HostPaymentTopUpError::V1(
                    v01::HostPaymentTopUpError::InsufficientFunds
                )))
            ));
            assert_eq!(
                wallet.products.lock().as_slice(),
                &["wallet.dot", "wallet.dot", "wallet.dot"],
            );
        });
    }

    #[test]
    fn native_pocket_removal_outcomes_are_decided_by_the_host() {
        let pocket_host = Arc::new(EventCallbacks::new());
        *pocket_host
            .pocket_cards
            .lock()
            .expect("pocket cards mutex poisoned") =
            vec![pocket_card("loyalty", false), pocket_card("humanity", true)];
        let events = Arc::new(NativeEventBus::default());
        let platform = PocketCallbackPlatform {
            pocket: pocket_host.clone(),
            events,
        };
        let product = ProductContext::new("pocket.dot".to_string()).expect("valid product id");
        let remove = |card_id: &str| {
            futures::executor::block_on(truapi_platform::PocketPlatform::remove_pocket_card(
                &platform,
                &product,
                v01::HostPocketRemoveCardRequest {
                    card_id: card_id.to_string(),
                },
            ))
        };
        let mut cards =
            truapi_platform::PocketPlatform::subscribe_pocket_cards(&platform, &product);
        let first = futures::executor::block_on(cards.next())
            .expect("the current list arrives on subscribe")
            .expect("no stream error");
        assert_eq!(
            first.cards,
            vec![pocket_card("loyalty", false), pocket_card("humanity", true)]
        );

        assert!(matches!(
            remove("humanity"),
            Err(v01::HostPocketRemoveCardError::Privileged)
        ));
        assert!(
            remove("absent").is_ok(),
            "an absent card is already removed"
        );
        assert!(remove("loyalty").is_ok());
        assert_eq!(
            pocket_host
                .pocket_removed
                .lock()
                .expect("pocket removed mutex poisoned")
                .as_slice(),
            ["humanity", "absent", "loyalty"],
            "the host decides every removal, so every request reaches it"
        );

        let republished = futures::executor::block_on(cards.next())
            .expect("a removal republishes the list")
            .expect("no stream error");
        assert_eq!(
            republished.cards,
            vec![pocket_card("humanity", true)],
            "the pinned card stays and the removed one is gone"
        );
    }

    #[test]
    fn native_chat_entrypoint_is_unsupported_without_an_adapter() {
        let mut config = native_host_runtime_config();
        config.local_session_secret = Some(vec![7; 32]);
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            config,
            None,
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                None,
                None,
                native_execution_config("chat-product.dot", ProductExecutionKind::Worker),
            )
            .expect("Chat execution should open");

        let result = execution.publish_chat_action(text_chat_action("hello"));

        assert!(matches!(
            result,
            Err(crate::ProductRuntimeError::Unsupported)
        ));
    }

    /// Hosts mediate product network access in their own code and ask this to
    /// decide whether to prompt, so it has to answer for the spellings a host
    /// actually holds, not only the normalized one the core passes internally.
    #[test]
    fn the_trusted_export_normalizes_before_matching() {
        for trusted in [
            "peopl.dot",
            "PEOPL.DOT",
            "  peopl.dot  ",
            "dim2.paseo",
            "stash.dot",
        ] {
            assert!(
                super::has_trusted_remote_permissions(trusted.to_string()),
                "{trusted} is a first-party product",
            );
        }
        for untrusted in [
            "app.peopl.dot",
            "peopl",
            "notpeopl.dot",
            "localhost:3000",
            "",
            "   ",
        ] {
            assert!(
                !super::has_trusted_remote_permissions(untrusted.to_string()),
                "{untrusted} is not",
            );
        }
    }

    #[test]
    fn native_auth_presenter_forwards_states_across_the_ffi_mirror() {
        let (callbacks, _events, platform) = event_platform();

        platform.auth_state_changed(truapi_platform::AuthState::Pairing {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
        });
        platform.auth_state_changed(truapi_platform::AuthState::Connected(
            truapi_platform::SessionUiInfo {
                public_key: [7; 32],
                identity_account_id: None,
                chat_public_key: None,
                device_enc_public_key: None,
                peer_statement_account_id: None,
                lite_username: Some("alice".to_string()),
                full_username: None,
            },
        ));
        platform.auth_state_changed(truapi_platform::AuthState::Disconnected);

        assert_eq!(
            callbacks
                .auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .as_slice(),
            &[
                AuthState::Pairing {
                    deeplink: "polkadotapp://pair?handshake=00".to_string(),
                },
                AuthState::Connected(truapi_platform::SessionUiInfo {
                    public_key: [7; 32],
                    identity_account_id: None,
                    chat_public_key: None,
                    device_enc_public_key: None,
                    peer_statement_account_id: None,
                    lite_username: Some("alice".to_string()),
                    full_username: None,
                }),
                AuthState::Disconnected,
            ]
        );
    }

    #[test]
    fn native_locale_subscription_emits_current_then_notified_changes() {
        let (callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_locale();
        let switched = v01::HostLocaleSubscribeItem {
            language_tag: "zh-Hans".to_string(),
        };

        let first = futures::executor::block_on(stream.next()).unwrap();
        *callbacks.locale.lock().expect("locale mutex poisoned") = switched.clone();
        events.notify_locale_changed(switched.clone());
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(
            first.unwrap(),
            v01::HostLocaleSubscribeItem {
                language_tag: "en".to_string(),
            }
        );
        assert_eq!(second.unwrap(), switched);
    }

    #[test]
    fn native_theme_subscription_emits_current_then_notified_changes() {
        let (callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_theme();
        let named = v01::HostThemeSubscribeItem {
            name: v01::ThemeName::Custom("midnight".to_string()),
            variant: v01::ThemeVariant::Dark,
        };

        let first = futures::executor::block_on(stream.next()).unwrap();
        *callbacks.theme.lock().expect("theme mutex poisoned") = named.clone();
        events.notify_theme_changed(named.clone());
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(
            first.unwrap(),
            v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Default,
                variant: v01::ThemeVariant::Light,
            }
        );
        assert_eq!(second.unwrap(), named);
    }

    #[test]
    fn native_preimage_subscription_emits_current_then_notified_value() {
        let (callbacks, events, platform) = event_platform();
        let key = vec![7; 32];
        callbacks
            .preimages
            .lock()
            .expect("preimage map mutex poisoned")
            .push((key.clone(), Some(vec![1, 2, 3])));
        let mut stream = platform.lookup_preimage(key.clone());

        let first = futures::executor::block_on(stream.next()).unwrap();
        events.notify_preimage_changed(&key, Some(vec![4, 5, 6]));
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(second.unwrap(), Some(vec![4, 5, 6]));
    }

    #[test]
    fn native_chat_room_subscription_emits_current_then_notified_replacement() {
        let (callbacks, events, _platform) = event_platform();
        let platform = ChatCallbackPlatform {
            chat: callbacks,
            events: events.clone(),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        let mut stream = truapi_platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);

        let first = ready_rooms(stream.as_mut(), "initial room list");
        events.notify_chat_rooms_changed(vec![v01::ChatRoom {
            room_id: "support".to_string(),
            participating_as: v01::ChatRoomParticipation::Bot,
        }]);
        let second = futures::executor::block_on(stream.next())
            .unwrap()
            .expect("replacement room list");

        assert!(first.rooms.is_empty());
        assert_eq!(second.rooms.len(), 1);
        assert_eq!(second.rooms[0].room_id, "support");
        assert_eq!(
            second.rooms[0].participating_as,
            v01::ChatRoomParticipation::Bot
        );
    }

    /// What a room-list subscription yields: a replacement, or the host's
    /// failure to produce one.
    type RoomListItem = Result<v01::HostChatListSubscribeItem, v01::GenericError>;

    /// Reads a room list the subscription has already emitted.
    ///
    /// Polled without blocking on purpose: both the eager snapshot and a
    /// notified replacement are sent before this runs, so a subscription that
    /// stopped emitting one fails here rather than parking on a live sender
    /// and timing the job out.
    fn ready_rooms(
        mut rooms: core::pin::Pin<&mut (dyn futures::Stream<Item = RoomListItem> + Send)>,
        what: &str,
    ) -> v01::HostChatListSubscribeItem {
        let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
        match rooms.as_mut().poll_next(&mut cx) {
            core::task::Poll::Ready(Some(Ok(item))) => item,
            other => panic!("{what} must be ready, got {other:?}"),
        }
    }

    #[test]
    fn native_chat_adapter_forwards_every_message_variant() {
        let callbacks = Arc::new(EventCallbacks::new());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: Arc::new(NativeEventBus::default()),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        let reaction = v01::ChatReaction {
            message_id: "message-1".to_string(),
            emoji: "\u{1f3b2}".to_string(),
        };
        // Every variant the protocol advertises reaches the host, so a product
        // can post the `Actions` that `action_subscribe` later reports back.
        // An added variant stops `validate_chat_message_content` compiling,
        // which is what forces this list to be revisited.
        let variants = [
            v01::ChatMessageContent::Text {
                text: "hello".to_string(),
            },
            v01::ChatMessageContent::RichText(v01::ChatRichText {
                text: None,
                media: Vec::new(),
            }),
            v01::ChatMessageContent::Actions(v01::ChatActions {
                text: None,
                actions: Vec::new(),
                layout: v01::ChatActionLayout::Column,
            }),
            v01::ChatMessageContent::File(v01::ChatFile {
                url: "https://example.invalid/f".to_string(),
                file_name: "f".to_string(),
                mime_type: "text/plain".to_string(),
                size_bytes: 1,
                text: None,
            }),
            v01::ChatMessageContent::Reaction(reaction.clone()),
            v01::ChatMessageContent::ReactionRemoved(reaction),
            v01::ChatMessageContent::Custom(v01::ChatCustomMessage {
                message_type: "vote".to_string(),
                payload: vec![1, 2],
            }),
        ];

        for payload in &variants {
            futures::executor::block_on(truapi_platform::ChatPlatform::post_chat_message(
                &platform,
                &product,
                v01::HostChatPostMessageRequest {
                    room_id: "support".to_string(),
                    payload: payload.clone(),
                },
            ))
            .unwrap_or_else(|error| panic!("{payload:?} must reach the host: {error:?}"));
        }

        let posted = callbacks
            .chat_posted
            .lock()
            .expect("posted messages mutex poisoned")
            .clone();
        // Asserts the room alongside the content: routing the room correctly
        // for `Text` while misrouting the rest must not pass.
        assert_eq!(
            posted,
            variants
                .iter()
                .map(|content| ("support".to_string(), content.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_posted_action_set_round_trips_to_the_product_that_posted_it() {
        // The loop the widened variant set exists to enable: a product posts
        // `Actions`, a user triggers one, and the product reads the trigger
        // back. The halves travel different paths -- `post_message` through the
        // chat adapter, `ActionTriggered` through the connection -- so this
        // covers what the core owns: that an `Actions` set reaches the host,
        // and that the trigger naming the returned id arrives unaltered.
        // Reusing that id is the host's half of the contract, documented on
        // `NativeChatCallbacks::post_message` and not enforceable here.
        let callbacks = Arc::new(EventCallbacks::new());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: Arc::new(NativeEventBus::default()),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        let connection = crate::runtime::ActionChannel::chat();

        let posted = futures::executor::block_on(truapi_platform::ChatPlatform::post_chat_message(
            &platform,
            &product,
            v01::HostChatPostMessageRequest {
                room_id: "support".to_string(),
                payload: v01::ChatMessageContent::Actions(v01::ChatActions {
                    text: Some("pick one".to_string()),
                    actions: vec![v01::ChatAction {
                        action_id: "approve".to_string(),
                        title: "Approve".to_string(),
                    }],
                    layout: v01::ChatActionLayout::Column,
                }),
            },
        ))
        .expect("an action set must reach the host");

        let mut actions = connection.subscribe::<truapi::latest::GenericError>();
        connection
            .publish(truapi::versioned::chat::HostChatActionSubscribeItem::V1(
                v01::HostChatActionSubscribeItem {
                    room_id: "support".to_string(),
                    peer: "alice".to_string(),
                    payload: v01::ChatActionPayload::ActionTriggered(v01::ActionTrigger {
                        message_id: posted.message_id.clone(),
                        action_id: "approve".to_string(),
                        payload: None,
                    }),
                },
            ))
            .expect("a trigger on a live subscription must be delivered");

        let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
        let delivered = match actions.poll_next_unpin(&mut cx) {
            core::task::Poll::Ready(Some(item)) => item,
            other => panic!("a published trigger must be ready, got {other:?}"),
        };
        let Ok(truapi::versioned::chat::HostChatActionSubscribeItem::V1(delivered)) = delivered
        else {
            panic!("expected a chat action item")
        };
        let v01::ChatActionPayload::ActionTriggered(trigger) = delivered.payload else {
            panic!(
                "expected an ActionTriggered payload, got {:?}",
                delivered.payload
            );
        };

        // The action set really reached the host, in the room it named.
        assert_eq!(
            callbacks
                .chat_posted
                .lock()
                .expect("posted messages mutex poisoned")
                .len(),
            1
        );
        // The id the product must match on to find the message it posted.
        assert_eq!(trigger.message_id, posted.message_id);
        assert_eq!(trigger.action_id, "approve");
    }

    #[test]
    fn a_renderer_action_reaches_the_product_that_rendered_it() {
        // The channel is execution-scoped, so the admin handle built from this
        // execution reads what the execution published.
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
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
    fn native_chat_adapter_surfaces_a_message_rejection() {
        let callbacks = Arc::new(EventCallbacks::new());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: Arc::new(NativeEventBus::default()),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        *callbacks
            .chat_post_rejection
            .lock()
            .expect("post rejection mutex poisoned") =
            Some("cannot render a file card".to_string());

        let error = futures::executor::block_on(truapi_platform::ChatPlatform::post_chat_message(
            &platform,
            &product,
            v01::HostChatPostMessageRequest {
                room_id: "support".to_string(),
                payload: v01::ChatMessageContent::File(v01::ChatFile {
                    url: "https://example.invalid/f".to_string(),
                    file_name: "f".to_string(),
                    mime_type: "text/plain".to_string(),
                    size_bytes: 1,
                    text: None,
                }),
            },
        ))
        .expect_err("a host rejection must not be reported as a stored message");

        // Declining a variant is how a host that cannot render one opts out,
        // so a swallowed rejection would hand the product a message id for
        // something that was never persisted.
        assert_eq!(
            error,
            v01::HostChatPostMessageError::Unknown {
                reason: "cannot render a file card".to_string(),
            }
        );
        assert!(
            callbacks
                .chat_posted
                .lock()
                .expect("posted messages mutex poisoned")
                .is_empty()
        );
    }

    #[test]
    fn native_chat_adapter_surfaces_a_bot_registration_rejection() {
        let callbacks = Arc::new(EventCallbacks::new());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: Arc::new(NativeEventBus::default()),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        *callbacks
            .chat_bot_rejection
            .lock()
            .expect("bot rejection mutex poisoned") = Some("keychain locked".to_string());

        let error = futures::executor::block_on(truapi_platform::ChatPlatform::register_chat_bot(
            &platform,
            &product,
            v01::HostChatRegisterBotRequest {
                bot_id: "flipper".to_string(),
                name: "Flipper".to_string(),
                icon: String::new(),
            },
        ))
        .expect_err("a host rejection must not be reported as a successful registration");

        // A swallowed rejection would reach the product as `New` for a bot
        // that does not exist.
        assert_eq!(
            error,
            v01::HostChatRegisterBotError::Unknown {
                reason: "keychain locked".to_string(),
            }
        );
        assert!(
            callbacks
                .chat_registered_bots
                .lock()
                .expect("registered bots mutex poisoned")
                .is_empty()
        );
    }

    #[test]
    fn native_chat_adapter_preserves_bot_status_and_leaves_rooms_alone() {
        let callbacks = Arc::new(EventCallbacks::new());
        let events = Arc::new(NativeEventBus::default());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: events.clone(),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        let request = v01::HostChatRegisterBotRequest {
            bot_id: "flipper".to_string(),
            name: "Flipper".to_string(),
            icon: String::new(),
        };

        let mut rooms = truapi_platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);
        assert!(
            ready_rooms(rooms.as_mut(), "initial room list")
                .rooms
                .is_empty()
        );

        let registered = futures::executor::block_on(
            truapi_platform::ChatPlatform::register_chat_bot(&platform, &product, request.clone()),
        )
        .unwrap();

        *callbacks
            .chat_bot_status
            .lock()
            .expect("bot status mutex poisoned") = v01::ChatBotRegistrationStatus::Exists;
        let existing = futures::executor::block_on(
            truapi_platform::ChatPlatform::register_chat_bot(&platform, &product, request),
        )
        .unwrap();

        assert_eq!(registered.status, v01::ChatBotRegistrationStatus::New);
        assert_eq!(existing.status, v01::ChatBotRegistrationStatus::Exists);
        assert_eq!(
            callbacks
                .chat_registered_bots
                .lock()
                .expect("registered bots mutex poisoned")
                .as_slice(),
            &[
                ("flipper".to_string(), "Flipper".to_string(), String::new()),
                ("flipper".to_string(), "Flipper".to_string(), String::new()),
            ]
        );

        // Registering a bot is not a room change. Polled without blocking so an
        // unexpected replacement fails instead of parking on a live sender.
        let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(
            rooms.as_mut().poll_next(&mut cx),
            core::task::Poll::Pending
        ));

        // Still live for genuine room changes.
        events.notify_chat_rooms_changed(vec![v01::ChatRoom {
            room_id: "support".to_string(),
            participating_as: v01::ChatRoomParticipation::Bot,
        }]);
        assert_eq!(
            ready_rooms(rooms.as_mut(), "a genuine room change")
                .rooms
                .len(),
            1
        );
    }

    #[test]
    fn native_chat_adapter_preserves_room_status_and_message_room() {
        let callbacks = Arc::new(EventCallbacks::new());
        let platform = ChatCallbackPlatform {
            chat: callbacks.clone(),
            events: Arc::new(NativeEventBus::default()),
        };
        let product = ProductContext::new_with_execution(
            "chat.dot".to_string(),
            ProductExecutionKind::Worker,
        )
        .unwrap();
        let request = v01::HostChatCreateRoomRequest {
            room_id: "support".to_string(),
            name: "Support".to_string(),
            icon: String::new(),
        };
        let mut rooms = truapi_platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);
        assert!(
            ready_rooms(rooms.as_mut(), "initial room list")
                .rooms
                .is_empty()
        );

        let created = futures::executor::block_on(truapi_platform::ChatPlatform::create_chat_room(
            &platform,
            &product,
            request.clone(),
        ))
        .unwrap();
        let updated_rooms = ready_rooms(rooms.as_mut(), "the created room replacement");
        *callbacks
            .chat_room_status
            .lock()
            .expect("room status mutex poisoned") = v01::ChatRoomRegistrationStatus::Exists;
        let existing = futures::executor::block_on(
            truapi_platform::ChatPlatform::create_chat_room(&platform, &product, request),
        )
        .unwrap();
        let posted = futures::executor::block_on(truapi_platform::ChatPlatform::post_chat_message(
            &platform,
            &product,
            v01::HostChatPostMessageRequest {
                room_id: "second-room".to_string(),
                payload: v01::ChatMessageContent::Text {
                    text: "Echo: hello".to_string(),
                },
            },
        ))
        .unwrap();

        assert_eq!(created.status, v01::ChatRoomRegistrationStatus::New);
        assert_eq!(updated_rooms.rooms.len(), 1);
        assert_eq!(updated_rooms.rooms[0].room_id, "support");
        assert_eq!(existing.status, v01::ChatRoomRegistrationStatus::Exists);
        assert_eq!(posted.message_id, "message-1");
        assert_eq!(
            callbacks
                .chat_created_rooms
                .lock()
                .expect("created rooms mutex poisoned")
                .as_slice(),
            &[
                ("support".to_string(), "Support".to_string(), String::new()),
                ("support".to_string(), "Support".to_string(), String::new()),
            ]
        );
        assert_eq!(
            callbacks
                .chat_posted
                .lock()
                .expect("posted messages mutex poisoned")
                .as_slice(),
            &[(
                "second-room".to_string(),
                v01::ChatMessageContent::Text {
                    text: "Echo: hello".to_string(),
                },
            )]
        );
    }

    #[test]
    fn native_chain_provider_forwards_send_response_and_close() {
        let (callbacks, events, platform) = event_platform();
        *callbacks.chain_id.lock().expect("chain id mutex poisoned") = Some(42);
        let genesis = [9; 32];

        let connection = futures::executor::block_on(ChainProvider::connect(&platform, genesis))
            .expect("chain connection should open");
        connection.send(r#"{"jsonrpc":"2.0","id":1}"#.to_string());
        let mut responses = connection.responses();
        events.notify_chain_response(42, r#"{"jsonrpc":"2.0","id":1,"result":true}"#.to_string());
        let response = futures::executor::block_on(responses.next()).unwrap();
        drop(responses);
        drop(connection);

        assert_eq!(
            callbacks
                .chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .as_slice(),
            &[genesis.to_vec()]
        );
        assert_eq!(
            callbacks
                .chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .as_slice(),
            &[(42, r#"{"jsonrpc":"2.0","id":1}"#.to_string())]
        );
        assert_eq!(response, r#"{"jsonrpc":"2.0","id":1,"result":true}"#);
        assert_eq!(
            callbacks
                .chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .as_slice(),
            &[42]
        );
    }

    #[test]
    fn product_execution_routes_chain_events_to_shared_and_scoped_services() {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::new()),
                None,
                None,
                native_execution_config("chain.dot", ProductExecutionKind::App),
            )
            .expect("App execution should open");
        let mut shared_responses = host.events.register_chain(41);
        let mut scoped_responses = execution.events.register_chain(41);
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
    fn runtime_config_reports_the_declared_host_platform() {
        let resolved = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            host_platform: HostPlatform::Ios,
            ..native_host_runtime_config()
        })
        .expect("config is valid");

        assert_eq!(resolved.signing.host.host_info.platform, HostPlatform::Ios);
    }

    #[test]
    fn runtime_config_rejects_wrong_size_genesis_hash() {
        let err = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            people_chain_genesis_hash: vec![0; 31],
            ..native_host_runtime_config()
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidPeopleChainGenesisHash { actual: 31 }
        ));
    }

    #[test]
    fn each_configured_genesis_hash_reaches_its_own_field() {
        // Three adjacent `Vec<u8>` feeding a positional constructor:
        // transposing any two compiles and, without this, passes.
        let resolved = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            people_chain_genesis_hash: vec![0xa1; 32],
            bulletin_chain_genesis_hash: vec![0xb2; 32],
            asset_hub_chain_genesis_hash: vec![0xc3; 32],
            ..native_host_runtime_config()
        })
        .expect("config is valid");

        assert_eq!(
            (
                resolved.signing.people_chain_genesis_hash,
                resolved.signing.bulletin_chain_genesis_hash,
                resolved.signing.asset_hub_chain_genesis_hash,
            ),
            ([0xa1; 32], [0xb2; 32], [0xc3; 32]),
        );
    }

    #[test]
    fn a_wrong_size_asset_hub_genesis_hash_is_rejected_as_its_own_field() {
        // An empty vec must be an error, never a silent all-zero "no Asset
        // Hub".
        for len in [0usize, 31, 33] {
            let err = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
                asset_hub_chain_genesis_hash: vec![0; len],
                ..native_host_runtime_config()
            })
            .unwrap_err();

            assert!(
                matches!(
                    err,
                    NativeRuntimeConfigError::InvalidAssetHubChainGenesisHash { actual }
                        if actual == len as u64
                ),
                "{len}-byte Asset Hub hash reported as {err:?}"
            );
        }
    }

    #[test]
    fn runtime_config_rejects_a_network_suffix_that_is_not_a_bare_tld() {
        // The suffix ends every reserved derivation (`peopl.<suffix>`), so a
        // shell passing the dotted form would silently derive a stranger.
        let err = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            network_suffix: ".paseo".to_string(),
            ..native_host_runtime_config()
        })
        .unwrap_err();
        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidNetworkSuffix { network_suffix } if network_suffix == ".paseo"
        ));
    }

    #[test]
    fn product_execution_config_rejects_empty_product_id() {
        let err = ProductContext::try_from(NativeProductExecutionConfig {
            product_id: " ".to_string(),
            ..native_execution_config("app.dot", ProductExecutionKind::App)
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::EmptyField { field } if field == "product_id"
        ));
    }

    #[test]
    fn runtime_config_rejects_relative_host_icon() {
        let err = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            host_icon: Some("/dotli.png".to_string()),
            ..native_host_runtime_config()
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidHostIcon { .. }
        ));
    }

    #[test]
    fn runtime_config_rejects_non_https_host_icon() {
        let err = NativeResolvedHostRuntimeConfig::try_from(NativeHostRuntimeConfig {
            host_icon: Some("http://localhost:3000/dotli.png".to_string()),
            ..native_host_runtime_config()
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InsecureHostIcon { scheme } if scheme == "http"
        ));
    }

    /// Calling `start_ws_bridge` twice on the same product execution
    /// without an intervening `stop_ws_bridge` is a hard error. The bridge
    /// is single-instance per execution, so the second start must surface
    /// `AlreadyRunning` rather than silently leaking a worker thread.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn start_ws_bridge_twice_returns_already_running() {
        struct Noop;
        #[async_trait::async_trait]
        impl HostCallbacks for Noop {
            async fn pick_chat_files(
                &self,
                _: NativeChatFilePickRequest,
            ) -> Result<Vec<NativeChatPickedFile>, HostRejection> {
                unavailable_chat_files()
            }
            async fn read_chat_file(
                &self,
                _: String,
                _: u64,
                _: u32,
            ) -> Result<Vec<u8>, HostRejection> {
                unavailable_chat_files()
            }
            async fn release_chat_file(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn begin_chat_file_export(
                &self,
                _: NativeChatFileExportRequest,
            ) -> Result<Option<String>, HostRejection> {
                unavailable_chat_files()
            }
            async fn write_chat_file_export(
                &self,
                _: String,
                _: u64,
                _: Vec<u8>,
            ) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn finish_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn cancel_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn identity_username_candidates(
                &self,
                _: String,
                _: Vec<u8>,
            ) -> Result<Vec<Vec<u8>>, HostRejection> {
                Err(HostRejection::Rejected {
                    reason: "no identity provider in fixture".into(),
                })
            }
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn worker_demand_changed(&self, _product_id: String, _transition: WorkerTransition) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: v01::HostPushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn device_permission_status(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<NativeDevicePermissionStatus, HostRejection> {
                Ok(NativeDevicePermissionStatus::NotApplicable)
            }
            async fn remote_permission(
                &self,
                _request: v01::RemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn allowed_hop_endpoints(
                &self,
                _: Vec<u8>,
            ) -> Result<Vec<String>, HostRejection> {
                Ok(Vec::new())
            }
            fn hop_connect(&self, _: Vec<u8>, _: String) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: UserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
                Ok(v01::HostThemeSubscribeItem {
                    name: v01::ThemeName::Default,
                    variant: v01::ThemeVariant::Light,
                })
            }
            fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
                Ok(v01::HostLocaleSubscribeItem {
                    language_tag: "en".to_string(),
                })
            }
            async fn feature_supported(
                &self,
                _request: v01::HostFeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, HostRejection> {
                Ok(truapi_platform::HostChainSet {
                    network: "paseo".to_string(),
                    chains: Vec::new(),
                })
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let execution = native_product_execution(Arc::new(Noop), "dotli.dot");
        let _first = execution
            .start_ws_bridge(0)
            .expect("first start must succeed");
        let err = execution
            .start_ws_bridge(0)
            .expect_err("second start must error");
        assert!(matches!(err, WsBridgeStartError::AlreadyRunning));
        execution.stop_ws_bridge();
    }

    /// A permission callback suspends while awaiting the user's decision and
    /// holds no executor worker, so an unrelated request on the same
    /// connection still round-trips while the decision is pending.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn pending_permission_decision_does_not_stall_bridge() {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::SinkExt;
        use parity_scale_codec::Decode;
        use tokio_tungstenite::tungstenite::Message as WsMessage;

        use crate::frame::{Payload, ProtocolMessage, request_ids};

        /// `device_permission` stays pending until the test sends on
        /// `release`; every other callback is a trivial success.
        struct GatedPermissionCallbacks {
            permission_entered: Arc<AtomicBool>,
            release: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>,
        }

        #[async_trait::async_trait]
        impl HostCallbacks for GatedPermissionCallbacks {
            async fn pick_chat_files(
                &self,
                _: NativeChatFilePickRequest,
            ) -> Result<Vec<NativeChatPickedFile>, HostRejection> {
                unavailable_chat_files()
            }
            async fn read_chat_file(
                &self,
                _: String,
                _: u64,
                _: u32,
            ) -> Result<Vec<u8>, HostRejection> {
                unavailable_chat_files()
            }
            async fn release_chat_file(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn begin_chat_file_export(
                &self,
                _: NativeChatFileExportRequest,
            ) -> Result<Option<String>, HostRejection> {
                unavailable_chat_files()
            }
            async fn write_chat_file_export(
                &self,
                _: String,
                _: u64,
                _: Vec<u8>,
            ) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn finish_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn cancel_chat_file_export(&self, _: String) -> Result<(), HostRejection> {
                unavailable_chat_files()
            }
            async fn identity_username_candidates(
                &self,
                _: String,
                _: Vec<u8>,
            ) -> Result<Vec<Vec<u8>>, HostRejection> {
                Err(HostRejection::Rejected {
                    reason: "no identity provider in permission fixture".into(),
                })
            }
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn worker_demand_changed(&self, _product_id: String, _transition: WorkerTransition) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: v01::HostPushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<bool, HostRejection> {
                self.permission_entered.store(true, Ordering::SeqCst);
                self.release
                    .lock()
                    .await
                    .recv()
                    .await
                    .expect("release signal");
                Ok(true)
            }
            async fn device_permission_status(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<NativeDevicePermissionStatus, HostRejection> {
                Ok(NativeDevicePermissionStatus::NotApplicable)
            }
            async fn remote_permission(
                &self,
                _request: v01::RemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn allowed_hop_endpoints(
                &self,
                _: Vec<u8>,
            ) -> Result<Vec<String>, HostRejection> {
                Ok(Vec::new())
            }
            fn hop_connect(&self, _: Vec<u8>, _: String) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: UserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
                Ok(v01::HostThemeSubscribeItem {
                    name: v01::ThemeName::Default,
                    variant: v01::ThemeVariant::Light,
                })
            }
            fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
                Ok(v01::HostLocaleSubscribeItem {
                    language_tag: "en".to_string(),
                })
            }
            async fn feature_supported(
                &self,
                _request: v01::HostFeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(true)
            }
            fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, HostRejection> {
                Ok(truapi_platform::HostChainSet {
                    network: "paseo".to_string(),
                    chains: Vec::new(),
                })
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let (release_tx, release_rx) = tokio::sync::mpsc::channel::<()>(1);
        let permission_entered = Arc::new(AtomicBool::new(false));
        let execution = native_product_execution(
            Arc::new(GatedPermissionCallbacks {
                permission_entered: permission_entered.clone(),
                release: tokio::sync::Mutex::new(release_rx),
            }),
            "dotli.dot",
        );
        let endpoint = execution.start_ws_bridge(0).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let permission_ids =
            request_ids("permissions_request_device_permission").expect("known request method");
        let feature_ids = request_ids("system_feature_supported").expect("known request method");
        let (feature_response, permission_response) = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

            let permission_value = truapi::versioned::permissions::HostDevicePermissionRequest::V1(
                v01::HostDevicePermissionRequest::Camera,
            )
            .encode();
            let permission_frame = ProtocolMessage {
                request_id: "p:permission".into(),
                payload: Payload {
                    trait_id: permission_ids.trait_id,
                    method_id: permission_ids.method_id,
                    message_type: crate::frame::MESSAGE_TYPE_REQUEST,
                    value: permission_value,
                },
            };
            ws.send(WsMessage::Binary(permission_frame.encode()))
                .await
                .expect("send device permission");

            // Wait until the permission callback is blocked on the decision.
            for _ in 0..1000 {
                if permission_entered.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                permission_entered.load(Ordering::SeqCst),
                "permission callback was not invoked"
            );

            let feature_value = truapi::versioned::system::HostFeatureSupportedRequest::V1(
                v01::HostFeatureSupportedRequest::Chain {
                    genesis_hash: vec![0u8; 32],
                },
            )
            .encode();
            let feature_frame = ProtocolMessage {
                request_id: "p:feature".into(),
                payload: Payload {
                    trait_id: feature_ids.trait_id,
                    method_id: feature_ids.method_id,
                    message_type: crate::frame::MESSAGE_TYPE_REQUEST,
                    value: feature_value,
                },
            };
            ws.send(WsMessage::Binary(feature_frame.encode()))
                .await
                .expect("send feature_supported");

            let feature_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("feature_supported must answer while the permission decision is pending");

            release_tx
                .send(())
                .await
                .expect("release permission callback");
            let permission_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("released permission must answer");

            (feature_response, permission_response)
        });

        assert_eq!(feature_response.request_id, "p:feature");
        assert_eq!(feature_response.payload.trait_id, feature_ids.trait_id);
        assert_eq!(feature_response.payload.method_id, feature_ids.method_id);

        assert_eq!(permission_response.request_id, "p:permission");
        assert_eq!(
            permission_response.payload.trait_id,
            permission_ids.trait_id
        );
        assert_eq!(
            permission_response.payload.method_id,
            permission_ids.method_id
        );
        assert_eq!(
            permission_response.payload.message_type,
            crate::frame::MESSAGE_TYPE_RESPONSE
        );
        let expected_permission: Result<
            truapi::versioned::permissions::HostDevicePermissionResponse,
            truapi::CallError<truapi::versioned::permissions::HostDevicePermissionError>,
        > = Ok(
            truapi::versioned::permissions::HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse { granted: true },
            ),
        );
        assert_eq!(
            permission_response.payload.value,
            expected_permission.encode()
        );

        execution.stop_ws_bridge();
    }

    fn native_host_runtime_no_session() -> Arc<NativeTrUApiHostRuntime> {
        let mut config = native_host_runtime_config();
        config.local_session_secret = None;
        config.local_session_lite_username = None;
        NativeTrUApiHostRuntime::with_runtime_config(Arc::new(EventCallbacks::new()), config, None)
            .expect("host runtime config should be valid")
    }

    #[test]
    fn handle_sso_request_rejects_undecodable_bytes() {
        let runtime = native_host_runtime_no_session();
        let result =
            futures::executor::block_on(runtime.handle_sso_request(vec![0xFF, 0xFF, 0xFF]));
        assert!(result.is_err(), "garbage bytes must be a decode error");
    }

    #[test]
    fn handle_sso_request_reports_disconnect_as_marker() {
        use crate::host_logic::sso::messages::{RemoteMessage, RemoteMessageData, v1};
        use parity_scale_codec::Encode;
        let runtime = native_host_runtime_no_session();
        let disconnected = RemoteMessage {
            message_id: "m1".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };
        let outcome =
            futures::executor::block_on(runtime.handle_sso_request(disconnected.encode()))
                .expect("decodable message");
        assert!(matches!(outcome, SsoRequestOutcome::Disconnected));
    }

    #[test]
    fn handle_sso_request_reencodes_a_request_response() {
        use crate::host_logic::sso::messages::{
            ProductSubtreeRequest, RemoteMessage, RemoteMessageData, v1,
        };
        use parity_scale_codec::{Decode, Encode};
        // The default test config carries a local session secret, so the
        // runtime is activated at construction.
        let runtime = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let request = RemoteMessage {
            message_id: "m9".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeRequest(
                ProductSubtreeRequest {
                    product_id: "browse.dot".to_string(),
                },
            )),
        };
        let outcome = futures::executor::block_on(runtime.handle_sso_request(request.encode()))
            .expect("decodable message");
        let SsoRequestOutcome::Response { message } = outcome else {
            panic!("expected a response outcome");
        };
        let response =
            RemoteMessage::decode(&mut message.as_slice()).expect("valid response encoding");
        assert_eq!(response.message_id, "m9:response");
        let RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(payload)) =
            response.data
        else {
            panic!("expected a product subtree response payload");
        };
        assert_eq!(payload.responding_to, "m9");
        assert!(payload.payload.is_ok());
    }

    #[test]
    fn prepare_disconnect_request_round_trips_with_fresh_ids() {
        use crate::host_logic::sso::messages::{RemoteMessage, RemoteMessageData, v1};
        use parity_scale_codec::Decode;
        let runtime = native_host_runtime_no_session();
        let bytes = runtime.prepare_disconnect_request();
        let message = RemoteMessage::decode(&mut bytes.as_slice()).expect("valid encoding");
        assert_eq!(message.message_id.len(), 8, "opaque nanoid message id");
        assert!(matches!(
            message.data,
            RemoteMessageData::V1(v1::RemoteMessage::Disconnected)
        ));
        let second = RemoteMessage::decode(&mut runtime.prepare_disconnect_request().as_slice())
            .expect("valid encoding");
        assert_ne!(
            message.message_id, second.message_id,
            "each disconnect message carries its own id"
        );
    }

    #[test]
    fn bytes32_widens_to_plain_bytes_on_the_wire() {
        let mut buf = Vec::new();
        <Bytes32 as uniffi::Lower<truapi::UniFfiTag>>::write([7; 32], &mut buf);
        assert_eq!(buf[..4], 32i32.to_be_bytes());
        assert_eq!(buf[4..], [7; 32]);
    }

    #[test]
    fn bytes32_lift_rejects_wrong_length() {
        let mut buf = Vec::new();
        <Vec<u8> as uniffi::Lower<crate::UniFfiTag>>::write(vec![7; 31], &mut buf);
        assert!(
            <Bytes32 as uniffi::Lift<truapi::UniFfiTag>>::try_read(&mut buf.as_slice()).is_err()
        );
    }

    #[test]
    fn bytes32_fields_survive_the_ffi_roundtrip() {
        let review = UserConfirmationReview::CreateTransaction(
            CreateTransactionReview::LegacyAccount(LegacyAccountTxPayload {
                signer: [13; 32],
                genesis_hash: [14; 32],
                call_data: vec![15],
                extensions: vec![],
                tx_ext_version: 0,
            }),
        );

        let mut buf = Vec::new();
        <UserConfirmationReview as uniffi::Lower<crate::UniFfiTag>>::write(
            review.clone(),
            &mut buf,
        );
        let lifted = <UserConfirmationReview as uniffi::Lift<crate::UniFfiTag>>::try_read(
            &mut buf.as_slice(),
        )
        .expect("review must lift back");
        assert_eq!(lifted, review);
    }

    /// Drives the whole native chain for a status read: foreign callback,
    /// `CallbackPlatform`, the per-execution connection adapters, and the
    /// permission service. Nothing else covers that path, and the adapter is
    /// per execution rather than per host runtime, so a host-level installer
    /// would silently answer from the wrong object.
    #[test]
    fn a_native_status_read_follows_the_os_gate() {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
            None,
        )
        .expect("host runtime config should be valid");
        let execution = host
            .open_product_execution(
                Arc::new(EventCallbacks::refusing(
                    v01::HostDevicePermissionRequest::Camera,
                )),
                None,
                None,
                native_execution_config("gated.dot", ProductExecutionKind::App),
            )
            .expect("execution should open");

        let camera = futures::executor::block_on(execution.permission_authorization_status(
            PermissionAuthorizationRequest::Device(v01::HostDevicePermissionRequest::Camera),
        ))
        .expect("status read");
        let microphone = futures::executor::block_on(execution.permission_authorization_status(
            PermissionAuthorizationRequest::Device(v01::HostDevicePermissionRequest::Microphone),
        ))
        .expect("status read");

        // Camera is refused by the OS; the microphone has no OS gate and no
        // stored decision, so its question is still open.
        assert_eq!(
            (camera, microphone),
            (
                PermissionAuthorizationStatus::Denied,
                PermissionAuthorizationStatus::NotDetermined,
            )
        );
    }
}
