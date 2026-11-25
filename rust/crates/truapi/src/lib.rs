// The pairing-flow future nests the chain, SSO and identity futures deeply
// enough that proving the tree's auto traits exceeds the default limit.
#![recursion_limit = "256"]
#![doc = include_str!("../README.md")]
//! TrUAPI trait and type definitions for the host product SDK, and the runtime
//! hosts embed (feature `runtime`, on by default).
//!
//! Concrete wire types live in per-version modules. Versioned envelopes are in
//! [`versioned`].
//! Async API traits use the `async_trait` macro so their concise `async fn` methods
//! still guarantee `Send` futures. Implementations must annotate their impl
//! blocks with `#[async_trait::async_trait]`.
//!
//! The runtime: hosts instantiate a role runtime around a `platform::Platform`
//! implementation, then create product-scoped `ProductRuntime` endpoints that
//! expose the stable byte-frame API used from WASM, native mobile, or desktop
//! shells. Host-facing bridges:
//! - `ws_bridge`: localhost WebSocket bridge for native WebView hosts (Android/iOS).
//! - `bootstrap`: the JavaScript those hosts inject to reach that bridge.
//! - `native`: UniFFI surface exposing the native host runtime + callbacks.
//! - `wasm` (wasm32 only): wasm-bindgen surface exposing `WasmProductRuntime`.
//! - `native_debug` (non-wasm32 only): a loopback WebSocket `DebugSink` that streams tapped frames
//!   to the `@parity/truapi-debugger` app.

// Runtime code, the generated dispatcher and the macros all name this crate
// `truapi`, the way code outside it does.
extern crate self as truapi;

use core::{
	convert::Infallible,
	fmt,
	future::Future,
	mem,
	pin::Pin,
	task::{Context, Poll, Waker},
	time::Duration,
};
use std::sync::{Arc, Mutex};

use futures::Stream;
use parity_scale_codec::{Decode, Encode};

pub mod api;
mod dotns_name;
pub mod markers;
mod product_id;
pub use product_id::{ExecutableKind, PRODUCT_ID_MAX_BYTES, ProductId, ProductIdError};
pub mod v01;
pub mod v02;
pub mod versioned;

/// A 32-byte value, passed as plain bytes on FFI surfaces. Version-neutral:
/// the FFI conversion in `uniffi_types` applies to `[u8; 32]` fields in every
/// protocol version.
pub type Bytes32 = [u8; 32];

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

#[cfg(not(target_arch = "wasm32"))]
mod uniffi_types;

/// A 64-byte value, such as an sr25519 secret key, passed as plain bytes on
/// FFI surfaces. Named only because `uniffi::custom_type!` takes an ident, not
/// an array type.
pub type Bytes64 = [u8; 64];

/// Latest-version protocol payload types, unwrapped from their versioned
/// envelopes. Runtime code should use these instead of per-version modules.
///
/// The payload of an envelope carries the envelope's name. Those aliases come
/// from the `latest` module `versioned_type!` generates beside each service's
/// envelopes. Types nested in a payload are re-exported here by name.
pub mod latest {
	use crate::versioned::Versioned;

