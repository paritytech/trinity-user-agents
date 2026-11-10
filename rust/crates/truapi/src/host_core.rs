//! Stable host-embedding API for the TrUAPI server runtime.
//!
//! `ProductRuntime` is the target-neutral boundary embedders should use.
//! Platform adapters provide:
//! - a [`crate::platform::Platform`] implementation for host callbacks,
//! - a task [`Spawner`] for runtime-owned async work,
//! - a [`FrameSink`] for outgoing protocol frames.
//!
//! Target-specific shells such as wasm-bindgen, iOS FFI, or desktop IPC should
//! keep their conversion code outside this module.

use std::{
	collections::HashMap,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
	time::Duration,
};

use crate::platform::{
	BalancePlatform, ChatPlatform, ContactsPlatform, CoreAdmin, ExecutableKind, ExpandedCardHost,
	FundingPlatform, GamePlatform, PairingHostAdmin, PairingHostConfig, PaymentPlatform,
	PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionStatusHost, Platform,
	PocketPlatform, ProductContext, ProductId, ScannerPlatform, SigningHostConfig, TopUpPlatform,
};
use futures::{
	FutureExt, StreamExt,
	future::{AbortHandle, Abortable, BoxFuture},
	pin_mut,
};
use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use tracing::{instrument, warn};
use truapi::{CallContext, CancellationReason, latest, v01};

use crate::{
	frame::ProtocolMessage,
	host_internal::sso_messages::{RemoteMessage, SsoRequestOutcome},
	host_logic::worker::WorkerLedger,
	runtime::{
		ActionChannel, DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT, DevicePairingObserver,
		FundingProviderLink, HostSession, LocalActivation, PairedSsoPeer, PairingHostRole,
		ProductAuthority, ProductConnection, ProductRuntimeHost, ResponderExit, ResumePairingError,
		RuntimeServices, SigningHostRole, SigningHostSsoService, SsoResponderService,
		sso_service::Dispatch,
	},
	subscription::{HostInitiatedSubscriptionManager, Spawner},
	transport::Transport,
	truapi_core::TrUApiCore,
};

/// Outgoing frame sink owned by a host adapter.
///
/// Implementations bridge encoded TrUAPI protocol frames to their target
/// transport: JS callbacks, native callbacks, IPC, channels, or another
/// host-specific mechanism.
pub trait FrameSink: Send + Sync {
	/// Emit one SCALE-encoded [`ProtocolMessage`] frame.
	fn emit_frame(&self, frame: Vec<u8>);
}

/// Dev-only sink that observes host debug events at the core's two frame choke
/// points. A host that does not enable the debugger leaves it unset and the tap
/// is inert. Fire-and-forget by construction: [`DebugSink::emit`] must not block
/// the frame path and must not fail the operation that produced the event, so a
/// slow, absent, or crashed debugger only loses the trace, never a session.
pub trait DebugSink: Send + Sync {
	/// Hand one event to the sink. Serialize and enqueue only; never block.
	fn emit(&self, event: DebugEvent);
}

/// Hand one event to a sink, containing a panic rather than letting it unwind
/// into the frame path that called it.
///
/// `catch_unwind` is a no-op under `panic = "abort"` (the shipping `release`
/// profile, which `codegen` inherits, and `wasm32`, which cannot unwind at all).
/// It is not dead code, because the profiles that *do* unwind are the ones the
/// debugger is used from: the workspace defines no `[profile.dev]`, so `dev`
/// keeps the default `panic = "unwind"`, and the Makefile builds
/// `truapi-host-cli` without `--release`. It also protects any downstream crate
/// that compiles this one under its own unwinding profile.
///
/// No in-process test can prove the protection - Cargo ignores the `panic`
/// setting for test profiles, so a test asserting "the guard saved the dispatch"
/// would pass even with the guard removed. The guard is kept because it costs
/// nothing when nothing panics, not because a test can demonstrate it.
fn emit_debug(sink: Arc<dyn DebugSink>, event: DebugEvent) {
	let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
		sink.emit(event);
		// Dropped INSIDE the guard, which is why this takes the `Arc` by value.
		// The frame path holds its own clone, so when `set_debug_sink` has
		// concurrently replaced the sink this binding is the last reference and
		// the out-of-repo destructor runs here - not at the caller's scope end,
		// where it would unwind into a live dispatch.
		drop(sink);
	}));
	if result.is_err() {
		tracing::warn!("debug sink panicked; frame dropped, dispatch unaffected");
	}
}

/// Identifies which product channel on a host a debug event belongs to, so one
/// debugger app can demultiplex several channels.
#[derive(Clone, PartialEq, Eq)]
pub struct ChannelId(pub String);

/// Direction of a tapped frame relative to the host core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameDirection {
	/// Product to core (inbound to the host).
	In,
	/// Core to product (outbound from the host).
	Out,
}

impl FrameDirection {
	/// The wire direction string, from the **product's** vantage - the vantage
	/// the debugger app and the design doc use: `"out"` = the frame left the
	/// product, `"in"` = it arrived at the product. This is the inverse of the
	/// enum's host-vantage variants (`In` = product to core, i.e. it *left* the
	/// product), so every sink serializes the same product-vantage string
	/// instead of re-deriving (and risking inverting) it.
	pub fn wire_str(self) -> &'static str {
		match self {
			FrameDirection::In => "out",
			FrameDirection::Out => "in",
		}
	}
}

/// One observable host debug event. Frame bytes are the untouched
/// `ProtocolMessage`; the debugger decodes them, so the core never does. The
/// enum leaves room for host-internal events (e.g. SSO) that have no wire frame,
/// so it is `#[non_exhaustive]`: adding a variant is not a breaking change.
#[derive(Clone)]
#[non_exhaustive]
pub enum DebugEvent {
	/// A SCALE wire frame crossing a product channel.
	Frame {
		/// Which product channel on this host.
		channel_id: ChannelId,
		/// Product to core, or core to product.
		dir: FrameDirection,
		/// Untouched encoded `ProtocolMessage` bytes.
		bytes: Vec<u8>,
	},
}

/// Errors returned while routing work through a product runtime.
#[derive(Debug, Clone, Error)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
pub enum ProductRuntimeError {
	/// No connected product runtime is available.
	#[error("product is not connected")]
	NotConnected,
	/// Incoming bytes did not decode as a protocol frame.
	#[error("invalid frame: {reason}")]
	InvalidFrame {
		/// Decode failure reason.
		reason: String,
	},
	/// The connection execution kind does not allow the operation.
	#[error("operation denied for this execution")]
	Denied,
	/// The product connection has already closed.
	#[error("product connection is closed")]
	Closed,
	/// The product or native host did not install the requested surface.
	#[error("operation is unsupported")]
	Unsupported,
	/// The bounded pre-subscription action queue is full.
	#[error("connection action buffer is full")]
	BufferFull,
}

fn product_context(product_id: &ProductId) -> ProductContext {
	ProductContext::new_with_execution(product_id.clone(), ExecutableKind::App)
}

/// A seedless pairing host: the user's keys live in an external wallet reached
/// over the SSO pairing channel.
///
/// Owns the shared services plus pairing-host state. Local-session activation
/// is a signing-host operation and is not present here.
pub struct PairingHostRuntime {
	services: Arc<RuntimeServices>,
	pairing_host: Arc<PairingHostRole>,
}