	pub use crate::v01::{
		AllocatableResource, AllocationOutcome, Arrangement, Background, BlendingMode, BorderStyle,
		BoxProps, ButtonProps, ButtonVariant, ChainIdentifier, ChatAction, ChatActionLayout,
		ChatActions, ChatBotRegistrationStatus, ChatCustomMessage, ChatFile, ChatMedia,
		ChatMessageContent, ChatReaction, ChatRichText, ChatRoom, ChatRoomParticipation,
		CodeFormat, ColorToken, ColumnProps, ContactHandle, ContactPickOutcome, ContentAlignment,
		ContextualAlias, DerivationIndex, Dimensions, DragAxis, FUNDING_DEPOSIT_URI_MAX_BYTES,
		FUNDING_DETAIL_MAX_BYTES, FUNDING_DETAIL_MAX_CHARS, FundingAssignment, FundingCancelAnswer,
		FundingCancelDecision, FundingDeposit, FundingDetail, FundingDirection, FundingFact,
		FundingFailure, FundingFrameOutcome, FundingFrameTarget, FundingInboundUpdate,
		FundingOutboundUpdate, FundingPayout, FundingQuote, FundingQuoteAnswer, FundingQuoteAsk,
		FundingQuoteRefusal, FundingRail, FundingReceived, FundingUpdate, GenericError,
		HorizontalAlignment, HostPlatform, HostSignPayloadData, HostWorkerOperationError, ImageFit,
		ImageProps, ImageRef, ImageSource, MaterialFailure, MaterialInstance, MaterialsProps,
		Modifier, NFT_PURSE_MAX_NAME_BYTES, NftPurseCheque, NftPurseChequeDestination,
		NftPurseChequeId, NftPurseChequeRecipient, NftPurseChequeRecord, NftPurseClearedItem,
		NftPurseClearingReference, NftPurseId, NftPurseIdempotencyKey, NftPurseInfoPage,
		NftPurseItemKey, NftPurseOwner, NftPursePaymentTransmissionChannel, NftPursePurse,
		NftPurseReceiveKey, NftPurseStatus, OperationStartedResult, Parameter, PaymentTopUpSource,
		PocketCard, ProductAccount, ProductAccountId, ProductProofContext, RawPayload, Region,
		RegisteredRingVrfKey, RemotePermission, RendererNode, ReplayProps, ResourceAllocationError,
		RingLocation, RingLocationJunction, RingVrfKeyDisclosure, RowProps, RuntimeApi,
		RuntimeSpec, RuntimeType, ScanOutcome, Shape, SignedStatement, Size, Statement,
		StatementProof, StatementTopic, StorageQueryItem, StorageQueryType, StorageResultItem,
		TextFieldProps, TextProps, ThemeName, ThemeVariant, TouchCondition, TouchProps,
		TouchTrigger, TrailProps, TxPayloadExtension, TypographyStyle, VerticalAlignment,
		VrfSignature, X25519PublicKey,
	};

	pub use crate::v02::{
		ChatCallClose, ChatCallRecord, ChatContactEvent, ChatCustomBody, ChatFeedUpdate,
		ChatMediaItem, ChatMediaMeta, ChatMessage, ChatMessageBody, ChatMessageOrigin,
		ChatMessageStatus, ChatPageAnchor, ChatPaymentRecord, ChatPreviewMessage,
		ChatPreviewReaction, ChatReactionDetail, ChatReactionSummary, ChatReply, ChatReplyPreview,
		ChatResource, ChatRoomActivity, ChatRoomBadge, ChatRoomConfig, ChatRoomFooter,
		ChatRoomMetadata, ChatRoomPreview, ChatRoomRegistrationStatus, ChatUserAction,
		RenderContext,
	};

	pub use crate::versioned::{
		account::latest::*, chain::latest::*, chat::latest::*, coin_payment::latest::*,
		contacts::latest::*, entropy::latest::*, expanded_card::latest::*, funding::latest::*,
		funding_provider::latest::*, game::latest::*, local_storage::latest::*, locale::latest::*,
		nft_purse::latest::*, notifications::latest::*, payment::latest::*, permissions::latest::*,
		pocket::latest::*, preimage::latest::*, renderer::latest::*,
		resource_allocation::latest::*, scanner::latest::*, signing::latest::*,
		statement_store::latest::*, system::latest::*, theme::latest::*, worker::latest::*,
	};

	/// Latest payload type of a versioned envelope.
	pub type LatestOf<T> = <T as Versioned>::Latest;
}

pub use truapi_macros::{service, wire, wire_trait};

/// Wire codec version this crate defines. Frames address a method with a
/// `(trait, method)` byte pair. The handshake accepts only this version, and
/// codegen stamps it into the generated clients, so every peer derives it
/// from here.
pub const WIRE_CODEC_VERSION: u8 = 3;

/// Framework-level outcomes shared by API methods.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CallError<D> {
	/// Method-specific failure.
	Domain(D),
	/// The caller is not allowed to perform this operation.
	Denied,
	/// The host does not support this operation.
	Unsupported,
	/// The incoming request payload could not be decoded or validated.
	MalformedFrame {
		/// Why decoding or validation failed.
		reason: String,
	},
	/// Host-side failure with a diagnostic reason.
	HostFailure {
		/// Diagnostic reason for the failure.
		reason: String,
	},
	/// The caller withdrew the request before it produced a result.
	///
	/// Appended last so the preceding variants keep their SCALE indices.
	Cancelled,
}

impl<D> CallError<D> {
	/// Transform the domain failure while preserving framework outcomes.
	pub fn map_domain<T>(self, map: impl FnOnce(D) -> T) -> CallError<T> {
		match self {
			Self::Domain(domain) => CallError::Domain(map(domain)),
			Self::Denied => CallError::Denied,
			Self::Unsupported => CallError::Unsupported,
			Self::MalformedFrame { reason } => CallError::MalformedFrame { reason },
			Self::HostFailure { reason } => CallError::HostFailure { reason },
			Self::Cancelled => CallError::Cancelled,
		}
	}

	/// Convenience for default handlers whose implementation is not wired.
	pub fn unavailable() -> Self {
		Self::HostFailure { reason: "unavailable".into() }
	}
}

/// Error type for methods with no domain-specific failures.
pub type FrameworkOnlyError = CallError<Infallible>;

/// Cooperative cancellation token exposed to handlers.
///
/// A request token fires when the peer sends a `Cancel` frame for the call,
/// when a runtime explicitly cancels it, or when an attached timeout elapses.
/// Subscription runtimes can cancel this token when the peer sends `_stop` or
/// disconnects.
#[derive(Clone, Default)]
pub struct CancellationToken {
	inner: Arc<CancellationInner>,
}

#[derive(Default)]
struct CancellationInner {
	state: Mutex<CancellationState>,
}

#[derive(Default)]
struct CancellationState {
	reason: Option<CancellationReason>,
	next_id: u64,
	wakers: Vec<(u64, Waker)>,
}

/// Cause attached to a cancelled call.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum CancellationReason {
	/// The caller or runtime explicitly cancelled the call.
	#[display("cancelled")]
	Cancelled,
	/// The call exceeded the configured timeout.
	#[display("timed out after {}", format_timeout(timeout))]
	TimedOut {
		/// Timeout that elapsed.
		timeout: Duration,
	},
}

/// Render a timeout as whole seconds when possible, milliseconds otherwise.
fn format_timeout(timeout: &Duration) -> String {
	if timeout.subsec_millis() == 0 {
		format!("{}s", timeout.as_secs())
	} else {
		format!("{}ms", timeout.as_millis())
	}
}

/// Future resolved when a [`CancellationToken`] is cancelled.
pub struct CancellationFuture {
	inner: Arc<CancellationInner>,
	id: Option<u64>,
}

impl fmt::Debug for CancellationToken {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("CancellationToken")
			.field("reason", &self.reason())
			.finish_non_exhaustive()
	}
}

impl CancellationToken {
	/// Mark the token as cancelled.
	pub fn cancel(&self) {
		self.cancel_with_reason(CancellationReason::Cancelled);
	}

	/// Mark the token as cancelled with an explicit `reason`.
	pub fn cancel_with_reason(&self, reason: CancellationReason) {
		let wakers = {
			let mut state = self.inner.state.lock().expect("cancel state poisoned");
			if state.reason.is_some() {
				return;
			}
			state.reason = Some(reason);
			mem::take(&mut state.wakers)
		};
		for (_, waker) in wakers {
			waker.wake();
		}
	}

	/// Returns the cancellation reason, if cancellation has been requested.
	pub fn reason(&self) -> Option<CancellationReason> {
		self.inner.state.lock().expect("cancel state poisoned").reason.clone()
	}

	/// Returns whether cancellation has been requested.
	pub fn is_cancelled(&self) -> bool {
		self.reason().is_some()
	}

	/// Future resolved with the cancellation reason when cancellation is requested.
	pub fn cancelled(&self) -> CancellationFuture {
		CancellationFuture { inner: self.inner.clone(), id: None }
	}
}

impl Future for CancellationFuture {
	type Output = CancellationReason;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let this = self.get_mut();
		let mut state = this.inner.state.lock().expect("cancel state poisoned");
		if let Some(reason) = state.reason.clone() {
			this.id = None;
			return Poll::Ready(reason);
		}

		if let Some(id) = this.id &&
			let Some((_, waker)) =
				state.wakers.iter_mut().find(|(waiter_id, _)| *waiter_id == id)
		{
			if !waker.will_wake(cx.waker()) {
				*waker = cx.waker().clone();
			}
			return Poll::Pending;
		}

		state.next_id = state.next_id.wrapping_add(1);
		let id = state.next_id;
		state.wakers.push((id, cx.waker().clone()));
		this.id = Some(id);
		Poll::Pending
	}
}