impl PairingHostRuntime {
	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.pairing_host.set_submit_preimages_locally(local);
	}

	/// Build a long-lived pairing-host runtime around a platform implementation.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.new"))]
	pub fn new<P>(platform: Arc<P>, config: PairingHostConfig, spawner: Spawner) -> Self
	where
		P: Platform + 'static,
	{
		Self::with_platforms(platform, config, spawner, None, None)
	}

	/// Same as [`Self::new`], with the host's chat adapter installed. Passing
	/// `None` leaves the host without the Chat capability, so its products'
	/// chat calls resolve as `Unsupported`.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.with_chat_platform"))]
	pub fn with_chat_platform<P>(
		platform: Arc<P>,
		config: PairingHostConfig,
		spawner: Spawner,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		Self::with_platforms(platform, config, spawner, chat_platform, None)
	}

	/// Same as [`Self::new`], with both optional adapters installed. An omitted
	/// adapter leaves the host without that capability, so its products' calls
	/// to it resolve as `Unsupported`.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.with_platforms"))]
	pub fn with_platforms<P>(
		platform: Arc<P>,
		config: PairingHostConfig,
		spawner: Spawner,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
		contacts_platform: Option<Arc<dyn ContactsPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		let platform: Arc<dyn Platform> = platform;
		let services = RuntimeServices::with_chat_platform(
			platform,
			config.host.host_info.clone(),
			config.people_chain_genesis_hash,
			config.bulletin_chain_genesis_hash,
			config.asset_hub_chain_genesis_hash,
			spawner.clone(),
			chat_platform,
		);
		if let Some(contacts_platform) = contacts_platform {
			services.install_contacts_platform(contacts_platform);
		}
		let pairing_host = PairingHostRole::new(services.clone(), config);
		pairing_host.session().clone().start_session_store_sync(spawner);
		Self { services, pairing_host }
	}

	/// Install the host's [`PermissionStatusHost`], which carries the reasoning
	/// for what it changes.
	///
	/// Set-once, so the capability cannot be swapped under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_permission_status_host"))]
	pub fn set_permission_status_host(&self, host: Arc<dyn PermissionStatusHost>) -> bool {
		self.services.install_permission_status_host(host)
	}

	/// Install the host's [`PocketPlatform`], which owns the card collection.
	///
	/// Set-once, so the collection cannot change hands under a running
	/// product. Returns whether this call installed it. Call it before serving
	/// any product runtime.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_pocket_platform"))]
	pub fn set_pocket_platform(&self, platform: Arc<dyn PocketPlatform>) -> bool {
		self.services.install_pocket_platform(platform)
	}

	/// Install the host's [`FundingPlatform`], the native funding overlay.
	///
	/// Set-once. Returns whether this call installed it. Call it before
	/// serving any product runtime.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_funding_platform"))]
	pub fn set_funding_platform(&self, platform: Arc<dyn FundingPlatform>) -> bool {
		self.services.install_funding_platform(platform)
	}

	/// Open a funding session on the host's own behalf, as the Balance card's
	/// Add and Withdraw do, and show the overlay. Returns the session id, or
	/// `None` when the user dismissed the overlay without starting.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.open_funding"))]
	pub async fn open_funding(
		&self,
		direction: v01::FundingDirection,
		amount: Option<u128>,
	) -> Result<Option<String>, v01::GenericError> {
		self.services.open_host_funding(direction, amount).await
	}

	/// Install the host's [`GamePlatform`], which holds each product's game
	/// reminder.
	///
	/// Set-once, so reminders cannot change hands under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_game_platform"))]
	pub fn set_game_platform(&self, platform: Arc<dyn GamePlatform>) -> bool {
		self.services.install_game_platform(platform)
	}

	/// Install the host's [`TopUpPlatform`], which claims funds into the
	/// balance. Set-once. Returns whether this call installed it.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_top_up_platform"))]
	pub fn set_top_up_platform(&self, platform: Arc<dyn TopUpPlatform>) -> bool {
		self.services.install_top_up_platform(platform)
	}

	/// Install the host's [`PaymentPlatform`], which pays from the user's
	/// balance once the user approves. Set-once; returns whether this call
	/// installed it.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_payment_platform"))]
	pub fn set_payment_platform(&self, platform: Arc<dyn PaymentPlatform>) -> bool {
		self.services.install_payment_platform(platform)
	}

	/// Install the host's [`BalancePlatform`], which shares the user's
	/// balance with products. Set-once; returns whether this call installed
	/// it.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_balance_platform"))]
	pub fn set_balance_platform(&self, platform: Arc<dyn BalancePlatform>) -> bool {
		self.services.install_balance_platform(platform)
	}

	/// Install the host's [`ScannerPlatform`] before serving any product
	/// runtime. Set-once: returns whether this call installed it.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_scanner_platform"))]
	pub fn set_scanner_platform(&self, platform: Arc<dyn ScannerPlatform>) -> bool {
		self.services.install_scanner_platform(platform)
	}

	/// Install the host's [`ContactsPlatform`], which owns the contact list and
	/// draws the picker.
	///
	/// Set-once, so the picker cannot change hands under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_contacts_platform"))]
	pub fn set_contacts_platform(&self, platform: Arc<dyn ContactsPlatform>) -> bool {
		self.services.install_contacts_platform(platform)
	}

	/// Tell the core the host's contacts changed, so no handle resolves from
	/// what it cached before. Call it whenever a contact is removed or blocked;
	/// the next transaction naming a contact reads the list again.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.notify_contacts_changed"))]
	pub fn notify_contacts_changed(&self) {
		self.services.contact_handles.clear();
	}

	/// Build a product-facing runtime from this pairing host.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_runtime"))]
	pub fn product_runtime(
		&self,
		product: ProductContext,
		sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		ProductRuntime::new(
			self.services.clone(),
			self.pairing_host.clone(),
			self.pairing_host.session().clone(),
			product,
			ConnectionAdapters::from_services(&self.services),
			sink,
		)
	}

	/// Build a product-scoped administration handle from this pairing host.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_admin"))]
	pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
		HostAdmin::new(
			self.services.clone(),
			self.pairing_host.clone(),
			self.pairing_host.session().clone(),
			product,
			ConnectionAdapters::from_services(&self.services),
		)
	}

	/// Disconnect the active account-authority session.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.disconnect_session"))]
	pub async fn disconnect_session(&self) {
		self.pairing_host.session().disconnect().await;
	}

	/// Log out and discard the old pairing keypair.
	///
	/// The next product login request generates a fresh pairing identity and
	/// presents a new deeplink suitable for another signing host.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.logout"))]
	pub async fn logout(&self) -> Result<(), v01::GenericError> {
		self.pairing_host.logout_and_reset_pairing().await.map_err(Into::into)
	}

	/// Clear one product's capability state while preserving the active
	/// session and unrelated products.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.clear_product_state", %product_id))]
	pub async fn clear_product_state(
		&self,
		product_id: &ProductId,
	) -> Result<(), v01::GenericError> {
		self.pairing_host.clear_product_state(product_id).await.map_err(Into::into)
	}

	/// Registered providers available for an internal well-known-ring feature.
	pub async fn ring_vrf_providers(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Vec<v01::ProductAccountId>, v01::GenericError> {
		self.pairing_host.ring_vrf_providers(ring).await.map_err(ring_vrf_admin_error)
	}

	/// Current provider selected for an internal well-known-ring feature.
	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Option<v01::ProductAccountId>, v01::GenericError> {
		self.pairing_host
			.selected_ring_vrf_provider(ring)
			.await
			.map_err(ring_vrf_admin_error)
	}

	/// Select a registered provider for an internal well-known-ring feature.
	pub async fn select_ring_vrf_provider(
		&self,
		ring: v01::RingLocation,
		handle: v01::ProductAccountId,
	) -> Result<(), v01::GenericError> {
		self.pairing_host
			.select_ring_vrf_provider(ring, handle)
			.await
			.map_err(ring_vrf_admin_error)
	}

	/// Reference counts per product worker, shared by every connection of
	/// this host.
	pub fn worker_ledger(&self) -> &WorkerLedger {
		&self.services.worker_ledger
	}

	/// Report that the product's worker stopped without being asked to:
	/// it is started again while something holds it, with a growing wait
	/// when it keeps crashing.
	pub fn worker_exited(&self, product_id: &ProductId) {
		self.services.worker_exited(product_id);
	}

	/// Read the active session's X25519 chat identity private key, for hosts
	/// running their own P2P chat channel for the paired identity.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.session_chat_identity_key"))]
	pub fn session_chat_identity_key(&self) -> Option<[u8; 32]> {
		self.pairing_host.session().session_state().current()?.identity_chat_private_key
	}

	/// Read the active session's sr25519 statement-store secret, for hosts
	/// running their own statement-store traffic against the advertised account.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.device_statement_key"))]
	pub fn device_statement_key(&self) -> Option<[u8; 64]> {
		Some(self.pairing_host.session().session_state().current()?.sso?.ss_secret)
	}

	/// Read this device's X25519 encryption secret, for hosts running device
	/// sync. Generated and persisted on first read.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.device_encryption_key"))]
	pub async fn device_encryption_key(&self) -> Result<[u8; 32], v01::GenericError> {
		self.services.device_encryption_secret().await.map_err(Into::into)
	}

	/// Resolve `product_id`'s hard-subtree public key from the cache, the
	/// persisted slot, or the Account Holder. `timeout_ms` bounds that wait.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_subtree_public_key"))]
	pub async fn product_subtree_public_key(
		&self,
		product_id: &ProductId,
		timeout_ms: Option<u32>,
	) -> Result<Option<[u8; 32]>, v01::GenericError> {
		product_subtree_public_key(self.pairing_host.as_ref(), product_id, timeout_ms).await
	}

	/// Clear the canonical paired session and all capability caches/storage
	/// without sending a peer-disconnect notice.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.reset_session_state"))]
	pub async fn reset_session_state(&self) {
		self.pairing_host.session().reset_session_state().await;
	}

	/// Start or join the pairing-host login flow for one product.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.login", %product_id))]
	pub async fn login(
		&self,
		product_id: &ProductId,
	) -> Result<v01::HostRequestLoginResponse, v01::GenericError> {
		let product = product_context(product_id);
		match self.pairing_host.session().request_login(&product).await {
			Ok(response) => Ok(response),
			Err(error) => Err(v01::GenericError { reason: pairing_login_error_reason(error) }),
		}
	}

	/// Cancel an in-flight SSO pairing request. A no-op when no pairing is
	/// active.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.cancel_pairing"))]
	pub fn cancel_pairing(&self) {
		self.pairing_host.session().cancel_login();
	}

	/// Activate a canonical session blob supplied by an external encrypted
	/// session owner without writing the blob to core storage.
	///
	/// Success means decoding, username resolution, replacement fencing, and
	/// connected-session installation have completed.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.activate_external_session"))]
	pub async fn activate_external_session(&self, blob: &[u8]) -> Result<(), v01::GenericError> {
		self.pairing_host
			.session()
			.activate_external_session(blob)
			.await
			.map_err(Into::into)
	}

	/// Await restoration of the persisted auth-session blob.
	///
	/// Success means decoding, username resolution, stale-read fencing, and
	/// connected-session installation have completed, so product frames may
	/// immediately use the restored authority session.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.activate_stored_session"))]
	pub async fn activate_stored_session(&self) -> Result<(), v01::GenericError> {
		self.pairing_host.session().activate_stored_session().await.map_err(Into::into)
	}

	/// Notify the pairing runtime that the persisted auth-session blob may
	/// have changed and should be re-read.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.notify_session_store_changed"))]
	pub fn notify_session_store_changed(&self) {
		self.pairing_host.session().notify_session_store_changed();
	}

	/// Read a stored permission authorization status for a product without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.permission_authorization_status", product_id = %product_id))]
	pub async fn permission_authorization_status(
		&self,
		product_id: &ProductId,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
		self.product_admin(product_context(product_id))
			.permission_authorization_status(request)
			.await
	}

	/// Read stored permission authorization statuses for a product without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.permission_authorization_statuses", product_id = %product_id))]
	pub async fn permission_authorization_statuses(
		&self,
		product_id: &ProductId,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
		self.product_admin(product_context(product_id))
			.permission_authorization_statuses(requests)
			.await
	}

	/// Update a stored permission authorization status for a product.
	#[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_permission_authorization_status", product_id = %product_id))]
	pub async fn set_permission_authorization_status(
		&self,
		product_id: &ProductId,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), v01::GenericError> {
		let account_access =
			matches!(request, PermissionAuthorizationRequest::AccountAccess { .. });
		self.product_admin(product_context(product_id))
			.set_permission_authorization_status(request, status)
			.await?;
		if account_access {
			self.pairing_host.consent().forget_allowed_once_for(product_id);
		}
		Ok(())
	}
}

fn pairing_login_error_reason(
	error: truapi::CallError<truapi::latest::HostRequestLoginError>,
) -> String {
	match error {
		truapi::CallError::Domain(truapi::latest::HostRequestLoginError::Unknown { reason }) |
		truapi::CallError::HostFailure { reason } |
		truapi::CallError::MalformedFrame { reason } => reason,
		truapi::CallError::Denied => "login denied".to_string(),
		truapi::CallError::Unsupported => "login unsupported".to_string(),
		truapi::CallError::Cancelled => "login cancelled".to_string(),
	}
}

impl PairingHostAdmin for PairingHostRuntime {
	fn cancel_pairing(&self) {
		PairingHostRuntime::cancel_pairing(self);
	}

	fn notify_session_store_changed(&self) {
		PairingHostRuntime::notify_session_store_changed(self);
	}
}

/// A wallet-local signing host: the user's keys are held on this device.
///
/// Owns the shared services plus signing-host state. There is no pairing flow,
/// so pairing cancellation is not present here.
///
/// Raw-bytes and extrinsic-payload signing, v4 transaction construction, and
/// product entropy are implemented; native signing hosts can also serve
/// ring-VRF aliases and on-chain resource allocation.
pub struct SigningHostRuntime {
	services: Arc<RuntimeServices>,
	signing_host: Arc<SigningHostRole>,
	sso_responder: Arc<SsoResponderService>,
	image_loader: crate::image_loader::ImageLoader,
}

impl SigningHostRuntime {
	/// Answer resource allocation as granted without performing it.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_grant_allowances_unchecked(&self, granted: bool) {
		self.signing_host.set_grant_allowances_unchecked(granted);
	}

	/// Keep preimage submissions in the core instead of the Bulletin chain.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_submit_preimages_locally(&self, local: bool) {
		self.signing_host.set_submit_preimages_locally(local);
	}

	/// The product's hard-subtree public key, derived from the active session
	/// root, or `None` while no session is active.
	///
	/// A signing host holds the root, so it answers this locally where a
	/// pairing host has to ask the Account Holder.
	pub fn product_subtree_public_key(
		&self,
		product_id: &ProductId,
	) -> Result<Option<[u8; 32]>, v01::GenericError> {
		self.signing_host
			.product_subtree_public_key(product_id)
			.map_err(|err| v01::GenericError { reason: err.to_string() })
	}

	/// End funding session `intent` with `stage` as though its funds had
	/// moved, so a test host can drive the flow with no chain behind it.
	/// Returns whether it was still in flight.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub async fn settle_funding_for_test(
		&self,
		intent: &str,
		stage: crate::host_logic::funding::FundingStage,
	) -> Result<bool, v01::GenericError> {
		Ok(self.services.settle_funding_for_test(intent, stage).await?)
	}

	/// Answer these resource tags as refused, replacing any earlier set.
	///
	/// For test hosts only, with the `test-host` feature enabled.
	#[cfg(feature = "test-host")]
	pub fn set_withheld_resources(&self, tags: Vec<String>) {
		self.signing_host.set_withheld_resources(tags);
	}

	/// Build a long-lived signing-host runtime around a platform implementation.
	/// Optional capabilities are answered `Unsupported`;
	/// [`Self::with_platforms`] serves them.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.new"))]
	pub fn new<P>(platform: Arc<P>, config: SigningHostConfig, spawner: Spawner) -> Self
	where
		P: Platform + 'static,
	{
		Self::with_platforms(platform, config, spawner, None, None)
	}

	/// Build a signing-host runtime that serves Chat through `chat_platform`.
	///
	/// The pairing host has had this since chat reached the core; a signing
	/// host needs it for the same reason a native host does, and without it no
	/// runnable host in this repo can serve a chat product at all.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.with_chat_platform"))]
	pub fn with_chat_platform<P>(
		platform: Arc<P>,
		config: SigningHostConfig,
		spawner: Spawner,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		Self::with_platforms(platform, config, spawner, chat_platform, None)
	}

	/// Build a signing-host runtime serving both optional adapters.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.with_platforms"))]
	pub fn with_platforms<P>(
		platform: Arc<P>,
		config: SigningHostConfig,
		spawner: Spawner,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
		contacts_platform: Option<Arc<dyn ContactsPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		let platform: Arc<dyn Platform> = platform;
		let services = RuntimeServices::with_chat_platform(
			platform,
			config.host.host_info.clone(),
			config.people_chain_genesis_hash,
			config.bulletin_chain_genesis_hash,
			config.asset_hub_chain_genesis_hash,
			spawner,
			chat_platform,
		);
		if let Some(contacts_platform) = contacts_platform {
			services.install_contacts_platform(contacts_platform);
		}
		if services.asset_hub_chain_genesis_hash().is_none() {
			// Said once at startup because the refusals themselves are
			// indistinguishable from an ungranted read. Only as visible as the
			// host's log level, which defaults to `ERROR`.
			warn!(
				"no Asset Hub configured: no product manifest will resolve, so \
                 every cross-product grant not already cached is refused"
			);
		}
		let signing_host = SigningHostRole::new(services.clone(), config.network_suffix);
		let sso_responder = SsoResponderService::new(services.clone(), signing_host.clone());
		Self {
			image_loader: crate::image_loader::ImageLoader::new(
				services.clone(),
				Arc::new(crate::time::RealTimer),
			),
			services,
			signing_host,
			sso_responder,
		}
	}

	/// Install the host's [`PermissionStatusHost`], which carries the reasoning
	/// for what it changes.
	///
	/// Set-once, so the capability cannot be swapped under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_permission_status_host"))]
	pub fn set_permission_status_host(&self, host: Arc<dyn PermissionStatusHost>) -> bool {
		self.services.install_permission_status_host(host)
	}

	/// Install the host's [`PocketPlatform`], which owns the card collection.
	///
	/// Set-once, so the collection cannot change hands under a running
	/// product. Returns whether this call installed it. Call it before serving
	/// any product runtime.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_pocket_platform"))]
	pub fn set_pocket_platform(&self, platform: Arc<dyn PocketPlatform>) -> bool {
		self.services.install_pocket_platform(platform)
	}

	/// Install the host's [`FundingPlatform`], the native funding overlay.
	///
	/// Set-once. Returns whether this call installed it. Call it before
	/// serving any product runtime.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_funding_platform"))]
	pub fn set_funding_platform(&self, platform: Arc<dyn FundingPlatform>) -> bool {
		self.services.install_funding_platform(platform)
	}

	/// Install the feed the host draws its funding screens from: sessions,
	/// whether funding is in progress, and the overlay's options.
	/// Install it before [`Self::set_funding_platform`], which resumes
	/// funding. Set-once; returns whether this call installed it.
	pub fn set_funding_feed(&self, feed: Arc<dyn crate::runtime::FundingFeed>) -> bool {
		self.services.funding().install_feed(feed)
	}

	/// Every ended funding session, as summaries newest first: the ones
	/// held in full and the ones compacted, which are kept for good. The
	/// host reads it whenever it shows history.
	pub fn funding_history(&self) -> Vec<crate::host_logic::funding::FundingSessionSummary> {
		self.services.funding().history()
	}

	/// Record that the host told the user how ended session `intent` went.
	/// Until it does, the session is handed over again through
	/// `funding_session_changed` each time funding resumes.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.acknowledge_funding_session"))]
	pub async fn acknowledge_funding_session(
		&self,
		intent: &str,
	) -> Result<bool, v01::GenericError> {
		Ok(self.services.acknowledge_funding_session(intent).await?)
	}

	/// Cancel funding session `intent` at the user's request, and say what
	/// that did: it ended, its provider was asked, a cancel is already with
	/// the provider, or it is too late because the balance was credited.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.cancel_funding"))]
	pub async fn cancel_funding(
		&self,
		intent: &str,
	) -> Result<crate::host_logic::funding::CancelOutcome, v01::GenericError> {
		Ok(self.services.cancel_funding(intent).await?)
	}

	/// Replace the funding providers this host offers, each with the Worker
	/// manifest it ships for it. The core re-reads each one from dotNS in the
	/// background, and that answer wins once it has one.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_funding_providers"))]
	pub fn set_funding_providers(
		&self,
		providers: Vec<crate::host_logic::funding_providers::FundingProviderEntry>,
	) -> Result<(), v01::GenericError> {
		self.services
			.set_funding_providers(providers)
			.map_err(|err| v01::GenericError { reason: err.to_string() })
	}

	/// Quotes for funding session `intent`, sent through
	/// [`FundingPlatform::funding_quotes_changed`] until the session is handed
	/// to a provider, ends or leaves the overlay, or the host asks for another
	/// `ask`. Returns the request id each list carries.
	pub fn request_funding_quotes(&self, intent: &str, ask: latest::FundingQuoteAsk) -> u64 {
		self.services.request_funding_quotes(intent, ask)
	}

	/// Record what the user picked in funding session `intent`'s overlay;
	/// the options it leads to arrive through
	/// [`crate::runtime::FundingFeed::funding_options_changed`].
	pub fn select_funding_options(
		&self,
		intent: &str,
		selection: crate::host_logic::funding_providers::FundingSelection,
	) {
		self.services.select_funding_options(intent, selection);
	}

	/// Answer the provider's cancel offer on funding session `intent`:
	/// accept it on `terms`, the fee and refund the host showed, so the
	/// provider settles the cancel, or decline it with `None`, so the session
	/// carries on. An offer whose terms changed answers `Changed`.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.confirm_funding_cancel"))]
	pub async fn confirm_funding_cancel(
		&self,
		intent: &str,
		terms: Option<crate::host_logic::funding::CancelTerms>,
	) -> Result<crate::host_logic::funding::CancelConfirmation, v01::GenericError> {
		Ok(self.services.confirm_funding_cancel(intent, terms).await?)
	}

	/// Hand open funding session `intent` to the provider the user chose,
	/// by product id, on the quote they chose it by. Returns whether it was
	/// open and not yet assigned.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.select_funding_provider"))]
	pub async fn select_funding_provider(
		&self,
		intent: &str,
		provider_id: &ProductId,
		quote_id: &str,
	) -> Result<bool, v01::GenericError> {
		self.services
			.select_funding_provider(intent, provider_id, quote_id)
			.await
			.map_err(|err| v01::GenericError { reason: err.to_string() })
	}

	/// Open a funding session on the host's own behalf, as the Balance card's
	/// Add and Withdraw do, and show the overlay. Returns the session id, or
	/// `None` when the user dismissed the overlay without starting.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.open_funding"))]
	pub async fn open_funding(
		&self,
		direction: v01::FundingDirection,
		amount: Option<u128>,
	) -> Result<Option<String>, v01::GenericError> {
		self.services.open_host_funding(direction, amount).await
	}

	/// Install the host's [`GamePlatform`], which holds each product's game
	/// reminder.
	///
	/// Set-once, so reminders cannot change hands under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_game_platform"))]
	pub fn set_game_platform(&self, platform: Arc<dyn GamePlatform>) -> bool {
		self.services.install_game_platform(platform)
	}

	/// Install the host's [`TopUpPlatform`], which claims funds into the
	/// balance. Set-once. Returns whether this call installed it.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_top_up_platform"))]
	pub fn set_top_up_platform(&self, platform: Arc<dyn TopUpPlatform>) -> bool {
		self.services.install_top_up_platform(platform)
	}

	/// Install the host's [`PaymentPlatform`], which pays from the user's
	/// balance once the user approves. Set-once; returns whether this call
	/// installed it.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_payment_platform"))]
	pub fn set_payment_platform(&self, platform: Arc<dyn PaymentPlatform>) -> bool {
		self.services.install_payment_platform(platform)
	}

	/// Install the host's [`BalancePlatform`], which shares the user's
	/// balance with products. Set-once; returns whether this call installed
	/// it.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_balance_platform"))]
	pub fn set_balance_platform(&self, platform: Arc<dyn BalancePlatform>) -> bool {
		self.services.install_balance_platform(platform)
	}

	/// Install the host's [`ScannerPlatform`] before serving any product
	/// runtime. Set-once: returns whether this call installed it.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_scanner_platform"))]
	pub fn set_scanner_platform(&self, platform: Arc<dyn ScannerPlatform>) -> bool {
		self.services.install_scanner_platform(platform)
	}

	/// Install the host's [`ContactsPlatform`], which owns the contact list and
	/// draws the picker.
	///
	/// Set-once, so the picker cannot change hands under a running product.
	/// Returns whether this call installed it. Call it before serving any
	/// product runtime.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_contacts_platform"))]
	pub fn set_contacts_platform(&self, platform: Arc<dyn ContactsPlatform>) -> bool {
		self.services.install_contacts_platform(platform)
	}

	/// Tell the core the host's contacts changed, so no handle resolves from
	/// what it cached before. Call it whenever a contact is removed or blocked;
	/// the next transaction naming a contact reads the list again.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.notify_contacts_changed"))]
	pub fn notify_contacts_changed(&self) {
		self.services.contact_handles.clear();
	}

	/// Install the host's [`DevicePairingObserver`], told whenever a device
	/// finishes pairing with this signing host.
	///
	/// Set-once, so the surface that announces a new device cannot change
	/// hands between two pairings. Returns whether this call installed it.
	/// Call it before answering any pairing deeplink.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_device_pairing_observer"))]
	pub fn set_device_pairing_observer(&self, observer: Arc<dyn DevicePairingObserver>) -> bool {
		self.services.install_device_pairing_observer(observer)
	}

	/// Install the core-owned database that durable consumers share.
	///
	/// Set-once, so durable state cannot move to another file under a running
	/// consumer. Returns whether this call installed it. Nothing runs over it
	/// until [`Self::start_core_services`].
	#[cfg(not(target_arch = "wasm32"))]
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_core_db"))]
	pub fn set_core_db(&self, db: crate::store::Db) -> bool {
		self.services.install_core_db(db)
	}

	/// Install the provider of the installation id NFT purse item keys are
	/// derived under. Set-once; returns whether this call installed it.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn set_installation_provider(
		&self,
		provider: Arc<dyn crate::installation::InstallationIdProvider>,
	) -> bool {
		self.services.install_installation_provider(provider)
	}

	/// Install the host storage encrypted at rest, which keeps the secrets the
	/// core database must not hold. Set-once; returns whether this call
	/// installed it.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn set_encrypted_storage(
		&self,
		storage: Arc<dyn crate::encrypted_storage::EncryptedStorage>,
	) -> bool {
		self.services.install_encrypted_storage(storage)
	}

	/// Starts what runs over the core database: the durable transaction
	/// engine, then NFT purses with the tracker of their holdings. Call it
	/// once the setters it reads have run: [`Self::set_core_db`],
	/// [`Self::set_installation_provider`], [`Self::set_encrypted_storage`]
	/// and [`Self::set_durable_work_observer`]. NFT transfers and purses need
	/// the encrypted storage. A part whose inputs are missing
	/// is skipped and logged. A second call does nothing.
	#[cfg(not(target_arch = "wasm32"))]
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.start_core_services"))]
	pub fn start_core_services(&self) {
		let Ok(db) = self.services.core_db() else {
			tracing::warn!("no core database set, durable transactions and NFT purses are off");
			return;
		};
		let signer = Arc::new(Arc::downgrade(&self.signing_host));
		let Some(engine) = start_durable_engine(&self.services, db.clone(), signer) else {
			return;
		};
		self.start_nft_purses(db, engine);
	}

	/// Installs the NFT ledger over `db` and `durable`, with the static receive
	/// secret NFT purses open cheques with, and starts tracking its holdings,
	/// when an installation provider and encrypted storage are set.
	#[cfg(not(target_arch = "wasm32"))]
	fn start_nft_purses(
		&self,
		db: crate::store::Db,
		durable: Arc<crate::durable::DurableTxEngine>,
	) {
		let Some(ledger) = self.nft_ledger(db, durable) else {
			return;
		};
		let static_secret = Arc::new(Arc::downgrade(&self.signing_host));
		self.services.install_static_receive_secret(static_secret);
		self.services.install_nft_ledger(ledger.clone());
		start_holding_tracker(&self.services, ledger);
	}

	/// The NFT ledger over `db` and `durable`, deriving item keys from the
	/// signing host, or `None`, logged, without an installation provider or
	/// encrypted storage.
	#[cfg(not(target_arch = "wasm32"))]
	fn nft_ledger(
		&self,
		db: crate::store::Db,
		durable: Arc<crate::durable::DurableTxEngine>,
	) -> Option<crate::nft::NftLedger> {
		let Some(installation) = self.services.installation_provider() else {
			tracing::warn!("no installation provider set, NFT purses are off");
			return None;
		};
		let Some(encrypted) = self.services.encrypted_storage() else {
			tracing::warn!("no encrypted storage set, NFT purses are off");
			return None;
		};
		let signing_host = Arc::downgrade(&self.signing_host);
		let keys = Arc::new(signing_host.clone());
		let secrets = Arc::new(signing_host);
		let imported = crate::nft::ImportedSecretStore::new(encrypted);
		let deps = crate::nft::NftLedgerDeps { db, installation, keys, durable, secrets, imported };
		Some(crate::nft::NftLedger::new(deps))
	}

	/// Install the host observer told whether durable transactions are
	/// pending. Set-once; returns whether this call installed it.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn set_durable_work_observer(
		&self,
		observer: Arc<dyn crate::durable::DurableWorkObserver>,
	) -> bool {
		self.services.install_durable_work_observer(observer)
	}

	/// Runs durable recovery until no transaction is live, building scheduled
	/// transactions meanwhile.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn run_durable_recovery(&self) -> Result<(), crate::durable::RecoveryError> {
		self.services
			.durable_engine()
			.ok_or(crate::durable::RecoveryError::NotConfigured)?
			.run_until_settled()
			.await
	}

	/// Reports the core database's state.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn core_database_status(
		&self,
	) -> Result<crate::store::DbStatus, crate::store::DbError> {
		self.services.core_db()?.status().await
	}

	/// The image `reference` names, checked against its CID and the image policy.
	pub async fn load_image(
		&self,
		reference: crate::ImageReference,
	) -> Result<crate::LoadedImage, crate::ImageError> {
		self.image_loader.load(reference).await
	}

	/// Build a product-facing runtime from this signing host.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.product_runtime"))]
	pub fn product_runtime(
		&self,
		product: ProductContext,
		sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		ProductRuntime::new(
			self.services.clone(),
			self.signing_host.clone(),
			self.signing_host.clone(),
			product,
			ConnectionAdapters::from_services(&self.services),
			sink,
		)
	}

	/// Build one product connection with adapters scoped to one native
	/// executable while sharing this runtime's authentication and services.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn product_runtime_with(
		&self,
		product: ProductContext,
		adapters: ConnectionAdapters,
		sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		ProductRuntime::new(
			self.services.clone(),
			self.signing_host.clone(),
			self.signing_host.clone(),
			product,
			adapters,
			sink,
		)
	}

	/// Build a product-scoped administration handle from this signing host.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.product_admin"))]
	pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
		HostAdmin::new(
			self.services.clone(),
			self.signing_host.clone(),
			self.signing_host.clone(),
			product,
			ConnectionAdapters::from_services(&self.services),
		)
	}

	/// Build a product administration handle with adapters scoped to one
	/// native executable connection.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn product_admin_with(
		&self,
		product: ProductContext,
		adapters: ConnectionAdapters,
	) -> HostAdmin {
		HostAdmin::new(
			self.services.clone(),
			self.signing_host.clone(),
			self.signing_host.clone(),
			product,
			adapters,
		)
	}

	/// Reference counts per product worker, shared by every connection of
	/// this host.
	pub fn worker_ledger(&self) -> &WorkerLedger {
		&self.services.worker_ledger
	}

	/// Report that the product's worker stopped without being asked to:
	/// it is started again while something holds it, with a growing wait
	/// when it keeps crashing.
	pub fn worker_exited(&self, product_id: &ProductId) {
		self.services.worker_exited(product_id);
	}

	/// Return whether this host currently has an authenticated signing session.
	pub fn has_active_session(&self) -> bool {
		self.signing_host.session_state().current().is_some()
	}

	/// Disconnect the active account-authority session.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.disconnect_session"))]
	pub async fn disconnect_session(&self) {
		self.signing_host.disconnect().await;
	}

	/// Revoke one product's grants from the current local activation while
	/// preserving unrelated products.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.clear_product_state", %product_id))]
	pub async fn clear_product_state(&self, product_id: &ProductId) {
		self.signing_host.clear_product_state(product_id);
	}

	/// Read a stored permission authorization status for a product without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.permission_authorization_status", product_id = %product_id))]
	pub async fn permission_authorization_status(
		&self,
		product_id: &ProductId,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
		self.product_admin(product_context(product_id))
			.permission_authorization_status(request)
			.await
	}

	/// Read stored permission authorization statuses for a product without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.permission_authorization_statuses", product_id = %product_id))]
	pub async fn permission_authorization_statuses(
		&self,
		product_id: &ProductId,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
		self.product_admin(product_context(product_id))
			.permission_authorization_statuses(requests)
			.await
	}

	/// Update a stored permission authorization status for a product.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.set_permission_authorization_status", product_id = %product_id))]
	pub async fn set_permission_authorization_status(
		&self,
		product_id: &ProductId,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), v01::GenericError> {
		let account_access =
			matches!(request, PermissionAuthorizationRequest::AccountAccess { .. });
		self.product_admin(product_context(product_id))
			.set_permission_authorization_status(request, status)
			.await?;
		if account_access {
			self.signing_host.consent().forget_allowed_once_for(product_id);
		}
		Ok(())
	}

	/// Registered providers available for an internal well-known-ring feature.
	pub async fn ring_vrf_providers(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Vec<v01::ProductAccountId>, v01::GenericError> {
		self.signing_host.ring_vrf_providers(ring).await.map_err(ring_vrf_admin_error)
	}

	/// Current provider selected for an internal well-known-ring feature.
	pub async fn selected_ring_vrf_provider(
		&self,
		ring: &v01::RingLocation,
	) -> Result<Option<v01::ProductAccountId>, v01::GenericError> {
		self.signing_host
			.selected_ring_vrf_provider(ring)
			.await
			.map_err(ring_vrf_admin_error)
	}

	/// Select a registered provider for an internal well-known-ring feature.
	pub async fn select_ring_vrf_provider(
		&self,
		ring: v01::RingLocation,
		handle: v01::ProductAccountId,
	) -> Result<(), v01::GenericError> {
		self.signing_host
			.select_ring_vrf_provider(ring, handle)
			.await
			.map_err(ring_vrf_admin_error)
	}

	/// Activate a wallet-local session from host-held secret material (raw
	/// BIP-39 entropy).
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.activate_local_session"))]
	pub async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), v01::GenericError> {
		self.signing_host
			.activate_local_session(secret)
			.await
			.map_err(|err| v01::GenericError { reason: err.to_string() })
	}

	/// Activate a wallet-local session from host-held secret material and
	/// attach known identity metadata.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.activate_local_session_with_identity"))]
	pub async fn activate_local_session_with_identity(
		&self,
		secret: Vec<u8>,
		lite_username: Option<String>,
	) -> Result<(), v01::GenericError> {
		self.signing_host
			.activate_local_session_with_identity(secret, lite_username)
			.await
			.map_err(|err| v01::GenericError { reason: err.to_string() })
	}

	/// Answer a pairing host's handshake deeplink and serve the resulting SSO
	/// session until it ends.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.respond_to_pairing"))]
	pub async fn respond_to_pairing(
		&self,
		deeplink: &str,
	) -> Result<ResponderExit, v01::GenericError> {
		self.sso_responder.respond_to_pairing(deeplink).await.map_err(Into::into)
	}

	/// Tell a pairing host that allowance allocation is under way, so it leaves
	/// its QR screen while the allocation runs.
	///
	/// Answering needs this host's own statement-store allowance, so register
	/// the `WalletSso` renewal target before calling.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.notify_pairing_allowance_allocation"))]
	pub async fn notify_pairing_allowance_allocation(
		&self,
		deeplink: &str,
	) -> Result<crate::runtime::AnnouncedPairing, v01::GenericError> {
		self.sso_responder
			.notify_pairing_allowance_allocation(deeplink)
			.await
			.map_err(Into::into)
	}

	/// Tell a pairing host that pairing failed, so it reports `reason` and
	/// offers a retry.
	///
	/// Owed to any host that was sent
	/// [`Self::notify_pairing_allowance_allocation`]: it has dropped its QR and
	/// waits without a deadline. Takes that call's handle, so the notice is
	/// signed by the account that already reached this host even if the signer
	/// has rotated since.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.notify_pairing_failed"))]
	pub async fn notify_pairing_failed(
		&self,
		announced: &crate::runtime::AnnouncedPairing,
		reason: String,
	) -> Result<(), v01::GenericError> {
		self.sso_responder
			.notify_pairing_failed(announced, reason)
			.await
			.map_err(Into::into)
	}

	/// Answer a pairing host's handshake without entering its long-lived serve loop.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.establish_pairing"))]
	pub async fn establish_pairing(&self, deeplink: &str) -> Result<(), v01::GenericError> {
		self.sso_responder.establish_pairing(deeplink).await.map_err(Into::into)
	}

	/// Resume a previously paired host from its persisted public peer keys.
	///
	/// Only [`ResponderExit::PeerDisconnected`] authorizes removing the durable
	/// pairing. Retain it after [`ResponderExit::SubscriptionEnded`] or an error.
	/// After [`ResumePairingError::Interrupted`] the call can be made again;
	/// after [`ResumePairingError::CannotServe`] it fails the same way until
	/// the local session changes.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.resume_pairing"))]
	pub async fn resume_pairing(
		&self,
		peer: PairedSsoPeer,
	) -> Result<ResponderExit, ResumePairingError> {
		self.sso_responder.resume_pairing(peer).await
	}

	/// Notify a paired host that this signing host is ending their SSO session.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.disconnect_paired_host"))]
	pub async fn disconnect_paired_host(
		&self,
		peer: PairedSsoPeer,
	) -> Result<(), v01::GenericError> {
		self.sso_responder.disconnect_paired_host(peer).await.map_err(Into::into)
	}

	/// Answer one SSO request using this signing host.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.answer_sso_request"))]
	pub async fn answer_sso_request(&self, message: RemoteMessage) -> SsoRequestOutcome {
		let service = SigningHostSsoService::new(self.signing_host.clone());
		match service.answer(message).await {
			Dispatch::Response(answer) => {
				SsoRequestOutcome::Response { message: answer.message.encode() }
			},
			Dispatch::Disconnected => SsoRequestOutcome::Disconnected,
			Dispatch::NotARequest(_) | Dispatch::Withdraw(_) | Dispatch::Withdrawn => {
				SsoRequestOutcome::Ignored
			},
		}
	}
}