impl Drop for CancellationFuture {
	fn drop(&mut self) {
		let Some(id) = self.id.take() else {
			return;
		};
		let mut state = self.inner.state.lock().expect("cancel state poisoned");
		if state.reason.is_some() {
			return;
		}
		state.wakers.retain(|(waiter_id, _)| *waiter_id != id);
	}
}

/// Ambient context passed to every trait method.
#[derive(Clone, Default)]
pub struct CallContext {
	request_id: String,
	cancel: CancellationToken,
	timeout: Option<Duration>,
}

impl CallContext {
	/// Construct a context bound to the given `request_id` with a fresh cancellation token.
	pub fn with_request_id(request_id: String) -> Self {
		Self { request_id, cancel: CancellationToken::default(), timeout: None }
	}

	/// Construct a context from explicit `request_id` and `cancel` parts.
	pub fn with_parts(request_id: String, cancel: CancellationToken) -> Self {
		Self { request_id, cancel, timeout: None }
	}

	/// Attach a timeout to this call.
	pub fn set_timeout(&mut self, timeout: Duration) {
		self.timeout = Some(timeout);
	}

	/// Return the request id this context is associated with.
	pub fn request_id(&self) -> &str {
		&self.request_id
	}

	/// Return the cancellation token that signals when the call should abort.
	pub fn cancel(&self) -> &CancellationToken {
		&self.cancel
	}

	/// Return the timeout attached to this call, if any.
	pub fn timeout(&self) -> Option<Duration> {
		self.timeout
	}
}

/// Handle to an active subscription. Implements [`Stream`] to yield values
/// pushed by the host. Drop to unsubscribe.
///
/// The stream yields `Ok(item)` for each value and at most one `Err`, which
/// ends it: the runtime encodes that value as the `_interrupt` payload and
/// polls no further. A stream that ends without an `Err` interrupts with
/// `Ok(())`, which the peer reads as a normal completion.
pub struct Subscription<Item, Interrupt> {
	inner: Pin<Box<dyn Stream<Item = Result<Item, Interrupt>> + Send>>,
}

impl<Item, Interrupt> Stream for Subscription<Item, Interrupt> {
	type Item = Result<Item, Interrupt>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.inner.as_mut().poll_next(cx)
	}
}

impl<Item, Interrupt> Subscription<Item, Interrupt> {
	/// Creates a subscription from a stream of items and at most one
	/// terminating interrupt.
	pub fn new<S>(stream: S) -> Self
	where
		S: Stream<Item = Result<Item, Interrupt>> + Send + 'static,
	{
		Self { inner: Box::pin(stream) }
	}

	/// Creates a subscription that yields no items and ends with `interrupt`.
	/// The default trait bodies of unimplemented methods interrupt with
	/// [`CallError::unavailable`], so a caller sees a failure rather than a
	/// stream that finished.
	pub fn interrupted(interrupt: Interrupt) -> Self
	where
		Item: Send + 'static,
		Interrupt: Send + 'static,
	{
		Self::new(futures::stream::once(core::future::ready(Err(interrupt))))
	}
}