/// Installs the durable transaction engine over `db`, reports its pending
/// work and starts its submissions. `None` when an engine is already
/// installed.
#[cfg(not(target_arch = "wasm32"))]
fn start_durable_engine(
	services: &Arc<RuntimeServices>,
	db: crate::store::Db,
	signer: Arc<dyn crate::nft::ItemSigner>,
) -> Option<Arc<crate::durable::DurableTxEngine>> {
	let engine = durable_engine(services, db, signer);
	if !services.install_durable_engine(engine.clone()) {
		return None;
	}
	forward_durable_work(services, &engine);
	engine.ensure_submissions_started();
	Some(engine)
}

/// The durable transaction engine over `db`, reading and submitting through
/// the runtime's chain connections. It serves NFT transfers when an Asset Hub
/// genesis is configured.
#[cfg(not(target_arch = "wasm32"))]
fn durable_engine(
	services: &RuntimeServices,
	db: crate::store::Db,
	signer: Arc<dyn crate::nft::ItemSigner>,
) -> Arc<crate::durable::DurableTxEngine> {
	use crate::{
		durable::{DurableDeps, DurableTxEngine},
		time::RealTimer,
	};

	let chain = Arc::new(services.chain.clone());
	let registry = durable_registry(services, signer);
	DurableTxEngine::new(DurableDeps {
		db,
		registry,
		heads: chain.clone(),
		blocks: chain.clone(),
		validator: chain.clone(),
		submitter: chain,
		timer: Arc::new(RealTimer),
		spawner: services.spawner.clone(),
	})
}

/// The domains and policies the durable engine serves: NFT transfers, on the
/// Asset Hub that runs the Scarcity pallet. They need encrypted storage, which
/// keeps the secrets of imported item keys.
#[cfg(not(target_arch = "wasm32"))]
fn durable_registry(
	services: &RuntimeServices,
	signer: Arc<dyn crate::nft::ItemSigner>,
) -> crate::durable::DurableRegistry {
	use crate::nft::{ImportedSecretStore, ScarcityTransfersDeps, with_scarcity_transfers};

	let registry = crate::durable::DurableRegistry::new();
	let Some(genesis) = services.asset_hub_chain_genesis_hash() else {
		tracing::warn!("no Asset Hub genesis configured, NFT transfers are off");
		return registry;
	};
	let Some(encrypted) = services.encrypted_storage() else {
		tracing::warn!("no encrypted storage set, NFT transfers are off");
		return registry;
	};
	let chain = Arc::new(services.chain.clone());
	let deps = ScarcityTransfersDeps {
		storage: chain.clone(),
		heads: chain,
		genesis: genesis.into(),
		signer,
		imported: ImportedSecretStore::new(encrypted),
		extrinsics: services.extrinsics.clone(),
	};
	with_scarcity_transfers(registry, deps)
}

/// Starts tracking what NFT item keys hold on Asset Hub, which runs the
/// Scarcity pallet. Without an Asset Hub genesis there is nothing to track.
#[cfg(not(target_arch = "wasm32"))]
fn start_holding_tracker(services: &RuntimeServices, ledger: crate::nft::NftLedger) {
	use crate::nft::{HoldingTracker, HoldingTrackerDeps};

	let Some(genesis) = services.asset_hub_chain_genesis_hash() else {
		tracing::warn!("no Asset Hub genesis configured, NFT holdings are not tracked");
		return;
	};
	let tracker = HoldingTracker::start(HoldingTrackerDeps {
		ledger,
		storage: Arc::new(services.chain.clone()),
		genesis: genesis.into(),
		spawner: services.spawner.clone(),
	});
	services.install_holding_tracker(tracker);
}

/// Reports every change of the engine's pending work to the host observer,
/// for as long as the engine lives.
#[cfg(not(target_arch = "wasm32"))]
fn forward_durable_work(services: &Arc<RuntimeServices>, engine: &crate::durable::DurableTxEngine) {
	let services = Arc::downgrade(services);
	engine.report_work_to(move |pending| {
		let observer = services.upgrade().and_then(|services| services.durable_work_observer());
		if let Some(observer) = observer {
			observer.durable_work_changed(pending);
		}
	});
}