cfg_select! {
	feature = "runtime" => {
		pub mod background_task;
		pub mod bootstrap;
		pub mod chain;
		mod chain_runtime;
		mod dispatcher;
		mod dotns_views;
		mod dynamic_vrf;
		mod evm_contract;
		#[cfg(not(target_arch = "wasm32"))]
		mod gap_scan;
		pub mod frame;
		mod host_core;
		mod host_internal;
		pub mod host_logic;
		mod host_rpc_client;
		mod interrupt;
		pub mod logging;
		pub mod platform;
		mod protocol_error;
		mod public_profile;
		mod runtime;
		mod session_usernames;
		mod spawner;
		mod stack;
		mod stream_extensions;
		pub mod subscription;
		pub mod time;
		pub mod transport;
		mod truapi_core;

		#[cfg(test)]
		mod test_support;
		mod unix_time;
		mod wake;

		// Dispatch must keep serving deprecated APIs while clients migrate.
		#[allow(deprecated)]
		pub mod generated;

		#[cfg(not(target_arch = "wasm32"))]
		pub mod native;

		#[cfg(target_arch = "wasm32")]
		pub mod wasm;

		#[cfg(not(target_arch = "wasm32"))]
		pub mod native_debug;

		#[cfg(not(target_arch = "wasm32"))]
		pub mod store;

		#[cfg(not(target_arch = "wasm32"))]
		#[macro_use]
		pub mod durable;

		#[cfg(not(target_arch = "wasm32"))]
		pub mod encrypted_storage;

		mod image_loader;

		#[cfg(not(target_arch = "wasm32"))]
		pub mod installation;

		#[cfg(not(target_arch = "wasm32"))]
		mod installation_registry;

		#[cfg(not(target_arch = "wasm32"))]
		mod nft;

		#[cfg(not(target_arch = "wasm32"))]
		mod nft_purse;

		#[cfg(not(target_arch = "wasm32"))]
		mod pocket_cards;

		#[cfg(not(target_arch = "wasm32"))]
		mod bulletin_lookup;

		#[cfg(not(target_arch = "wasm32"))]
		mod materials;

		pub use spawner::Spawner;

		pub use truapi_core::TrUApiCore;

		#[cfg(not(target_arch = "wasm32"))]
		pub use bulletin_lookup::{BlobSource, BulletinLookup, SourceError};

		pub use host_core::{
			ChannelId, DebugEvent, DebugSink, FrameDirection, FrameSink, HostAdmin, HostRuntime,
			ProductRuntime, ProductRuntimeControl, ProductRuntimeError,
		};
		pub use host_internal::{
			bulletin::{BitswapError, BulletinCid, BulletinCidError, bitswap_get, preimage_key},
			extrinsic::Sr25519Signer,
		};
		pub use host_logic::{
			session::{
				ExternalPairedSession, SsoSessionInfo, decode_persisted_session,
				encode_external_paired_session,
			},
			worker::{WorkerLedger, WorkerTransition},
		};
		pub use host_rpc_client::HostRpcClient;
		pub use image_loader::{ImageError, ImageFormat, ImageReference, LoadedImage, check_image};
		#[cfg(not(target_arch = "wasm32"))]
		pub use materials::{
			KeepMaterialError, MaterialShader, MaterialsResolution, NodeFallback, NodeMaterials,
			ReadyLayer, ResolvedMaterials, ShaderLanguage, Stage, Texture, TextureRole,
			TouchInputs, TouchPhase, TrailImage, Uniform, UniformKind, UniformRole,
		};
		#[cfg(not(target_arch = "wasm32"))]
		pub use native::{
			NativeMaterialsObserver, NativeMaterialsResolution, NativePublicProfileObserver,
			NativeRendererObserver, NativeSubscription, NativeTouchArea, WsBridgeEndpoint,
			WsBridgeStartError,
		};
		#[cfg(not(target_arch = "wasm32"))]
		pub use native_debug::{DebugSinkError, WsDebugSink};
		pub use platform::{
			CoreStorageKeyDescription, CoreStorageKeyDescriptionError, CoreStorageKeyOwner,
			HostIdentity, PairingHostConfig, PermissionAuthorizationRequest,
			PermissionAuthorizationStatus, Platform, ProductContext, SigningHostConfig,
			describe_core_storage_key,
		};
		pub use public_profile::{
			PROFILE_RECORD_KEY, ProfileAvatar, ProfileRecord, ProfileRecordLocation, PublicProfile,
			PublicProfileError, PublicProfileResolutionOptions,
		};
		pub use runtime::{
			AnnouncedPairing, DevicePairingObserver, FundingFeed, MAX_PAIRING_METADATA_CHARS,
			PairedSsoPeer, PairingHost, PairingProposal, PairingProposalMetadata, ResponderExit,
			ResumePairingError, SigningHost, StatementRenewalTarget,
			contacts::contact_handle,
			login_failure::reports_exhausted_period,
			product_manifest::{encode_cached_root_manifest, worker_manifest_cache_key},
			statement_allowance,
		};
		pub use unix_time::current_unix_millis;

		#[cfg(all(target_arch = "wasm32", feature = "wasm-signing-host"))]
		pub use wasm::WasmSigningHostRuntime;
		#[cfg(target_arch = "wasm32")]
		pub use wasm::{
			WasmPairingHostRuntime, WasmProductRuntime, WasmRendererSubscription,
			derive_product_account_public_key, describe_core_storage_key_for_wasm,
			has_trusted_remote_permissions_for_wasm, product_account_address, set_log_level,
			wire_schema_hash,
		};
	},
	_ => {},
}

#[cfg(test)]
mod tests;