#[cfg(not(target_arch = "wasm32"))]
impl SigningHostRuntime {
	/// Record statement-store accounts the host must keep renewed across
	/// allowance periods.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.track_statement_renewal_targets"))]
	pub async fn track_statement_renewal_targets(
		&self,
		targets: Vec<crate::runtime::StatementRenewalTarget>,
	) -> Result<(), v01::GenericError> {
		self.signing_host
			.track_statement_renewal_targets(targets)
			.await
			.map_err(Into::into)
	}

	/// Every statement account the renewal ledger currently tracks.
	///
	/// Needs no active session, so a host can audit which entries are spending
	/// its finite per-period slots before deciding to renew.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.statement_renewal_targets"))]
	pub async fn statement_renewal_targets(
		&self,
	) -> Result<Vec<crate::runtime::TrackedStatementRenewalTarget>, v01::GenericError> {
		self.signing_host.statement_renewal_targets().await.map_err(Into::into)
	}

	/// Root public key the active identity records its fixed ledger entries
	/// under.
	///
	/// Needs an active session, and fails with `Disconnected` without one.
	/// Compare it against each entry's owner to tell what a pass will renew
	/// from what it will prune.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.statement_renewal_owner_key"))]
	pub fn statement_renewal_owner_key(&self) -> Result<truapi::Bytes32, v01::GenericError> {
		self.signing_host.statement_renewal_owner_key().map_err(Into::into)
	}

	/// Stop renewing one fixed statement account.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.untrack_statement_renewal_account"))]
	pub async fn untrack_statement_renewal_account(
		&self,
		account_id: &[u8; 32],
	) -> Result<bool, v01::GenericError> {
		self.signing_host
			.untrack_statement_renewal_account(account_id)
			.await
			.map_err(Into::into)
	}

	/// Run one statement-store renewal pass now and return per-target
	/// outcomes. This is the primary entry point; hosts whose process cannot
	/// stay alive (mobile) call it from an OS scheduler instead of
	/// [`Self::start_statement_allowance_renewal`].
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.renew_statement_allowances"))]
	pub async fn renew_statement_allowances(
		&self,
	) -> Result<crate::statement_allowance::renewal::StatementRenewalReport, v01::GenericError> {
		self.signing_host.renew_statement_allowances().await.map_err(Into::into)
	}

	/// Start the periodic statement-store renewal loop (hourly, plus a tick
	/// just after each period boundary). Idempotent; the loop stops when this
	/// runtime is dropped.
	#[instrument(skip_all, fields(runtime.method = "signing_host_runtime.start_statement_allowance_renewal"))]
	pub fn start_statement_allowance_renewal(&self) {
		self.signing_host.start_statement_allowance_renewal();
	}

	/// Delay until the next renewal pass is due, for hosts that schedule
	/// wake-ups through an OS scheduler instead of the in-process loop.
	pub fn next_statement_renewal_delay(&self) -> std::time::Duration {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|now| crate::statement_allowance::renewal::next_tick_delay(now.as_secs()))
			.unwrap_or(std::time::Duration::from_secs(3_600))
	}
	/// The most recent pass the in-process renewal loop ran.
	///
	/// `None` until a pass has run, which is "not yet" rather than healthy. A host
	/// driving the loop has no return value to inspect, so this is where it learns
	/// that a period was exhausted and an allowance went unrenewed.
	pub fn last_statement_renewal_report(
		&self,
	) -> Option<crate::statement_allowance::renewal::StatementRenewalReport> {
		self.signing_host.last_statement_renewal_report()
	}
}

/// Adapters scoped to one product connection: the platform serving its
/// syscalls, the optional native Chat adapter, and the connection's
/// host-fed action streams. Non-native connections use [`Self::from_services`].
///
/// `pocket_platform` is the same kind of optional adapter for the card
/// collection, and `game_platform` for the product's game reminder.
#[derive(Clone)]
pub struct ConnectionAdapters {
	pub platform: Arc<dyn Platform>,
	pub chat_platform: Option<Arc<dyn ChatPlatform>>,
	/// Live OS permission state for this connection. It travels here rather
	/// than on the host runtime because a native host builds one platform per
	/// product execution, so the object that reports OS state has to be the
	/// same one that presents the prompt.
	pub permission_status: Option<Arc<dyn PermissionStatusHost>>,
	/// SDK and internal network connections must share an execution's one-use grants.
	pub permission_grants: Arc<crate::host_internal::permissions::TemporaryPermissions>,
	pub chat: Arc<ActionChannel<truapi::versioned::chat::HostChatActionSubscribeItem>>,
	pub renderer: Arc<ActionChannel<truapi::versioned::renderer::HostRendererActionSubscribeItem>>,
	pub pocket_platform: Option<Arc<dyn PocketPlatform>>,
	pub game_platform: Option<Arc<dyn GamePlatform>>,
	/// Control of the card face above this connection's Widget, when the host draws one.
	pub expanded_card: Option<Arc<dyn ExpandedCardHost>>,
}

impl ConnectionAdapters {
	/// Default adapters for a connection without native scoping.
	pub fn from_services(services: &RuntimeServices) -> Self {
		Self {
			platform: services.platform.clone(),
			chat_platform: services.chat_platform.clone(),
			permission_status: services.permission_status_host(),
			permission_grants: Arc::default(),
			chat: Arc::new(ActionChannel::chat()),
			renderer: Arc::new(ActionChannel::renderer()),
			pocket_platform: services.pocket_platform(),
			expanded_card: None,
			game_platform: services.game_platform(),
		}
	}
}

fn ring_vrf_admin_error(
	error: crate::host_internal::sso_messages::RingVrfError,
) -> v01::GenericError {
	v01::GenericError {
		reason: match error {
			crate::host_internal::sso_messages::RingVrfError::Unknown { reason } => reason,
			other => format!("{other:?}"),
		},
	}
}

/// Product-scoped administration handle for host UI.
///
/// Host UI should use this when it needs to inspect or update core-owned state
/// without owning a product frame endpoint.
pub struct HostAdmin {
	authority: Arc<dyn ProductAuthority>,
	host_session: Arc<dyn HostSession>,
	product_runtime: Arc<ProductRuntimeHost>,
}

impl HostAdmin {
	/// Product runtime managed by this admin handle.
	#[cfg(any(test, not(target_arch = "wasm32")))]
	pub fn product_runtime(&self) -> &Arc<ProductRuntimeHost> {
		&self.product_runtime
	}

	/// Build an admin handle for one product connection.
	#[instrument(skip_all, fields(runtime.method = "host_admin.new"))]
	pub fn new(
		services: Arc<RuntimeServices>,
		authority: Arc<dyn ProductAuthority>,
		host_session: Arc<dyn HostSession>,
		product: ProductContext,
		adapters: ConnectionAdapters,
	) -> Self {
		let product_runtime = Arc::new(ProductRuntimeHost::from_services(
			services,
			adapters,
			authority.clone(),
			host_session.clone(),
			product,
		));
		Self { authority, host_session, product_runtime }
	}

	/// Core-owned logout/disconnect.
	#[instrument(skip_all, fields(runtime.method = "host_admin.disconnect_session"))]
	pub async fn disconnect_session(&self) {
		self.host_session.disconnect().await;
	}

	/// Read a stored permission authorization status without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "host_admin.permission_authorization_status"))]
	pub async fn permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
		self.product_runtime.connection().permission_authorization_status(request).await
	}

	/// Read stored permission authorization statuses without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "host_admin.permission_authorization_statuses"))]
	pub async fn permission_authorization_statuses(
		&self,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
		self.product_runtime
			.connection()
			.permission_authorization_statuses(requests)
			.await
	}

	/// Update a stored permission authorization status.
	#[instrument(skip_all, fields(runtime.method = "host_admin.set_permission_authorization_status"))]
	pub async fn set_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), v01::GenericError> {
		self.product_runtime.set_permission_authorization_status(request, status).await
	}
}

#[async_trait::async_trait]
impl CoreAdmin for HostAdmin {
	async fn disconnect_session(&self) -> Result<(), v01::GenericError> {
		HostAdmin::disconnect_session(self).await;
		Ok(())
	}

	async fn get_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
		self.permission_authorization_status(request).await
	}

	async fn get_permission_authorization_statuses(
		&self,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
		self.permission_authorization_statuses(requests).await
	}

	async fn set_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), v01::GenericError> {
		HostAdmin::set_permission_authorization_status(self, request, status).await
	}

	async fn get_session_chat_identity_key(&self) -> Result<Option<[u8; 32]>, v01::GenericError> {
		Ok(self
			.host_session
			.session_state()
			.current()
			.and_then(|session| session.identity_chat_private_key))
	}

	async fn get_device_statement_key(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
		Ok(self
			.host_session
			.session_state()
			.current()
			.and_then(|session| session.sso)
			.map(|sso| sso.ss_secret.to_vec()))
	}

	async fn get_device_encryption_key(&self) -> Result<[u8; 32], v01::GenericError> {
		self.product_runtime
			.connection()
			.services()
			.device_encryption_secret()
			.await
			.map_err(Into::into)
	}

	async fn get_product_subtree_public_key(
		&self,
		product_id: &ProductId,
		timeout_ms: Option<u32>,
	) -> Result<Option<[u8; 32]>, v01::GenericError> {
		product_subtree_public_key(self.authority.as_ref(), product_id, timeout_ms).await
	}
}

/// Shared body of the two entry points that resolve a product subtree: the
/// `CoreAdmin` method native hosts call and the pairing-host runtime method the
/// wasm bridge wraps.
///
/// The identifier is normalized because the product path normalizes before it
/// populates the cache, so an unnormalized lookup would miss a key the core is
/// already holding.
///
/// The deadline is enforced by dropping the call rather than awaiting it after
/// cancelling. Only the SSO response wait observes the cancellation token; the
/// statement-store setup before it does not, so a call parked there ignores a
/// cancel and would outlive any deadline that waited for it to finish.
async fn product_subtree_public_key(
	authority: &(impl ProductAuthority + ?Sized),
	product_id: &ProductId,
	timeout_ms: Option<u32>,
) -> Result<Option<[u8; 32]>, v01::GenericError> {
	let Some(session) = authority.current_session() else {
		return Ok(None);
	};
	let timeout = timeout_ms
		.map(|timeout_ms| Duration::from_millis(u64::from(timeout_ms)))
		.unwrap_or(DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT);
	let mut cx = CallContext::default();
	cx.set_timeout(timeout);

	let call = authority.product_subtree_public_key(&cx, &session, product_id).fuse();
	let deadline = futures_timer::Delay::new(timeout).fuse();
	pin_mut!(call, deadline);
	futures::select! {
		result = call => result.map(Some).map_err(|error| v01::GenericError {
			reason: error.to_string(),
		}),
		() = deadline => {
			let reason = CancellationReason::TimedOut { timeout };
			// Best effort for anything already watching the token. The call is
			// dropped on return either way, which is what actually ends it.
			cx.cancel().cancel_with_reason(reason.clone());
			Err(v01::GenericError {
				reason: format!("Product subtree request {reason}"),
			})
		}
	}
}

/// Target-neutral host runtime wrapper.
///
/// `ProductRuntime` is product-scoped. It owns the dispatcher core for one product
/// connection and handles byte-frame ingress, response/subscription egress, and
/// in-flight dispatch cancellation on dispose.
pub struct ProductRuntime {
	core: TrUApiCore,
	admin: HostAdmin,
	transport: Arc<SinkTransport>,
	host_subscriptions: Arc<HostInitiatedSubscriptionManager>,
	/// How the funding registry calls this connection, when it is a worker's,
	/// with the product it calls.
	funding_link: Option<(ProductId, Arc<dyn FundingProviderLink>)>,
	disposed: Arc<AtomicBool>,
	in_flight: Mutex<HashMap<u64, AbortHandle>>,
	next_dispatch_id: AtomicU64,
}

/// Host-facing control handle for pushing native events into one concrete
/// product connection.
#[derive(Clone)]
pub struct ProductRuntimeControl {
	connection: Arc<ProductConnection>,
	transport: Arc<SinkTransport>,
	host_subscriptions: Arc<HostInitiatedSubscriptionManager>,
	disposed: Arc<AtomicBool>,
}

impl ProductRuntimeControl {
	fn connection(&self) -> Result<&ProductConnection, ProductRuntimeError> {
		if self.disposed.load(Ordering::Acquire) {
			return Err(ProductRuntimeError::Closed);
		}
		Ok(&self.connection)
	}

	/// Publish one host-authored Chat action into this connection's action
	/// stream, buffering it until the product subscribes.
	pub fn publish_chat_action(
		&self,
		action: v01::HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.connection()?
			.publish_chat_action(truapi::versioned::chat::HostChatActionSubscribeItem::V1(action))
	}

	/// Publish one action triggered inside a product-rendered body into this
	/// connection's renderer action stream, buffering it until the product
	/// subscribes.
	pub fn publish_renderer_action(
		&self,
		item: v01::HostRendererActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.connection()?.publish_renderer_action(
			truapi::versioned::renderer::HostRendererActionSubscribeItem::V1(item),
		)
	}

	/// Ask this connection's product to draw one body, streaming replacement
	/// trees until the returned subscription is dropped.
	///
	/// An open render stream is one reference on the product's worker, held by
	/// the core for exactly as long as the returned subscription lives. A
	/// transition it causes reaches the host through the controller installed with
	/// [`WorkerLedger::install_worker_controller`].
	pub fn render(
		&self,
		request: v01::ProductRendererRenderRequest,
	) -> Result<
		truapi::Subscription<v01::RendererNode, truapi::CallError<v01::GenericError>>,
		ProductRuntimeError,
	> {
		self.connection()?.renderer_access()?;
		let reference = self.connection.hold_worker();
		let back = match &request.context {
			v01::RenderContext::PocketCardBack { card_id } => {
				Some((self.connection.clone(), card_id.clone()))
			},
			_ => None,
		};
		let request = truapi::versioned::renderer::ProductRendererRenderRequest::V1(request);
		let transport: Arc<dyn Transport> = self.transport.clone();
		let stream = crate::generated::dispatcher::renderer_render(
			&self.host_subscriptions,
			transport,
			request,
		);
		// The reference lives in the stream's state, so a subscription dropped
		// unpolled still releases it. An interrupt ends the stream by contract
		// and is not polled past, so it releases there rather than waiting for
		// the caller to drop the handle.
		let stream = futures::stream::unfold(
			(stream, Some(reference), back),
			|(mut stream, reference, mut back)| async move {
				match stream.next().await? {
					Ok(truapi::versioned::renderer::ProductRendererRenderItem::V1(node)) => {
						if let Some((connection, card_id)) = back.take() {
							connection.mark_back_drawn(card_id);
						}
						Some((Ok(node), (stream, reference, back)))
					},
					Err(interrupt) => Some((
						Err(crate::interrupt::interrupt_into_latest(interrupt)),
						(stream, None, None),
					)),
				}
			},
		);
		Ok(truapi::Subscription::new(stream))
	}
}

/// Calls a funding provider's worker through its product connection, each
/// call awaiting the worker's one answer.
struct WorkerFundingLink {
	transport: Arc<SinkTransport>,
	host_subscriptions: Arc<HostInitiatedSubscriptionManager>,
}

impl WorkerFundingLink {
	fn transport(&self) -> Arc<dyn Transport> {
		self.transport.clone()
	}
}

impl FundingProviderLink for WorkerFundingLink {
	fn assign(&self, session: v01::FundingAssignment) -> BoxFuture<'static, bool> {
		use truapi::versioned::funding_provider::ProductFundingAssignRequest;
		let subscriptions = self.host_subscriptions.clone();
		let transport = self.transport();
		Box::pin(async move {
			crate::generated::dispatcher::funding_provider_assign(
				&subscriptions,
				transport,
				ProductFundingAssignRequest::V1(v01::ProductFundingAssignRequest { session }),
			)
			.await
			.is_ok()
		})
	}

	fn quote(
		&self,
		ask: latest::FundingQuoteAsk,
	) -> BoxFuture<'static, Option<v01::FundingQuoteAnswer>> {
		use truapi::versioned::funding_provider::{
			ProductFundingQuoteRequest, ProductFundingQuoteResponse,
		};
		let subscriptions = self.host_subscriptions.clone();
		let transport = self.transport();
		Box::pin(async move {
			let ProductFundingQuoteResponse::V1(answer) =
				crate::generated::dispatcher::funding_provider_quote(
					&subscriptions,
					transport,
					ProductFundingQuoteRequest::V1(v01::ProductFundingQuoteRequest { ask }),
				)
				.await
				.ok()?;
			Some(answer)
		})
	}

	fn cancel(&self, intent: String) -> BoxFuture<'static, Option<v01::FundingCancelAnswer>> {
		use truapi::versioned::funding_provider::{
			ProductFundingCancelRequest, ProductFundingCancelResponse,
		};
		let subscriptions = self.host_subscriptions.clone();
		let transport = self.transport();
		Box::pin(async move {
			let ProductFundingCancelResponse::V1(answer) =
				crate::generated::dispatcher::funding_provider_cancel(
					&subscriptions,
					transport,
					ProductFundingCancelRequest::V1(v01::ProductFundingCancelRequest { intent }),
				)
				.await
				.ok()?;
			Some(answer)
		})
	}

	fn settle_cancel(
		&self,
		intent: String,
		decision: v01::FundingCancelDecision,
	) -> BoxFuture<'static, bool> {
		use truapi::versioned::funding_provider::ProductFundingSettleCancelRequest;
		let subscriptions = self.host_subscriptions.clone();
		let transport = self.transport();
		Box::pin(async move {
			crate::generated::dispatcher::funding_provider_settle_cancel(
				&subscriptions,
				transport,
				ProductFundingSettleCancelRequest::V1(v01::ProductFundingSettleCancelRequest {
					intent,
					decision,
				}),
			)
			.await
			.is_ok()
		})
	}
}

impl ProductRuntime {
	/// Tell the core the host's contacts changed, so no handle resolves from
	/// what it cached before. For an embedder that holds only this runtime;
	/// one holding the host runtime calls it there.
	pub fn notify_contacts_changed(&self) {
		self.admin.product_runtime.connection().services().contact_handles.clear();
	}

	/// Build a product-facing host core around a platform implementation and
	/// outgoing frame sink.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.from_platform_with_config"))]
	pub fn from_platform_with_config<P>(
		platform: Arc<P>,
		host_config: PairingHostConfig,
		product: ProductContext,
		spawner: Spawner,
		sink: Arc<dyn FrameSink>,
	) -> Self
	where
		P: Platform + 'static,
	{
		Self::from_platform_with_platforms(
			platform,
			host_config,
			product,
			spawner,
			sink,
			None,
			None,
		)
	}

	/// Same as [`Self::from_platform_with_config`], with the host's chat adapter
	/// installed.
	pub fn from_platform_with_chat_platform<P>(
		platform: Arc<P>,
		host_config: PairingHostConfig,
		product: ProductContext,
		spawner: Spawner,
		sink: Arc<dyn FrameSink>,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		Self::from_platform_with_platforms(
			platform,
			host_config,
			product,
			spawner,
			sink,
			chat_platform,
			None,
		)
	}

	/// Same as [`Self::from_platform_with_config`], with both optional adapters
	/// installed.
	pub fn from_platform_with_platforms<P>(
		platform: Arc<P>,
		host_config: PairingHostConfig,
		product: ProductContext,
		spawner: Spawner,
		sink: Arc<dyn FrameSink>,
		chat_platform: Option<Arc<dyn ChatPlatform>>,
		contacts_platform: Option<Arc<dyn ContactsPlatform>>,
	) -> Self
	where
		P: Platform + 'static,
	{
		let pairing = PairingHostRuntime::with_platforms(
			platform,
			host_config,
			spawner,
			chat_platform,
			contacts_platform,
		);
		pairing.product_runtime(product, sink)
	}

	/// Build a product runtime with connection-scoped adapters.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.new"))]
	pub fn new(
		services: Arc<RuntimeServices>,
		authority: Arc<dyn ProductAuthority>,
		host_session: Arc<dyn HostSession>,
		product: ProductContext,
		adapters: ConnectionAdapters,
		sink: Arc<dyn FrameSink>,
	) -> Self {
		let session_state = host_session.session_state();
		let worker =
			(product.execution_kind == ExecutableKind::Worker).then(|| product.product_id.clone());
		let admin = HostAdmin::new(services.clone(), authority, host_session, product, adapters);
		let disposed = Arc::new(AtomicBool::new(false));
		let transport = Arc::new(SinkTransport {
			sink,
			disposed: disposed.clone(),
			has_debug: AtomicBool::new(false),
			debug: Mutex::new(None),
		});
		let host_subscriptions = Arc::new(HostInitiatedSubscriptionManager::new());
		let funding_link = worker.map(|product_id| {
			let link: Arc<dyn FundingProviderLink> = Arc::new(WorkerFundingLink {
				transport: transport.clone(),
				host_subscriptions: host_subscriptions.clone(),
			});
			services.funding().link(&product_id, link.clone());
			(product_id, link)
		});
		Self {
			core: TrUApiCore::from_product_runtime(
				admin.product_runtime.clone(),
				services.spawner.clone(),
				session_state,
			),
			admin,
			transport,
			host_subscriptions,
			funding_link,
			disposed,
			in_flight: Mutex::new(HashMap::new()),
			next_dispatch_id: AtomicU64::new(0),
		}
	}

	/// Push one SCALE-encoded protocol frame into the dispatcher.
	///
	/// Calls after [`Self::dispose`] are ignored and return `Ok(())` without
	/// decoding. If dispose happens while a dispatch is in flight, the dispatch
	/// is aborted and this method still returns `Ok(())`.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.receive_frame"))]
	pub async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), ProductRuntimeError> {
		if self.disposed.load(Ordering::Acquire) {
			return Ok(());
		}

		// Tap inbound before decode, so a corrupt frame is still observed.
		if let Some((channel_id, debug)) = self.transport.debug() {
			emit_debug(
				debug,
				DebugEvent::Frame { channel_id, dir: FrameDirection::In, bytes: frame.clone() },
			);
		}

		let message = ProtocolMessage::decode(&mut frame.as_slice())
			.map_err(|err| ProductRuntimeError::InvalidFrame { reason: err.to_string() })?;
		let Some(message) = self.host_subscriptions.handle_message(message) else {
			return Ok(());
		};
		let dispatch_id = self.next_dispatch_id.fetch_add(1, Ordering::Relaxed);
		let (abort_handle, abort_registration) = AbortHandle::new_pair();
		// Same poison recovery as `self.debug`, and for a concrete reason rather than
		// symmetry: `dispose` below holds THIS guard across its whole drain loop and
		// calls `AbortHandle::abort()` inside it, which wakes the task's waker - i.e.
		// arbitrary out-of-repo executor code, under the lock. One panicking waker
		// would poison this mutex and every later `receive_frame` would then panic
		// here, which is exactly the production-host-killing shape the debug tap
		// above was fixed for.
		//
		// Re-check under the disposal lock so a racing dispatch cannot register
		// after `dispose` has drained the active requests.
		{
			let mut in_flight =
				self.in_flight.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
			if self.disposed.load(Ordering::Acquire) {
				return Ok(());
			}
			in_flight.insert(dispatch_id, abort_handle);
		}

		let transport: Arc<dyn Transport> = self.transport.clone();
		let _ = Abortable::new(self.core.dispatch(message, transport), abort_registration).await;

		self.in_flight
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.remove(&dispatch_id);
		if self.disposed.load(Ordering::Acquire) {
			self.core.cancel_subscriptions();
		}
		Ok(())
	}

	/// Return a cloneable native control handle bound to this connection.
	pub fn control(&self) -> ProductRuntimeControl {
		ProductRuntimeControl {
			connection: self.admin.product_runtime.connection().clone(),
			transport: self.transport.clone(),
			host_subscriptions: self.host_subscriptions.clone(),
			disposed: self.disposed.clone(),
		}
	}

	/// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
	/// the session has channel material, then clears in-memory and persisted
	/// session state.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.disconnect_session"))]
	pub async fn disconnect_session(&self) {
		self.admin.disconnect_session().await;
	}

	/// Read a stored permission authorization status without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.permission_authorization_status"))]
	pub async fn permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
		self.admin.permission_authorization_status(request).await
	}

	/// Read stored permission authorization statuses without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.permission_authorization_statuses"))]
	pub async fn permission_authorization_statuses(
		&self,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
		self.admin.permission_authorization_statuses(requests).await
	}

	/// Update a stored permission authorization status. `NotDetermined`
	/// clears the stored value so the next product request prompts again.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.set_permission_authorization_status"))]
	pub async fn set_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), v01::GenericError> {
		self.admin.set_permission_authorization_status(request, status).await
	}

	/// Install a dev-only [`DebugSink`] that observes every product frame in
	/// both directions for `channel_id`. Absent by default and inert in
	/// production.
	///
	/// The sink cannot FAIL a dispatch - a panic is contained at both tap sites -
	/// but it can STALL one: `emit` is called synchronously on the frame path, and
	/// nothing here bounds how long it may take. Read [`DebugSink::emit`] before
	/// implementing one; a sink that may be slow must own its own queue and return
	/// immediately.
	pub fn set_debug_sink(&self, channel_id: ChannelId, sink: Arc<dyn DebugSink>) {
		self.transport.set_debug_sink(channel_id, sink);
	}

	/// Dispose this host core. Idempotent.
	///
	/// Disposal suppresses future outgoing frames, aborts in-flight dispatch
	/// futures, and cancels active subscriptions.
	#[instrument(skip_all, fields(runtime.method = "product_runtime.dispose"))]
	pub fn dispose(&self) {
		// Aborting under the lock can wake code that re-enters disposal.
		if self.disposed.load(Ordering::Acquire) {
			return;
		}
		{
			let mut in_flight =
				self.in_flight.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
			if self.disposed.swap(true, Ordering::AcqRel) {
				return;
			}
			for (_, handle) in in_flight.drain() {
				handle.abort();
			}
		}
		let connection = self.admin.product_runtime.connection();
		connection.detach_chat();
		connection.detach_renderer();
		connection.release_open_operations();
		if let Some((product_id, link)) = &self.funding_link {
			connection.services().funding().unlink(product_id, link);
		}
		self.host_subscriptions.close();
		self.core.cancel_subscriptions();
	}
}

struct SinkTransport {
	sink: Arc<dyn FrameSink>,
	disposed: Arc<AtomicBool>,
	/// Fast-path flag: `false` (the production default) lets the per-frame
	/// `debug()` return without touching the mutex. Set once when a sink is
	/// installed; a reader that races the install just misses one frame.
	has_debug: AtomicBool,
	debug: Mutex<Option<(ChannelId, Arc<dyn DebugSink>)>>,
}

impl SinkTransport {
	/// The installed debug sink and its channel, if any. Lock-free `None` on the
	/// production path (no sink installed); only locks once one is.
	fn debug(&self) -> Option<(ChannelId, Arc<dyn DebugSink>)> {
		if !self.has_debug.load(Ordering::Relaxed) {
			return None;
		}
		// Recover from poisoning rather than panicking. Of the fixes here, moving
		// the previous sink's `drop` out of `set_debug_sink`'s critical section
		// (below) is what removes the only reachable poisoner: nothing else run
		// under this guard can unwind, the body being an
		// `Option<(ChannelId, Arc<..>)>` clone.
		//
		// That poisoner is unreachable in what ships, and not because of the
		// profile. There are two non-test `set_debug_sink` callers: `wasm.rs`, on a
		// target that cannot unwind, and `truapi-host-cli`'s `DebugTappedRuntime`
		// behind `--debugger`, which can. Both share the property that actually
		// closes the hole: each builds a fresh `SinkTransport` per
		// `product_runtime()` and installs at most once on it, so `previous` is
		// always `None` and there is no destructor to run under the lock, whatever
		// the profile or target.
		//
		// The recovery is kept regardless, because this guard sits on the per-frame
		// path in both directions and outside `emit_debug`'s `catch_unwind`, so any
		// future in-guard work that can unwind would land in live dispatch.
		self.debug.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
	}

	fn set_debug_sink(&self, channel_id: ChannelId, sink: Arc<dyn DebugSink>) {
		// Take the previous sink out under the lock, then drop it AFTER releasing.
		// `*guard = Some(..)` would drop the old `Arc` in place, running an
		// out-of-repo destructor inside the critical section: a panic there
		// poisoned the mutex, and every subsequent frame then panicked on the
		// lock. Dropping outside keeps the destructor off the critical section.
		// It is not containment - this drop is not wrapped in `catch_unwind` - and
		// it is not necessarily the last reference either: a frame being tapped
		// concurrently holds its own clone and may be the one that drops it. That
		// is why `emit_debug` takes its clone by value and drops it inside the
		// guard, so the frame path never runs the destructor uncontained.
		let previous = self
			.debug
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.replace((channel_id, sink));
		self.has_debug.store(true, Ordering::Relaxed);
		drop(previous);
	}
}

impl Transport for SinkTransport {
	fn send(&self, message: ProtocolMessage) {
		if self.disposed.load(Ordering::Acquire) {
			return;
		}
		let encoded = message.encode();
		// Forward to the product first, then tap: the debugger is in the path
		// but never in the critical path for LATENCY - the product already has the
		// frame. That is not a claim about a hung sink: `emit_debug` runs
		// synchronously here, so a sink that never returns stalls this dispatch.
		// See `DebugSink::emit` for why that is caller-enforced.
		// Take the sink handle only AFTER delivery. Holding the
		// `Arc<dyn DebugSink>` clone across `emit_frame` - which is out-of-repo
		// (a JS callback via `WasmFrameSink`, or `WsFrameSink`) - means an unwind
		// there can drop the last reference and run an out-of-repo destructor
		// DURING the unwind, outside `emit_debug`'s `catch_unwind`. A panic while
		// panicking aborts. The window opens only if `set_debug_sink` replaced the
		// sink concurrently, which is why the inbound tap (whose window is empty)
		// did not need this shape.
		if !self.has_debug.load(Ordering::Relaxed) {
			self.sink.emit_frame(encoded);
			return;
		}
		self.sink.emit_frame(encoded.clone());
		if let Some((channel_id, debug)) = self.debug() {
			emit_debug(
				debug,
				DebugEvent::Frame { channel_id, dir: FrameDirection::Out, bytes: encoded },
			);
		}
	}

	fn on_message(
		&self,
		_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
	) -> Box<dyn FnOnce()> {
		Box::new(|| {})
	}
}

#[cfg(test)]
mod tests;
