//! `ProductRuntimeHost` adapts one product connection into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

/// Connection-scoped, host-fed action streams.
pub mod actions;
mod allowances;
/// Core-owned auth/session UI state machine.
pub mod auth_state;
mod authority;
/// In-core Bulletin preimage submission over the shared Subxt client.
pub mod bulletin_rpc;
mod capabilities;
mod chat;
pub mod contacts;
mod dotns_lookup;
mod funding;
mod host_session;
mod identity;
pub mod login_failure;
mod pairing_host;
mod payment_id;
mod product_accounts;
pub mod product_manifest;
mod product_subtree;
mod renderer;
mod ring_vrf_registry;
/// Role-neutral runtime services shared by product-facing runtimes.
pub mod services;
mod signing_host;
/// SSO remote request/response messaging over the statement store.
pub mod sso_remote;
mod sso_request_service;
mod sso_responder_service;
pub mod sso_service;
/// Statement Store and Bulletin allowance allocation.
pub mod statement_allowance;
/// `StatementStore` surface: proofs plus submit and subscribe flows.
pub mod statement_store;
mod statement_store_rpc;
#[cfg(feature = "test-host")]
mod test_resource_controls;
mod versioning;
mod vrf;

use core::{future::Future, time::Duration};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
};

pub use actions::ActionChannel;
use authority::{AuthorityCancelError, AuthoritySession};
pub use authority::{AuthorityError, BulletinAllowanceKey, ProductAuthority};
pub use chat::chat_platform_for;
pub use contacts::ContactResolutionError;
pub use native_chat::{NativeChatContact, NativeChatContactsSnapshot};

/// The host's contact picker plus the key its handles are minted under:
/// everything one `contacts.pick` call needs from the connection.
type ContactsPicker = (
    Arc<dyn crate::platform::ContactsPlatform>,
    crate::runtime::contacts::ContactHandles,
);
use crate::platform::{
    AccountAccessReview, ChatFieldError, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, PermissionDecision, Platform, ProductContext, ProductStorageKey,
    SessionUiInfo, UserConfirmationReview, normalize_chat_identifier, normalize_product_identifier,
    validate_chat_icon, validate_chat_message_content, validate_chat_name,
};
use futures::{FutureExt, StreamExt, pin_mut};
#[cfg(test)]
use pairing_host::PairingHost;
pub use pairing_host::PairingHost as PairingHostRole;
use payment_id::host_funding_intent;
use product_accounts::ProductAccounts;
pub use renderer::renderer_access_for;
pub use services::RuntimeServices;
#[cfg(any(test, not(target_arch = "wasm32")))]
pub use signing_host::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use signing_host::TrackedStatementRenewalTarget;
pub use signing_host::{
    AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
    PairingProposal, PairingProposalMetadata, ResponderExit,
};
pub use signing_host::{
    LocalActivation, SigningHost as SigningHostRole, SigningHostSsoService, disconnect_paired_host,
    establish_pairing, notify_pairing_allowance_allocation, notify_pairing_failed,
    respond_to_pairing, resume_pairing,
};
pub use signing_host::{LocalIdentity, LocalIdentityContext, WalletAllowanceSnapshot};
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, CancellationReason, Subscription,
	api::{Chat, Contacts, Funding, Pocket, Renderer},
	latest, v01,
	versioned::{
		IntoLatest,
		chat::{
			HostChatActionSubscribeError, HostChatActionSubscribeItem,
			HostChatActionSubscribeRequest, HostChatCreateRoomError, HostChatCreateRoomRequest,
			HostChatCreateRoomResponse, HostChatListSubscribeError, HostChatListSubscribeItem,
			HostChatListSubscribeRequest, HostChatPostMessageError, HostChatPostMessageRequest,
			HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
			HostChatRegisterBotResponse, HostChatSetRoomFooterError, HostChatSetRoomFooterRequest,
			HostChatSetRoomFooterResponse,
		},
		contacts::{HostContactsPickError, HostContactsPickRequest, HostContactsPickResponse},
		funding::{
			HostFundingError, HostFundingRequest, HostFundingResponse,
			HostFundingStatusSubscribeError, HostFundingStatusSubscribeItem,
			HostFundingStatusSubscribeRequest,
		},
		pocket::{
			HostPocketListSubscribeError, HostPocketListSubscribeItem,
			HostPocketListSubscribeRequest, HostPocketRemoveCardError, HostPocketRemoveCardRequest,
			HostPocketRemoveCardResponse,
		},
		renderer::{
			HostRendererActionSubscribeError, HostRendererActionSubscribeItem,
			HostRendererActionSubscribeRequest,
		},
	},
};
use truapi::versioned::chat::{
    HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
    HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
    HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
};
use truapi::versioned::contacts::{
    HostContactsPickError, HostContactsPickRequest, HostContactsPickResponse,
    HostContactsPickManyError, HostContactsPickManyRequest, HostContactsPickManyResponse,
    HostContactsPlaceLabelsError, HostContactsPlaceLabelsRequest, HostContactsPlaceLabelsResponse,
};
use truapi::versioned::pocket::{
    HostPocketListSubscribeError, HostPocketListSubscribeItem, HostPocketListSubscribeRequest,
    HostPocketRemoveCardError, HostPocketRemoveCardRequest, HostPocketRemoveCardResponse,
};
use truapi::versioned::preimage::RemotePreimageSubmitError;
use truapi::versioned::profile::{
    HostProfileDiscloseError, HostProfileDiscloseRequest, HostProfileDiscloseResponse,
    HostProfileOwnStatusError, HostProfileOwnStatusRequest, HostProfileOwnStatusResponse,
    HostProfilePlaceContactAvatarsError, HostProfilePlaceContactAvatarsRequest,
    HostProfilePlaceContactAvatarsResponse, HostProfilePresentContactError,
    HostProfilePresentContactRequest, HostProfilePresentContactResponse, HostProfilePresentError,
    HostProfilePresentOwnError, HostProfilePresentOwnRequest, HostProfilePresentOwnResponse,
    HostProfilePresentRequest, HostProfilePresentResponse, HostProfileRetractError,
    HostProfileRetractRequest, HostProfileRetractResponse,
};
use truapi::versioned::renderer::{
    HostRendererActionSubscribeError, HostRendererActionSubscribeItem,
    HostRendererActionSubscribeRequest,
};
use truapi::{CallContext, CallError, CancellationReason, Subscription, v01};
#[cfg(all(target_arch = "wasm32", feature = "test-host"))]
pub use vrf::ring_vrf_member;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(test)]
use crate::subscription::Spawner;
use crate::{
	chain_runtime::RuntimeFailure,
	host_internal::{
		bulletin::preimage_key,
		permissions::{PermissionsService, TemporaryPermissions, cross_product_signature_key},
		product_manifest::Granted,
		sso_messages::RingVrfError,
	},
	host_logic::{
		product_account::{
			derivation_index_bytes, derive_product_public_key, public_key_from_address,
		},
		session::{SessionInfo, SessionState},
		sso::pairing::x25519_public_key,
	},
};

/// Error reason surfaced to products when a permission is not granted.
pub const PERMISSION_DENIED_REASON: &str = "Permission denied";
/// Host-spec B.6.2 recommends timing out unanswered SSO application requests
/// after 180 seconds:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L303-L307>
pub const DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
/// After a cancel or deadline, how long a remote authority call is given to
/// observe the cancellation and unwind (unsubscribing its statement streams)
/// before it is dropped. A call parked in the statement-store setup, which does
/// not watch the cancel token, would otherwise outlive its deadline, so this
/// caps the wait. A normal unwind completes in well under this.
const AUTHORITY_CANCEL_UNWIND_GRACE: Duration = Duration::from_secs(2);
/// Resource allocation may include a People -> Bulletin cross-chain
/// propagation before the signing host can truthfully report `Allocated`.
/// Keep this above the signing host's 240-second propagation ceiling while
/// still bounding an unanswered request.
const RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);
/// End-to-end timeout for an in-core Bulletin preimage submit, starting after
/// the permission check. It covers allowance allocation, chain submission, and
/// one optional allowance refresh and retry. A first live allocation may need
/// to fund and register the identity before the submit can start.
const PREIMAGE_SUBMIT_TIMEOUT: Duration = Duration::from_secs(360);
/// Per-request cap for obtaining or refreshing the Bulletin allowance. The
/// end-to-end submit deadline may reduce it further.
const PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT: Duration =
	RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT;
/// Cap on waiting for an extended Bulletin authorization to reach this host's
/// Bulletin node. The claim is on chain once the extension returns, so only
/// propagation is left, and the submit keeps the rest of its deadline.
const PREIMAGE_BULLETIN_AUTHORIZATION_WAIT: Duration = Duration::from_secs(18);

const LEGACY_PRODUCT_ACCOUNT_MISMATCH_REASON: &str =
	"Account can't be derived from product account id";
const LEGACY_ACCOUNT_UNAVAILABLE_REASON: &str = "Account is not available in the active session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacySigner {
	Product,
	Identity([u8; 32]),
}

#[derive(Debug, PartialEq, Eq)]
enum LegacySignerError {
	Unavailable,
	ProductDerivation(String),
}

impl LegacySignerError {
	fn into_reason(self, unavailable_reason: &'static str) -> String {
		match self {
			Self::Unavailable => unavailable_reason.to_string(),
			Self::ProductDerivation(reason) => reason,
		}
	}

	fn into_host_error(self, unavailable_reason: &'static str) -> latest::HostSignPayloadError {
		latest::HostSignPayloadError::Unknown { reason: self.into_reason(unavailable_reason) }
	}
}

fn remote_authority_context(cx: &CallContext) -> CallContext {
	remote_authority_context_with_default(cx, DEFAULT_REMOTE_AUTHORITY_RESPONSE_TIMEOUT)
}

fn remote_authority_context_with_default(
	cx: &CallContext,
	default_timeout: Duration,
) -> CallContext {
	let mut cx = cx.clone();
	if cx.timeout().is_none() {
		cx.set_timeout(default_timeout);
	}
	cx
}

fn remote_authority_context_until(
	cx: &CallContext,
	default_timeout: Duration,
	deadline: Instant,
) -> CallContext {
	let mut cx = cx.clone();
	let timeout = cx
		.timeout()
		.unwrap_or(default_timeout)
		.min(deadline.saturating_duration_since(Instant::now()));
	cx.set_timeout(timeout);
	cx
}

async fn remote_authority_call<T, E, F>(cx: &CallContext, call: F) -> Result<T, E>
where
	F: Future<Output = Result<T, E>>,
	E: From<AuthorityError>,
{
	// A call already withdrawn never sends its request, not even within the
	// unwind grace below.
	if let Some(reason) = cx.cancel().reason() {
		return Err(authority_cancellation_error(cx, reason).into());
	}
	let call = call.fuse();
	let cancelled = cx.cancel().cancelled().fuse();
	pin_mut!(call, cancelled);

	// First, run the call against cancellation and the optional deadline. A
	// completed call returns directly; a cancel or timeout yields its reason and
	// falls through to a bounded unwind.
	let reason = if let Some(timeout_duration) = cx.timeout() {
		let timeout = futures_timer::Delay::new(timeout_duration).fuse();
		pin_mut!(timeout);
		futures::select! {
			result = call => return result,
			reason = cancelled => reason,
			() = timeout => {
				let reason = CancellationReason::TimedOut {
					timeout: timeout_duration,
				};
				cx.cancel().cancel_with_reason(reason.clone());
				reason
			}
		}
	} else {
		futures::select! {
			result = call => return result,
			reason = cancelled => reason,
		}
	};

	// Give the call a bounded chance to observe the cancellation and tear down
	// its statement-store subscriptions, then drop it. The grace caps a call
	// parked in the setup region that never observes the token.
	let error = authority_cancellation_error(cx, reason);
	let unwind = futures_timer::Delay::new(AUTHORITY_CANCEL_UNWIND_GRACE).fuse();
	pin_mut!(unwind);
	futures::select! {
		_ = call => {},
		() = unwind => {},
	}
	Err(error.into())
}

/// Await `wait` unless the call is cancelled first.
///
/// For waits a person controls, such as a local confirmation prompt: a
/// withdrawn call stops waiting, so an answer given after the withdrawal
/// authorizes nothing. The error is the one `remote_authority_call` answers,
/// so each caller maps it into its method's own domain error.
async fn until_cancelled<T>(
	cx: &CallContext,
	wait: impl Future<Output = T>,
) -> Result<T, AuthorityError> {
	let wait = wait.fuse();
	let cancelled = cx.cancel().cancelled().fuse();
	pin_mut!(wait, cancelled);
	futures::select_biased! {
		reason = cancelled => Err(authority_cancellation_error(cx, reason)),
		output = wait => Ok(output),
	}
}

fn authority_cancellation_error(cx: &CallContext, reason: CancellationReason) -> AuthorityError {
	AuthorityError::Cancelled(AuthorityCancelError::new(cx.request_id(), reason))
}

/// Product adapter for the generated TrUAPI dispatcher.
pub struct ProductRuntimeHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    chat_platform: Option<Arc<dyn crate::platform::ChatPlatform>>,
    contacts_platform: Option<Arc<dyn crate::platform::ContactsPlatform>>,
    /// Live OS permission state for this connection, when the host serves it.
    permission_status: Option<Arc<dyn crate::platform::PermissionStatusHost>>,
    /// Permission requests and consuming operations can arrive on different connections.
    temporary_permissions: Arc<TemporaryPermissions>,
    authority: Arc<dyn ProductAuthority>,
    product: ProductContext,
    /// Stable per-product-runtime id used to scope long-lived chain follow
    /// operation ids within one shared host runtime.
    core_instance: u64,
    chat: Arc<ActionChannel<HostChatActionSubscribeItem>>,
    renderer: Arc<ActionChannel<HostRendererActionSubscribeItem>>,
    pocket_platform: Option<Arc<dyn crate::platform::PocketPlatform>>,
    /// Host-assigned ids of this connection's open pending operations, each
    /// holding one worker reference until it ends or the connection is torn
    /// down.
    ///
    /// Scoped to the connection rather than the product, which holds because
    /// only a Worker execution reaches `begin_operation`/`end_operation` and a
    /// product has one of those at a time.
    ///
    /// The set is unbounded here. Whether a product may hold a thousand open
    /// operations is the host's call, made in `begin_operation`, since the
    /// host is what the operations keep running.
    open_operations: Mutex<HashSet<u32>>,
}

/// A connection that goes away without ending its operations still owes the
/// ledger their references, so the host is told to stop rather than keeping a
/// worker alive for a product that is gone.
impl Drop for ProductRuntimeHost {
    fn drop(&mut self) {
        self.release_open_operations();
        self.release_contact_avatars();
        self.release_contact_labels();
    }
}

impl ProductRuntimeHost {
    /// Build a product-scoped dispatcher target from a long-lived host runtime
    /// and the adapters scoped to this product connection.
    pub fn from_services(
        services: Arc<RuntimeServices>,
        adapters: crate::host_core::ConnectionAdapters,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
    ) -> Self {
        let core_instance = services.next_core_instance();
        Self {
            services,
            platform: adapters.platform,
            chat_platform: adapters.chat_platform,
            contacts_platform: adapters.contacts_platform,
            permission_status: adapters.permission_status,
            temporary_permissions: adapters.permission_grants,
            authority,
            product,
            core_instance,
            chat: adapters.chat,
            renderer: adapters.renderer,
            pocket_platform: adapters.pocket_platform,
            open_operations: Mutex::new(HashSet::new()),
        }
    }

	/// Shared connection state used by product control handles.
	pub fn connection(&self) -> &Arc<ProductConnection> {
		&self.connection
	}

	/// Test constructor building a standalone pairing-host runtime.
	#[cfg(test)]
	pub fn new<P>(
		platform: Arc<P>,
		config: (crate::platform::PairingHostConfig, ProductContext),
		spawner: Spawner,
	) -> Self
	where
		P: Platform + 'static,
	{
		let (host_config, product) = config;
		let platform: Arc<dyn Platform> = platform;
		Self::new_pairing_for_tests(platform, host_config, product, spawner).0
	}

	/// Compatibility constructor used only by tests that do not exercise
	/// product-scoped behavior.
	#[cfg(test)]
	fn new_compat(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
		Self::new_compat_with_pairing(platform, spawner).0
	}

	#[cfg(test)]
	fn compat_host_config() -> crate::platform::PairingHostConfig {
		crate::platform::PairingHostConfig::new(
			crate::platform::HostInfo {
				name: "Polkadot Web".to_string(),
				icon: Some("https://example.invalid/dotli.png".to_string()),
				version: None,
				platform: truapi::latest::HostPlatform::Web,
			},
			crate::platform::PlatformInfo::default(),
			[0; 32],
			[0xbb; 32],
			[0xcc; 32],
			"polkadotapp".to_string(),
		)
		.expect("compat runtime config is valid")
	}

	/// Compat host used by preimage tests.
	#[cfg(test)]
	fn new_compat_with_bulletin(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
		Self::new_pairing_for_tests(
			platform,
			Self::compat_host_config(),
			ProductContext::new(ProductId::for_test("unknown.dot")),
			spawner,
		)
		.0
	}

	#[cfg(test)]
	fn new_compat_with_pairing(
		platform: Arc<dyn Platform>,
		spawner: Spawner,
	) -> (Self, Arc<PairingHost>) {
		let host_config = Self::compat_host_config();
		Self::new_pairing_for_tests(
			platform,
			host_config,
			ProductContext::new(ProductId::for_test("unknown.dot")),
			spawner,
		)
	}

	#[cfg(test)]
	fn new_pairing_for_tests(
		platform: Arc<dyn Platform>,
		host_config: crate::platform::PairingHostConfig,
		product: ProductContext,
		spawner: Spawner,
	) -> (Self, Arc<PairingHost>) {
		let services = RuntimeServices::new(
			platform.clone(),
			host_config.host.host_info.clone(),
			host_config.people_chain_genesis_hash,
			host_config.bulletin_chain_genesis_hash,
			host_config.asset_hub_chain_genesis_hash,
			spawner.clone(),
		);
		let pairing_host = PairingHost::new(services.clone(), host_config);
		let core_instance = services.next_core_instance();
		let chain = services.chain.for_product(core_instance);
		let chat = Arc::new(ActionChannel::chat());
		let renderer = Arc::new(ActionChannel::renderer());
		let connection = Arc::new(ProductConnection {
			services,
			platform,
			chat_platform: None,
			permission_status: None,
			temporary_permissions: Arc::default(),
			session_state: pairing_host.session().session_state(),
			product,
			core_instance,
			chain,
			chat,
			renderer,
			pocket_platform: None,
			game_platform: None,
			expanded_card: None,
			open_operations: Mutex::new(HashSet::new()),
		});
		let host = Self {
			accounts: ProductAccounts::new(pairing_host.clone(), connection.product.clone()),
			connection,
			host_session: pairing_host.session().clone(),
		};
		(host, pairing_host)
	}

	/// Test-only access to the shared session-state holder.
	#[cfg(test)]
	pub fn test_session_state(&self) -> Arc<SessionState> {
		self.host_session.session_state()
	}

    #[cfg(test)]
    fn new_pairing_for_tests(
        platform: Arc<dyn Platform>,
        host_config: crate::platform::PairingHostConfig,
        product: ProductContext,
        spawner: Spawner,
    ) -> (Self, Arc<PairingHost>) {
        let services = RuntimeServices::new(
            platform.clone(),
            host_config.host.host_info.clone(),
            host_config.people_chain_genesis_hash,
            host_config.bulletin_chain_genesis_hash,
            host_config.asset_hub_chain_genesis_hash,
            spawner.clone(),
        );
        let pairing_host = PairingHost::new(services.clone(), host_config);
        let core_instance = services.next_core_instance();
        let chat = Arc::new(ActionChannel::chat());
        let renderer = Arc::new(ActionChannel::renderer());
        let host = Self {
            services,
            platform,
            chat_platform: None,
            contacts_platform: None,
            permission_status: None,
            temporary_permissions: Arc::default(),
            authority: pairing_host.clone(),
            product,
            core_instance,
            chat,
            renderer,
            pocket_platform: None,
            open_operations: Mutex::new(HashSet::new()),
        };
        (host, pairing_host)
    }

	/// Disconnect this runtime from its paired signing host.
	#[cfg(test)]
	#[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
	pub async fn disconnect(&self) {
		self.host_session.disconnect().await;
	}

	/// Whether this product may use `owner`'s accounts.
	pub async fn may_use_product_account(&self, owner: &ProductId, cx: &CallContext) -> bool {
		if self.connection.product_id().is_localhost() {
			return true;
		}
		let cx = remote_authority_context(cx);
		self.holds_cross_product_scope_within(owner, Granted::Context, &cx).await
	}

	/// [`Self::holds_cross_product_scope`], answering `false` if the caller's
	/// deadline passes or the call is cancelled first.
	pub async fn holds_cross_product_scope_within(
		&self,
		target: &ProductId,
		scope: Granted,
		cx: &CallContext,
	) -> bool {
		let lookup = self.connection.holds_cross_product_scope(target, scope).fuse();
		let cancelled = cx.cancel().cancelled().fuse();
		pin_mut!(lookup, cancelled);
		let Some(budget) = cx.timeout() else {
			return futures::select! {
				resolved = lookup => resolved,
				_ = cancelled => false,
			};
		};
		let deadline = futures_timer::Delay::new(budget).fuse();
		pin_mut!(deadline);
		futures::select! {
			resolved = lookup => resolved,
			_ = cancelled => false,
			() = deadline => false,
		}
	}

	async fn product_account_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		owner: &ProductId,
		derivation_index: &latest::DerivationIndex,
	) -> Result<[u8; 32], AuthorityError> {
		let cx = remote_authority_context(cx);
		let subtree = remote_authority_call(
			&cx,
			self.accounts.product_subtree_public_key(session, &cx, owner),
		)
		.await?;
		derive_product_public_key(subtree, derivation_index_bytes(derivation_index))
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	async fn legacy_slot_zero_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<[u8; 32], String> {
		self.product_account_public_key(
			cx,
			session,
			self.connection.product_id(),
			&v01::DerivationIndex::Index(0),
		)
		.await
		.map_err(|err| err.to_string())
	}

	async fn classify_legacy_address_signer(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		signer: &str,
	) -> Result<LegacySigner, LegacySignerError> {
		let requested_key = parse_legacy_signer_hex(signer)
			.or_else(|| public_key_from_address(signer))
			.ok_or(LegacySignerError::Unavailable)?;
		self.classify_legacy_signer(cx, session, requested_key).await
	}

	async fn classify_legacy_signer(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		requested_key: [u8; 32],
	) -> Result<LegacySigner, LegacySignerError> {
		if session.identity_account_id == Some(requested_key) {
			return Ok(LegacySigner::Identity(requested_key));
		}
		let product_public_key = self
			.legacy_slot_zero_public_key(cx, session)
			.await
			.map_err(LegacySignerError::ProductDerivation)?;
		if requested_key == product_public_key {
			return Ok(LegacySigner::Product);
		}
		Err(LegacySignerError::Unavailable)
	}

	/// Replace declared contact handles with their accounts before review or signing.
	pub async fn substitute_declared_contacts(
		&self,
		call_data: Vec<u8>,
		declared: &[v01::ContactHandle],
	) -> Result<Vec<u8>, ContactResolutionError> {
		let cache = &self.connection.services.contact_handles;
		let declared_bytes: Vec<[u8; 32]> = declared.iter().map(|handle| handle.bytes).collect();
		if cache.has_undeclared_handle(&call_data, &declared_bytes) {
			return Err(ContactResolutionError::UnknownContact);
		}
		if declared.is_empty() {
			return Ok(call_data);
		}
		let (platform, handles) = self.contacts_picker().map_err(|error| match error {
			CallError::Unsupported => ContactResolutionError::Unsupported,
			CallError::Domain(v01::HostContactsPickError::NotConnected) => {
				ContactResolutionError::NotConnected
			},
			CallError::Domain(v01::HostContactsPickError::Unknown { reason }) => {
				ContactResolutionError::Host(reason)
			},
			other => ContactResolutionError::Host(format!("{other:?}")),
		})?;
		let mut resolved: Vec<([u8; 32], Option<[u8; 32]>)> = declared_bytes
			.iter()
			.map(|handle| (*handle, cache.get(handle, &handles)))
			.collect();
		// The host is asked only about handles the cache cannot answer, all
		// of them in one lookup.
		let misses: Vec<[u8; 32]> = resolved
			.iter()
			.filter(|(_, account)| account.is_none())
			.map(|(handle, _)| *handle)
			.collect();
		if !misses.is_empty() {
			let generation = cache.generation();
			let lookup = crate::platform::HostContactLookup {
				handle_key: handles.handle_key(),
				handles: misses,
			};
			let matches = platform
				.contacts(&lookup)
				.await
				.map_err(|error| ContactResolutionError::Host(error.reason))?;
			// One answer per handle, or the answers cannot be paired up.
			if matches.accounts.len() != lookup.handles.len() {
				return Err(ContactResolutionError::Host(format!(
					"contacts lookup answered {} of {} handles",
					matches.accounts.len(),
					lookup.handles.len()
				)));
			}
			let mut answers = matches.accounts.into_iter();
			for (handle, account) in resolved.iter_mut().filter(|(_, account)| account.is_none()) {
				let answer = answers.next().expect("one answer per miss; qed");
				// A host answer is checked, not trusted: an account that does
				// not hash to its handle is treated as no contact at all.
				*account = answer.filter(|account| handles.names(handle, account));
				if let Some(account) = account {
					cache.insert(*handle, *account, generation);
				}
			}
		}
		crate::host_logic::contact_substitution::substitute(&call_data, &resolved)
			.map_err(|_| ContactResolutionError::UnknownContact)
	}

    async fn product_account_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_account_id: &v01::ProductAccountId,
    ) -> Result<[u8; 32], AuthorityError> {
        let cx = remote_authority_context(cx);
        let subtree = remote_authority_call(
            &cx,
            self.authority.product_subtree_public_key(
                &cx,
                session,
                product_account_id.dot_ns_identifier.clone(),
            ),
        )
        .await?;
        derive_product_public_key(
            subtree,
            derivation_index_bytes(&product_account_id.derivation_index),
        )
        .map_err(|err| AuthorityError::Unknown {
            reason: err.to_string(),
        })
    }

    async fn legacy_slot_zero_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
    ) -> Result<[u8; 32], String> {
        self.product_account_public_key(
            cx,
            session,
            &v01::ProductAccountId {
                dot_ns_identifier: self.product_id(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
        )
        .await
        .map_err(|err| err.to_string())
    }

    /// The storage key `owner` holds `key` under.
    ///
    /// The owner is explicit because a read may be addressed at another product:
    /// deriving it from `self` would hand a granted foreign read the caller's own
    /// values instead of the ones it asked for.
    ///
    /// `owner` must already be normalized — either this product's validated id or
    /// an id returned by [`Self::cross_product_scope_target`]. `ProductStorageKey`
    /// re-applies the same normalization, so the key cannot fail to build.
    fn product_storage_key(&self, owner: &str, key: String) -> String {
        ProductStorageKey::new(owner, key)
            .expect("storage key owner was already normalized")
            .encode()
    }

    fn follow_id(&self, id: &str) -> String {
        format!("c{}:{id}", self.core_instance)
    }
}

impl ProductRuntimeHost {
    /// Read a stored permission authorization status without prompting.
    ///
    /// A device capability also resolves the host application's OS gate, so an
    /// OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_status"))]
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        let service = self.permissions_service();
        service.authorization_status(&request).await
    }

    /// Read stored permission authorization statuses without prompting.
    ///
    /// A device capability also resolves the host application's OS gate, so an
    /// OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_statuses"))]
    pub async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        let service = self.permissions_service();
        service.authorization_statuses(&requests).await
    }

    /// Update a stored permission authorization status. `NotDetermined`
    /// clears the stored value so the next product request prompts again.
    #[instrument(skip_all, fields(runtime.method = "permissions.set_authorization_status"))]
    pub async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        let service = self.permissions_service();
        let contacts_changed = matches!(request, PermissionAuthorizationRequest::ChatAuthority);
        if contacts_changed {
            self.services.invalidate_contacts();
        }
        let result = service.set_authorization_status(&request, status).await;
        if contacts_changed {
            self.services.invalidate_contacts();
        }
        result
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.remote_authorization"))]
    async fn remote_permission_authorization(
        &self,
        permission: v01::RemotePermission,
    ) -> Result<PermissionAuthorizationStatus, String> {
        let service = self.permissions_service();
        service
            .authorize_remote(v01::RemotePermissionRequest { permission })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    /// Gate a remote call on `permission`, prompting the user when it is
    /// undetermined. Anything short of `Authorized` fails with `denied_error`.
    pub async fn require_remote_permission<E>(
        &self,
        permission: v01::RemotePermission,
        denied_error: E,
    ) -> Result<(), CallError<E>> {
        match self.remote_permission_authorization(permission).await {
            Ok(PermissionAuthorizationStatus::Authorized) => Ok(()),
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => Err(CallError::Domain(denied_error)),
            Err(reason) => Err(CallError::HostFailure { reason }),
        }
    }

    async fn confirm_product_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        if crate::platform::has_trusted_remote_permissions(&self.product_id()) {
            return Ok(true);
        }
        self.platform.confirm_user_action(review).await
    }

    async fn require_chain_submit<E>(&self, denied_error: E) -> Result<(), CallError<E>> {
        self.require_remote_permission(v01::RemotePermission::ChainSubmit, denied_error)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.identity_disclosure_authorization"))]
    async fn identity_disclosure_authorization(
        &self,
    ) -> Result<PermissionAuthorizationStatus, String> {
        self.permissions_service()
            .check_or_prompt_identity_disclosure()
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.chat_authority_authorization"))]
    async fn chat_authority_authorization(
        &self,
    ) -> Result<crate::host_internal::permissions::ChatAuthorityConsent, String> {
        self.permissions_service()
            .check_or_prompt_chat_authority()
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    async fn classify_legacy_address_signer(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        signer: &str,
    ) -> Result<LegacySigner, LegacySignerError> {
        let requested_key = parse_legacy_signer_hex(signer)
            .or_else(|| public_key_from_address(signer))
            .ok_or(LegacySignerError::Unavailable)?;
        self.classify_legacy_signer(cx, session, requested_key)
            .await
    }

    async fn classify_legacy_signer(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        requested_key: [u8; 32],
    ) -> Result<LegacySigner, LegacySignerError> {
        if session.identity_account_id == Some(requested_key) {
            return Ok(LegacySigner::Identity(requested_key));
        }
        let product_public_key = self
            .legacy_slot_zero_public_key(cx, session)
            .await
            .map_err(LegacySignerError::ProductDerivation)?;
        if requested_key == product_public_key {
            return Ok(LegacySigner::Product);
        }
        Err(LegacySignerError::Unavailable)
    }
}

async fn account_access_authorization(
	platform: &dyn Platform,
	requesting_product_id: &ProductId,
	target_product_id: &ProductId,
) -> Result<PermissionAuthorizationStatus, AccountAccessAuthorizationError> {
	if requesting_product_id == target_product_id ||
		requesting_product_id.has_trusted_remote_permissions()
	{
		return Ok(PermissionAuthorizationStatus::Authorized);
	}

	let target = target_product_id.bare_label();
	let caller = requesting_product_id.bare_label();
	let cached = crate::host_internal::permissions::account_access_status(platform, caller, target)
		.await
		.map_err(AccountAccessAuthorizationError::PermissionStorage)?;
	if cached != PermissionAuthorizationStatus::NotDetermined {
		return Ok(cached);
	}

	let decision = platform
		.confirm_permission(UserConfirmationReview::AccountAccess(AccountAccessReview {
			requesting_product_id: requesting_product_id.clone(),
			target_product_id: target_product_id.clone(),
		}))
		.await
		.map_err(AccountAccessAuthorizationError::Confirmation)?;
	let status = match decision {
		PermissionDecision::AllowOnce => return Ok(PermissionAuthorizationStatus::Authorized),
		PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
		PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
	};
	crate::host_internal::permissions::set_account_access_status(platform, caller, target, status)
		.await
		.map_err(AccountAccessAuthorizationError::PermissionStorage)?;
	Ok(status)
}

#[derive(Debug, thiserror::Error)]
enum AccountAccessAuthorizationError {
	#[error("permission storage failed: {0:?}")]
	PermissionStorage(v01::GenericError),
	#[error("account access confirmation failed: {0:?}")]
	Confirmation(v01::GenericError),
}

fn parse_legacy_signer_hex(signer: &str) -> Option<[u8; 32]> {
	let raw = signer
		.strip_prefix("0x")
		.or_else(|| signer.strip_prefix("0X"))
		.unwrap_or(signer);
	if raw.len() != 64 {
		return None;
	}
	hex::decode(raw).ok()?.try_into().ok()
}

fn runtime_failure_to_call_error<E>(failure: RuntimeFailure) -> CallError<E> {
	CallError::HostFailure { reason: failure.reason() }
}

/// Host-UI projection of an active session for `AuthState::Connected`.
fn connected_session_ui_info(session: &SessionInfo) -> SessionUiInfo {
	SessionUiInfo {
		public_key: session.public_key,
		identity_account_id: session.identity_account_id,
		chat_public_key: session.identity_chat_private_key.map(x25519_public_key),
		device_enc_public_key: session.device_enc_public_key,
		peer_statement_account_id: session.sso.as_ref().map(|sso| sso.identity_account_id),
		device_statement_account_id: session.sso.as_ref().map(|sso| sso.ss_public_key),
		lite_username: session.lite_username.clone(),
		full_username: session.full_username.clone(),
	}
}

const MAX_VRF_TRANSCRIPT_ITEMS: usize = 32;
const MAX_VRF_TRANSCRIPT_BYTES: usize = 8 * 1024;

fn validate_vrf_transcript(request: &latest::HostAccountSignVrfRequest) -> Result<(), String> {
	if request.items.len() > MAX_VRF_TRANSCRIPT_ITEMS {
		return Err(format!(
			"VRF transcript has {} items, at most {MAX_VRF_TRANSCRIPT_ITEMS} are allowed",
			request.items.len()
		));
	}
	let total_bytes =
		request.items.iter().try_fold(request.transcript_label.len(), |total, item| {
			total
				.checked_add(item.label.len())
				.and_then(|total| total.checked_add(item.value.len()))
		});
	if total_bytes.is_none_or(|total| total > MAX_VRF_TRANSCRIPT_BYTES) {
		return Err(format!("VRF transcript exceeds the {MAX_VRF_TRANSCRIPT_BYTES}-byte limit"));
	}
	Ok(())
}

fn vrf_call_error(err: AuthorityError) -> CallError<latest::HostAccountSignVrfError> {
	CallError::Domain(err.into())
}
fn account_get_authority_error(err: AuthorityError) -> CallError<latest::HostAccountGetError> {
	let error = match err {
		AuthorityError::Disconnected => latest::HostAccountGetError::NotConnected,
		AuthorityError::Rejected => latest::HostAccountGetError::Rejected,
		AuthorityError::Cancelled(err) => {
			latest::HostAccountGetError::Unknown { reason: err.to_string() }
		},
		AuthorityError::Unavailable { reason } |
		AuthorityError::NotSupported { reason } |
		AuthorityError::Unknown { reason } => latest::HostAccountGetError::Unknown { reason },
	};
	CallError::Domain(error)
}

fn ring_vrf_alias_error(err: RingVrfError) -> latest::HostAccountGetAliasError {
	match err {
		RingVrfError::RingNotFound => latest::HostAccountGetAliasError::RingNotFound,
		RingVrfError::NotMember => latest::HostAccountGetAliasError::NotMember,
		RingVrfError::KeyNotRegistered => latest::HostAccountGetAliasError::KeyNotRegistered,
		RingVrfError::KeyNotInRing => latest::HostAccountGetAliasError::KeyNotInRing,
		RingVrfError::NotAllowlisted => latest::HostAccountGetAliasError::Rejected,
		RingVrfError::Rejected => latest::HostAccountGetAliasError::Rejected,
		RingVrfError::Unknown { reason } => latest::HostAccountGetAliasError::Unknown { reason },
	}
}

fn ring_vrf_proof_error(err: RingVrfError) -> latest::HostAccountCreateProofError {
	match err {
		RingVrfError::RingNotFound => latest::HostAccountCreateProofError::RingNotFound,
		RingVrfError::NotMember => latest::HostAccountCreateProofError::NotMember,
		RingVrfError::KeyNotRegistered => latest::HostAccountCreateProofError::KeyNotRegistered,
		RingVrfError::KeyNotInRing => latest::HostAccountCreateProofError::KeyNotInRing,
		RingVrfError::NotAllowlisted => latest::HostAccountCreateProofError::NotAllowlisted,
		RingVrfError::Rejected => latest::HostAccountCreateProofError::Rejected,
		RingVrfError::Unknown { reason } => latest::HostAccountCreateProofError::Unknown { reason },
	}
}

fn ring_vrf_register_error(err: RingVrfError) -> latest::HostAccountRegisterRingVrfKeyError {
	match err {
		RingVrfError::RingNotFound => latest::HostAccountRegisterRingVrfKeyError::RingNotFound,
		RingVrfError::Rejected => latest::HostAccountRegisterRingVrfKeyError::Rejected,
		RingVrfError::NotMember |
		RingVrfError::KeyNotRegistered |
		RingVrfError::KeyNotInRing |
		RingVrfError::NotAllowlisted => {
			latest::HostAccountRegisterRingVrfKeyError::Unknown { reason: format!("{err:?}") }
		},
		RingVrfError::Unknown { reason } => {
			latest::HostAccountRegisterRingVrfKeyError::Unknown { reason }
		},
	}
}

fn ring_vrf_list_error(err: RingVrfError) -> latest::HostAccountListRingVrfKeysError {
	match err {
		RingVrfError::Rejected => latest::HostAccountListRingVrfKeysError::Rejected,
		RingVrfError::RingNotFound |
		RingVrfError::NotMember |
		RingVrfError::KeyNotRegistered |
		RingVrfError::KeyNotInRing |
		RingVrfError::NotAllowlisted => {
			latest::HostAccountListRingVrfKeysError::Unknown { reason: format!("{err:?}") }
		},
		RingVrfError::Unknown { reason } => {
			latest::HostAccountListRingVrfKeysError::Unknown { reason }
		},
	}
}

fn ring_vrf_sign_error(err: RingVrfError) -> latest::HostAccountRingVrfSignError {
	match err {
		RingVrfError::KeyNotRegistered => latest::HostAccountRingVrfSignError::KeyNotRegistered,
		RingVrfError::NotAllowlisted => latest::HostAccountRingVrfSignError::NotAllowlisted,
		RingVrfError::Rejected => latest::HostAccountRingVrfSignError::Rejected,
		RingVrfError::RingNotFound | RingVrfError::NotMember | RingVrfError::KeyNotInRing => {
			latest::HostAccountRingVrfSignError::Unknown { reason: format!("{err:?}") }
		},
		RingVrfError::Unknown { reason } => latest::HostAccountRingVrfSignError::Unknown { reason },
	}
}

fn signing_call_error(err: AuthorityError) -> CallError<latest::HostSignPayloadError> {
	CallError::Domain(match err {
		AuthorityError::Rejected | AuthorityError::Disconnected => {
			latest::HostSignPayloadError::Rejected
		},
		AuthorityError::Cancelled(err) => {
			latest::HostSignPayloadError::Unknown { reason: err.to_string() }
		},
		AuthorityError::Unavailable { reason } |
		AuthorityError::NotSupported { reason } |
		AuthorityError::Unknown { reason } => latest::HostSignPayloadError::Unknown { reason },
	})
}

fn signing_call_error<E>(
    wrap: fn(v01::HostSignPayloadError) -> E,
    err: AuthorityError,
) -> CallError<E> {
    CallError::Domain(wrap(match err {
        AuthorityError::Rejected | AuthorityError::Disconnected => {
            v01::HostSignPayloadError::Rejected
        }
        AuthorityError::Cancelled(err) => v01::HostSignPayloadError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::Unavailable { reason }
        | AuthorityError::NotSupported { reason }
        | AuthorityError::Unknown { reason } => v01::HostSignPayloadError::Unknown { reason },
    }))
}

fn transaction_call_error<E>(
    wrap: fn(v01::HostCreateTransactionError) -> E,
    err: AuthorityError,
) -> CallError<E> {
    CallError::Domain(wrap(match err {
        AuthorityError::Rejected | AuthorityError::Disconnected => {
            v01::HostCreateTransactionError::Rejected
        }
        AuthorityError::Cancelled(err) => v01::HostCreateTransactionError::Unknown {
            reason: err.to_string(),
        },
        AuthorityError::NotSupported { reason } => {
            v01::HostCreateTransactionError::NotSupported { reason }
        }
        AuthorityError::Unavailable { reason } | AuthorityError::Unknown { reason } => {
            v01::HostCreateTransactionError::Unknown { reason }
        }
    }))
}

const PAYMENTS_NOT_IMPLEMENTED: &str = "Payments are not supported in dot.li";

impl ProductRuntimeHost {
    /// Chat access policy for this connection; see [`chat_platform_for`].
    pub fn native_chat_platform(
        &self,
    ) -> Result<Arc<dyn crate::platform::ChatPlatform>, crate::host_core::ProductRuntimeError> {
        chat_platform_for(
            self.product.execution_kind,
            self.authority.session_state().current().is_some(),
            self.chat_platform.as_ref(),
        )
    }

    fn chat_platform<E>(&self) -> Result<Arc<dyn crate::platform::ChatPlatform>, CallError<E>> {
        self.native_chat_platform().map_err(|error| match error {
            crate::host_core::ProductRuntimeError::Denied => CallError::Denied,
            _ => CallError::Unsupported,
        })
    }

    pub fn detach_chat(&self) {
        self.chat.detach();
    }

    /// Buffer one host-authored Chat action for this connection's product,
    /// behind the same access policy as every other Chat entry point.
    pub fn publish_chat_action(
        &self,
        action: truapi::versioned::chat::HostChatActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.native_chat_platform()?;
        self.chat.publish(action)
    }

    /// Renderer access policy for this connection; see [`renderer_access_for`].
    pub fn renderer_access(&self) -> Result<(), crate::host_core::ProductRuntimeError> {
        renderer_access_for(self.product.execution_kind)
    }

    /// Take one core-held reference on this connection's product worker, for
    /// a body the product is drawing. Pair every call with one
    /// [`Self::release_worker_reference`].
    pub fn acquire_worker_reference(&self) {
        self.services
            .worker_ledger
            .acquire(&self.product.product_id);
    }

    /// Release one core-held reference on this connection's product worker.
    pub fn release_worker_reference(&self) {
        self.services
            .worker_ledger
            .release(&self.product.product_id);
    }

    /// Begin a pending operation with the host, on a task this dispatch's
    /// cancellation cannot reach.
    ///
    /// A cancelled dispatch drops whatever it is awaiting, and dropping the
    /// host's call mid-answer would leave the host holding an operation the
    /// core never counted and the product never learned the id of, which
    /// nothing could then end. The call runs to completion either way, and
    /// ends the operation itself when nobody is left to receive it.
    pub async fn begin_operation_with_host(
        &self,
        label: String,
    ) -> Result<v01::HostWorkerBeginOperationResponse, v01::HostWorkerOperationError> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let platform = self.platform.clone();
        let product = self.product.clone();
        (self.services.spawner)(Box::pin(async move {
            let begun = platform.begin_operation(&product, label).await;
            if let Err(Ok(response)) = tx.send(begun) {
                let _ = platform.end_operation(&product, response.id).await;
            }
        }));
        rx.await.unwrap_or_else(|_| {
            Err(v01::HostWorkerOperationError::Unknown {
                reason: "the host did not answer".to_string(),
            })
        })
    }

    /// Record a pending operation and take the worker reference it holds, so
    /// an operation outliving the product's surface still reads as demand.
    pub fn hold_worker_for_operation(&self, id: u32) {
        if self
            .open_operations
            .lock()
            .expect("open operations mutex poisoned")
            .insert(id)
        {
            self.acquire_worker_reference();
        }
    }

    /// Drop every worker reference this connection's open operations hold.
    ///
    /// Teardown calls this rather than leaving it to `Drop`: a disposed
    /// connection can outlive its last `Arc` holder, and a reference kept past
    /// dispose would leave the host running a worker for a connection that is
    /// gone.
    ///
    /// Telling the host runs on the spawner, so a spawner whose runtime is
    /// already gone drops that work. The references are still released, and
    /// the host is shutting down with its own records anyway.
    pub fn release_open_operations(&self) {
        let open = core::mem::take(
            &mut *self
                .open_operations
                .lock()
                .expect("open operations mutex poisoned"),
        );
        if open.is_empty() {
            return;
        }
        for _ in &open {
            self.release_worker_reference();
        }
        // The host holds its own record of each operation, and nothing else
        // ever ends one for a connection that is gone: left alone they
        // accumulate against whatever limit the host puts on a product's open
        // operations. Ending them reaches the host, so it runs off this
        // thread.
        let platform = self.platform.clone();
        let product = self.product.clone();
        (self.services.spawner)(Box::pin(async move {
            for id in open {
                let _ = platform.end_operation(&product, id).await;
            }
        }));
    }

    /// Clear the contact avatars the host drew for this connection and stop
    /// redrawing them.
    pub(crate) fn release_contact_avatars(&self) {
        self.services
            .contact_avatars
            .release(self.core_instance, &self.services.spawner);
    }

    /// Clear this connection's host-owned contact names and prevent late draws.
    pub fn release_contact_labels(&self) {
        self.services.contact_labels.release(self.core_instance, &self.services.spawner);
    }

    /// Drop the worker reference a pending operation held. An id that is not
    /// open releases nothing, which is what keeps `end_operation` idempotent.
    pub fn release_worker_for_operation(&self, id: u32) {
        if self
            .open_operations
            .lock()
            .expect("open operations mutex poisoned")
            .remove(&id)
        {
            self.release_worker_reference();
        }
    }

    /// End the renderer action stream this connection's product is reading.
    pub fn detach_renderer(&self) {
        self.renderer.detach();
    }

    /// Buffer one renderer action for this connection's product.
    pub fn publish_renderer_action(
        &self,
        item: HostRendererActionSubscribeItem,
    ) -> Result<(), crate::host_core::ProductRuntimeError> {
        self.renderer_access()?;
        self.renderer.publish(item)
    }

    /// Pocket access policy for this connection: the collection is reachable
    /// only from a Worker execution with an active session, and only where the
    /// host installed an adapter. The kind and session checks come first, so a
    /// connection that may never reach Pocket is told `Denied` even on a host
    /// that serves nothing.
    fn pocket_platform<E>(&self) -> Result<Arc<dyn crate::platform::PocketPlatform>, CallError<E>> {
        if self.product.execution_kind != crate::platform::ProductExecutionKind::Worker
            || self.authority.session_state().current().is_none()
        {
            return Err(CallError::Denied);
        }
        self.pocket_platform.clone().ok_or(CallError::Unsupported)
    }

    /// The host's profile presenter. Any product execution may ask the host to
    /// show a profile: the host renders it in its own UI, attributed to the
    /// calling product, and nothing returns to the product.
    fn profile_platform<E>(
        &self,
    ) -> Result<Arc<dyn crate::platform::ProfilePlatform>, CallError<E>> {
        self.profile_platform.clone().ok_or(CallError::Unsupported)
    }

    /// The signed-in wallet on the Chat network, which owns the user's
    /// disclosure and what their contacts sent back: the same wallet and
    /// network the Chat actor relays for. `None` with no one signed in.
    fn profile_owner(&self) -> Option<profile::ProfileOwner> {
        let session = self.authority.session_state().current()?;
        Some(profile::ProfileOwner {
            root_public_key: session.public_key,
            genesis_hash: self.services.people_chain_genesis_hash,
        })
    }

    /// The host-verified name of `peer_identity`, this product's Chat
    /// contact, or `None` when the host knows none within
    /// [`CONTACT_USERNAME_BUDGET`]: a presented profile is not held back
    /// waiting for a slow directory.
    async fn contact_username(&self, peer_identity: &[u8; 32]) -> Option<String> {
        let session = self.authority.current_session()?;
        let product_id = self.product_id();
        let lookup = self
            .authority
            .contact_username(&session, &product_id, *peer_identity)
            .fuse();
        let deadline = futures_timer::Delay::new(CONTACT_USERNAME_BUDGET).fuse();
        pin_mut!(lookup, deadline);
        futures::select! {
            username = lookup => username,
            () = deadline => None,
        }
    }

    /// Tell the authority the disclosure changed, so open Chats relay it now
    /// rather than when their product next initializes. Never waits for the
    /// relay.
    fn profile_disclosure_changed(&self) {
        if let Some(session) = self.authority.current_session() {
            self.authority.profile_disclosure_changed(&session);
        }
    }

    /// Replace the contact handles a call declares with the accounts they
    /// name, before the call is shown to the user or signed.
    ///
    /// Substituting here rather than at the authority is what lets the
    /// confirmation show who is being paid: the host draws the review from the
    /// call it is about to sign, and by then the handle is an account it can
    /// put a name to. A call declaring no contacts reaches for no host, but is
    /// still refused if it carries a handle it forgot to declare.
    pub async fn substitute_declared_contacts(
        &self,
        call_data: Vec<u8>,
        declared: &[v01::ContactHandle],
    ) -> Result<Vec<u8>, ContactResolutionError> {
        let cache = &self.services.contact_handles;
        let declared_bytes: Vec<[u8; 32]> = declared.iter().map(|handle| handle.bytes).collect();
        if cache.has_undeclared_handle(&call_data, &declared_bytes) {
            return Err(ContactResolutionError::UnknownContact);
        }
        if declared.is_empty() {
            return Ok(call_data);
        }
        let resolved = self.resolve_contact_handles(&declared_bytes).await?;
        crate::host_logic::contact_substitution::substitute(&call_data, &resolved)
            .map_err(|_| ContactResolutionError::UnknownContact)
    }

    async fn resolve_contact_handles(
        &self,
        requested: &[[u8; 32]],
    ) -> Result<Vec<([u8; 32], Option<[u8; 32]>)>, ContactResolutionError> {
        if requested.is_empty() {
            return Ok(Vec::new());
        }
        let session = self.authority.current_session();
        let (platform, handles) = self.contacts_picker().map_err(|error| match error {
            CallError::Unsupported => ContactResolutionError::Unsupported,
            CallError::Domain(v01::HostContactsPickError::NotConnected) => {
                ContactResolutionError::NotConnected
            }
            CallError::Domain(v01::HostContactsPickError::Unknown { reason }) => {
                ContactResolutionError::Host(reason)
            }
            other => ContactResolutionError::Host(format!("{other:?}")),
        })?;
        let resolved =
            resolve_contact_accounts(&self.services, platform.as_ref(), &handles, requested)
                .await?;
        if self.authority.current_session() != session {
            return Err(ContactResolutionError::NotConnected);
        }
        Ok(resolved)
    }

    /// The contact picker for this connection, plus the key its handles are
    /// minted under.
    ///
    /// Ordered so a host that serves no picker answers `Unsupported` without an
    /// overlay ever being raised. There is no permission step: the user
    /// selecting a contact is the consent.
    fn contacts_picker(&self) -> Result<ContactsPicker, CallError<v01::HostContactsPickError>> {
        // A capability the host does not serve is a framework answer; a
        // missing session is one the product handles.
        let platform = self
            .contacts_platform
            .clone()
            .or_else(|| self.services.contacts_platform())
            .ok_or(CallError::Unsupported)?;
        let session = self
            .authority
            .current_session()
            .ok_or(CallError::Domain(v01::HostContactsPickError::NotConnected))?;
        let handle_key =
            self.authority
                .contacts_handle_key(&session)
                .map_err(|error| match error {
                    AuthorityError::Disconnected => {
                        CallError::Domain(v01::HostContactsPickError::NotConnected)
                    }
                    other => CallError::Domain(v01::HostContactsPickError::Unknown {
                        reason: other.to_string(),
                    }),
                })?;
        Ok((
            platform,
            crate::runtime::contacts::ContactHandles::from_handle_key(handle_key),
        ))
    }
}

async fn resolve_contact_accounts(
    services: &RuntimeServices,
    platform: &dyn crate::platform::ContactsPlatform,
    handles: &contacts::ContactHandles,
    requested: &[[u8; 32]],
) -> Result<Vec<([u8; 32], Option<[u8; 32]>)>, ContactResolutionError> {
    let cache = &services.contact_handles;
    let generation = cache.generation();
    let mut resolved: Vec<([u8; 32], Option<[u8; 32]>)> = requested
        .iter()
        .map(|handle| (*handle, cache.get(handle, handles)))
        .collect();
    let misses: Vec<[u8; 32]> = resolved
        .iter()
        .filter(|(_, account)| account.is_none())
        .map(|(handle, _)| *handle)
        .collect();
    if !misses.is_empty() {
        let lookup = crate::platform::HostContactLookup {
            handle_key: handles.handle_key(),
            handles: misses,
        };
        let matches = platform
            .contacts(&lookup)
            .await
            .map_err(|error| ContactResolutionError::Host(error.reason))?;
        if matches.accounts.len() != lookup.handles.len() {
            return Err(ContactResolutionError::Host(format!(
                "contacts lookup answered {} of {} handles",
                matches.accounts.len(),
                lookup.handles.len()
            )));
        }
        let mut answers = matches.accounts.into_iter();
        for (handle, account) in resolved.iter_mut().filter(|(_, account)| account.is_none()) {
            let answer = answers.next().expect("one answer per miss; qed");
            *account = answer.filter(|account| handles.names(handle, account));
            if let Some(account) = account {
                cache.insert(*handle, *account, generation);
            }
        }
    }
    if cache.generation() != generation {
        for (_, account) in &mut resolved {
            *account = None;
        }
    }
    Ok(resolved)
}

#[crate::platform::async_trait]
impl Contacts for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "contacts.pick"))]
    async fn pick(
        &self,
        cx: &CallContext,
        _request: HostContactsPickRequest,
    ) -> Result<HostContactsPickResponse, CallError<HostContactsPickError>> {
        let wrap = HostContactsPickError::V1;
        let session = self.authority.current_session();
        let (platform, handles) = self
            .contacts_picker()
            .map_err(|error| contacts_error(error, wrap))?;

				let unknown = |error: latest::GenericError| {
					CallError::Domain(latest::HostContactsPickError::Unknown {
						reason: error.reason,
					})
				};

        // Read before the picker opens: a removal signalled while the user is
        // choosing must not be undone by caching their choice.
        let generation = self.services.contact_handles.generation();
        if self.authority.current_session() != session {
            return Err(CallError::Domain(wrap(v01::HostContactsPickError::NotConnected)));
        }
        let picked = until_cancelled(cx, platform.pick_contact(&self.product))
            .await
            .map_err(|_| unknown(v01::GenericError { reason: "contact picker interrupted".into() }))?;
        if self.authority.current_session() != session {
            return Err(CallError::Domain(wrap(v01::HostContactsPickError::NotConnected)));
        }
        if self.services.contact_handles.generation() != generation || cx.cancel().is_cancelled() {
            return Err(unknown(v01::GenericError { reason: "contact picker interrupted".into() }));
        }
        let outcome = match picked.map_err(unknown)? {
            crate::platform::HostContactPick::Picked { account } => {
                let handle = handles.mint(&account);
                self.services
                    .contact_handles
                    .insert(handle, account, generation);
                v01::ContactPickOutcome::Picked {
                    handle: v01::ContactHandle { bytes: handle },
                }
            }
            crate::platform::HostContactPick::Dismissed => v01::ContactPickOutcome::Dismissed,
            crate::platform::HostContactPick::NoContacts => v01::ContactPickOutcome::NoContacts,
            // A host that resolves contacts but cannot present them is a
            // framework-level gap, not an outcome the user produced.
            crate::platform::HostContactPick::Unsupported => return Err(CallError::Unsupported),
        };
        Ok(HostContactsPickResponse::V1(
            v01::HostContactsPickResponse { outcome },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "contacts.pick_many"))]
    async fn pick_many(
        &self,
        cx: &CallContext,
        request: HostContactsPickManyRequest,
    ) -> Result<HostContactsPickManyResponse, CallError<HostContactsPickManyError>> {
        use crate::latest::{ContactHandle, ContactPickManyOutcome, HostContactsPickManyError as Error};
        let error = |error| CallError::Domain(HostContactsPickManyError::V1(error));
        let HostContactsPickManyRequest::V1(request) = request;
        let selected = contacts::selected_handles(request.selected)
            .ok_or_else(|| error(Error::InvalidSelection))?;
        let session = self.authority.current_session();
        let (platform, handles) = self.contacts_picker().map_err(|failure| match failure {
            CallError::Unsupported => CallError::Unsupported,
            CallError::Domain(v01::HostContactsPickError::NotConnected) => error(Error::NotConnected),
            _ => error(Error::Unknown { reason: "contact picker unavailable".into() }),
        })?;
        let generation = self.services.contact_handles.generation();
        if self.authority.current_session() != session {
            return Err(error(Error::NotConnected));
        }
        let resolved = until_cancelled(
            cx,
            resolve_contact_accounts(&self.services, platform.as_ref(), &handles, &selected),
        ).await.map_err(|_| error(Error::Unknown { reason: "contact lookup interrupted".into() }))?;
        if self.authority.current_session() != session {
            return Err(error(Error::NotConnected));
        }
        if self.services.contact_handles.generation() != generation || cx.cancel().is_cancelled() {
            return Err(error(Error::Unknown { reason: "contact selection interrupted".into() }));
        }
        let accounts = resolved
            .map_err(|_| error(Error::Unknown { reason: "contact lookup failed".into() }))?
            .into_iter()
            .map(|(_, account)| account)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(Error::InvalidSelection))?;
        let picked = until_cancelled(cx, platform.pick_contacts(&self.product, crate::platform::ContactSelection { selected: accounts }))
            .await
            .map_err(|_| error(Error::Unknown { reason: "contact picker interrupted".into() }))?;
        if self.authority.current_session() != session {
            return Err(error(Error::NotConnected));
        }
        if self.services.contact_handles.generation() != generation || cx.cancel().is_cancelled() {
            return Err(error(Error::Unknown { reason: "contact selection interrupted".into() }));
        }
        let outcome = match picked.map_err(|_| error(Error::Unknown {
            reason: "contact picker failed".into(),
        }))? {
            crate::platform::HostContactsPick::Picked { accounts } => {
                let accounts = contacts::selected_accounts(accounts).ok_or_else(|| error(Error::Unknown {
                    reason: "contact selection exceeds the limit".into(),
                }))?;
                let selected = accounts.into_iter().map(|account| {
                    let bytes = handles.mint(&account);
                    self.services.contact_handles.insert(bytes, account, generation);
                    ContactHandle { bytes }
                }).collect();
                ContactPickManyOutcome::Picked { handles: selected }
            }
            crate::platform::HostContactsPick::Dismissed => ContactPickManyOutcome::Dismissed,
            crate::platform::HostContactsPick::NoContacts => ContactPickManyOutcome::NoContacts,
            crate::platform::HostContactsPick::Unsupported => return Err(CallError::Unsupported),
        };
        Ok(HostContactsPickManyResponse::V1(crate::latest::HostContactsPickManyResponse { outcome }))
    }

    #[instrument(skip_all, fields(runtime.method = "contacts.place_labels"))]
    async fn place_labels(
        &self,
        cx: &CallContext,
        request: HostContactsPlaceLabelsRequest,
    ) -> Result<HostContactsPlaceLabelsResponse, CallError<HostContactsPlaceLabelsError>> {
        use crate::latest::HostContactsPlaceLabelsError as Error;
        if self.product.execution_kind != crate::platform::ProductExecutionKind::App {
            return Err(CallError::Denied);
        }
        let error = |error| CallError::Domain(HostContactsPlaceLabelsError::V1(error));
        let HostContactsPlaceLabelsRequest::V1(request) = request;
        if !contacts::valid_label_placement(&request) {
            return Err(error(Error::InvalidPlacement));
        }
        let session = self.authority.current_session();
        let (platform, handles) = self.contacts_picker().map_err(|failure| match failure {
            CallError::Unsupported => CallError::Unsupported,
            CallError::Domain(v01::HostContactsPickError::NotConnected) => error(Error::NotConnected),
            _ => error(Error::Unknown { reason: "contact labels unavailable".into() }),
        })?;
        let placement = self.services.contact_labels.for_runtime(self.core_instance, platform.clone(), &self.product);
        let mut surface = placement.surface.lock().await;
        if placement.is_closed() || cx.cancel().is_cancelled() {
            return Err(error(Error::NotConnected));
        }
        let generation = self.services.contact_handles.generation();
        if self.authority.current_session() != session {
            return Err(error(Error::NotConnected));
        }
        let requested: Vec<_> = request.slots.iter().map(|slot| slot.handle.bytes).collect();
        let resolved = until_cancelled(
            cx,
            resolve_contact_accounts(&self.services, platform.as_ref(), &handles, &requested),
        ).await.map_err(|_| error(Error::Unknown { reason: "contact lookup interrupted".into() }))?;
        if self.authority.current_session() != session {
            return Err(error(Error::NotConnected));
        }
        if self.services.contact_handles.generation() != generation || cx.cancel().is_cancelled() {
            return Err(error(Error::Unknown { reason: "contact labels interrupted".into() }));
        }
        let resolved = resolved.map_err(|_| error(Error::Unknown { reason: "contact lookup failed".into() }))?;
        let labels = request.slots.into_iter().zip(resolved).filter_map(|(slot, (_, account))| {
            account.map(|account| crate::platform::PlacedContactLabel {
                slot: slot.slot,
                account,
                rect: slot.rect,
                clip: slot.clip,
            })
        }).collect();
        if placement.is_closed() {
            return Err(error(Error::NotConnected));
        }
        *surface = Some((request.surface_width, request.surface_height, generation));
        let result = platform.place_contact_labels(&self.product, crate::platform::PlacedContactLabels {
            surface_width: request.surface_width,
            surface_height: request.surface_height,
            labels,
        }).await;
        if self.authority.current_session() != session
            || cx.cancel().is_cancelled()
            || placement.is_closed()
        {
            let _ = platform.place_contact_labels(&self.product, crate::platform::PlacedContactLabels {
                surface_width: request.surface_width,
                surface_height: request.surface_height,
                labels: Vec::new(),
            }).await;
            return Err(error(Error::NotConnected));
        }
        if matches!(result, Ok(false) | Err(Error::Unsupported)) {
            return Err(CallError::Unsupported);
        }
        Ok(HostContactsPlaceLabelsResponse::V1(crate::latest::HostContactsPlaceLabelsResponse {}))
    }
}

/// Re-wrap a latest-payload picker error into its versioned envelope.
fn contacts_error<E>(
    error: CallError<v01::HostContactsPickError>,
    wrap: fn(v01::HostContactsPickError) -> E,
) -> CallError<E> {
    match error {
        CallError::Domain(domain) => CallError::Domain(wrap(domain)),
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
        CallError::HostFailure { reason } => CallError::HostFailure { reason },
        CallError::Cancelled => CallError::Cancelled,
    }
}

#[crate::platform::async_trait]
impl Chat for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "chat.create_room"))]
	async fn create_room(
		&self,
		_cx: &CallContext,
		request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(chat_create_room_field_error)?;
				request.name = validate_chat_name("name", &request.name)
					.map_err(chat_create_room_field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon)
					.map_err(chat_create_room_field_error)?;
				platform
					.create_chat_room(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.register_bot"))]
	async fn register_bot(
		&self,
		_cx: &CallContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.bot_id = normalize_chat_identifier("botId", &request.bot_id)
					.map_err(chat_register_bot_field_error)?;
				request.name = validate_chat_name("name", &request.name)
					.map_err(chat_register_bot_field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon)
					.map_err(chat_register_bot_field_error)?;
				platform
					.register_chat_bot(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.list_subscribe"))]
	async fn list_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatListSubscribeRequest,
	) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
		version(request)
			.exec(|()| async move {
				let platform = match self.connection.chat_platform::<latest::GenericError>() {
					Ok(platform) => platform,
					Err(error) => return Subscription::interrupted(error),
				};
				Subscription::new(platform.subscribe_chat_rooms(&self.connection.product).map(
					|item| {
						item.map_err(|error| {
							warn!(
								reason = %error.reason,
								"chat room list platform stream failed"
							);
							CallError::HostFailure { reason: error.reason }
						})
					},
				))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.post_message"))]
	async fn post_message(
		&self,
		_cx: &CallContext,
		request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				// The same normalization create_room applied, so a product's own
				// spelling of a room id still resolves to the stored room.
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(chat_post_field_error)?;
				// Content is product-authored and host-rendered, so it gets the same
				// treatment the room fields get rather than reaching a host raw.
				request.payload = validate_chat_message_content(request.payload)
					.map_err(chat_post_field_error)?;
				request.alt = request
					.alt
					.map(|alt| validate_chat_name("alt", &alt))
					.transpose()
					.map_err(chat_post_field_error)?
					.filter(|alt| !alt.is_empty())
					.filter(|_| {
						matches!(request.payload, truapi::latest::ChatMessageContent::Custom(_))
					});
				platform
					.post_chat_message(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_footer"))]
	async fn set_room_footer(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				let failure = |reason: String| {
					CallError::Domain(latest::HostChatSetRoomFooterError::Unknown { reason })
				};
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(|error| failure(error.to_string()))?;

				// Checked here rather than by each host, so every host answers a room
				// the product never created the same way.
				let rooms =
					match platform.subscribe_chat_rooms(&self.connection.product).next().await {
						Some(Ok(item)) => item.rooms,
						Some(Err(error)) => return Err(failure(error.reason)),
						None => return Err(failure("the host published no room list".to_string())),
					};
				let created = rooms.iter().any(|room| {
					room.room_id == request.room_id &&
						room.participating_as == latest::ChatRoomParticipation::RoomHost
				});
				if !created {
					return Err(CallError::Domain(latest::HostChatSetRoomFooterError::UnknownRoom));
				}

				platform
					.set_chat_room_footer(&self.connection.product, request)
					.await
					.map_err(|error| failure(error.reason))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.action_subscribe"))]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		version(request)
			.exec(|()| async move {
				if let Err(error) = self.connection.chat_platform::<latest::GenericError>() {
					return Subscription::interrupted(error);
				}
				Subscription::new(
					self.connection.chat.subscribe().map(|item| item.map(IntoLatest::into_latest)),
				)
			})
			.await
	}
}

#[crate::platform::async_trait]
impl Renderer for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "renderer.action_subscribe"))]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		request: HostRendererActionSubscribeRequest,
	) -> Subscription<HostRendererActionSubscribeItem, CallError<HostRendererActionSubscribeError>>
	{
		version(request)
			.exec(|()| async move {
				if self.connection.renderer_access().is_err() {
					return Subscription::interrupted(CallError::Denied);
				}
				Subscription::new(
					self.connection
						.renderer
						.subscribe()
						.map(|item| item.map(IntoLatest::into_latest)),
				)
			})
			.await
	}
}

#[truapi::async_trait]
impl Funding for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "funding.request"))]
	async fn request(
		&self,
		_cx: &CallContext,
		request: HostFundingRequest,
	) -> Result<HostFundingResponse, CallError<HostFundingError>> {
		version(request)
			.exec(|latest::HostFundingRequest { id, direction, amount }| async move {
				// A capability the host does not serve is a framework answer; a
				// missing session is one the product handles.
				if self.connection.services.funding().platform().is_none() {
					return Err(CallError::Unsupported);
				}
				if self.accounts.current_session().is_none() {
					return Err(CallError::Domain(latest::HostFundingError::NotConnected));
				}
				self.connection
					.services
					.open_funding(
						Some(&self.connection.product),
						host_funding_intent(&self.connection.product, id),
						direction,
						amount,
					)
					.await
					.map_err(|error| match error {
						funding::OpenFundingError::Unsupported => CallError::Unsupported,
						funding::OpenFundingError::Dismissed => {
							CallError::Domain(latest::HostFundingError::Rejected)
						},
						funding::OpenFundingError::InvalidAmount => {
							CallError::Domain(latest::HostFundingError::InvalidAmount)
						},
						funding::OpenFundingError::AlreadyExists => {
							CallError::Domain(latest::HostFundingError::AlreadyExists)
						},
						funding::OpenFundingError::Stopped => {
							CallError::HostFailure { reason: error.to_string() }
						},
						funding::OpenFundingError::Present(error) => {
							CallError::HostFailure { reason: error.reason }
						},
						funding::OpenFundingError::Session(error) => {
							CallError::HostFailure { reason: error.to_string() }
						},
					})?;
				Ok(())
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "funding.status_subscribe"))]
	async fn status_subscribe(
		&self,
		_cx: &CallContext,
		request: HostFundingStatusSubscribeRequest,
	) -> Subscription<HostFundingStatusSubscribeItem, CallError<HostFundingStatusSubscribeError>> {
		version(request)
			.exec(|latest::HostFundingStatusSubscribeRequest { id }| async move {
				let registry = self.connection.services.funding();
				if registry.platform().is_none() {
					return Subscription::interrupted(CallError::Unsupported);
				}
				if let Err(error) = self.require_session() {
					return Subscription::interrupted(error);
				}
				let intent = host_funding_intent(&self.connection.product, id);
				if let Err(error) = registry.load(&self.connection.services).await {
					return Subscription::interrupted(CallError::HostFailure {
						reason: error.to_string(),
					});
				}
				// Another product's session reads as absent, so ids reveal nothing.
				let owned = registry.owner(&intent).is_some_and(|owner| {
					owner.as_ref() == Some(&self.connection.product.product_id)
				});
				match owned.then(|| registry.subscribe(&intent)).flatten() {
					Some(stream) => Subscription::new(stream.map(Ok)),
					None => Subscription::interrupted(CallError::Domain(
						latest::HostFundingStatusSubscribeError::NotFound,
					)),
				}
			})
			.await
	}
}

#[truapi::async_trait]
impl Pocket for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "pocket.list_subscribe"))]
	async fn list_subscribe(
		&self,
		_cx: &CallContext,
		request: HostPocketListSubscribeRequest,
	) -> Subscription<HostPocketListSubscribeItem, CallError<HostPocketListSubscribeError>> {
		version(request)
			.exec(|()| async move {
				let platform = match self.connection.pocket_platform::<latest::GenericError>() {
					Ok(platform) => platform,
					Err(error) => return Subscription::interrupted(error),
				};
				Subscription::new(platform.subscribe_pocket_cards(&self.connection.product).map(
					|item| {
						item.map_err(|error| {
							warn!(
								reason = %error.reason,
								"pocket card list platform stream failed"
							);
							CallError::HostFailure { reason: error.reason }
						})
					},
				))
			})
			.await
	}

    #[instrument(skip_all, fields(runtime.method = "pocket.remove_card"))]
    async fn remove_card(
        &self,
        _cx: &CallContext,
        request: HostPocketRemoveCardRequest,
    ) -> Result<HostPocketRemoveCardResponse, CallError<HostPocketRemoveCardError>> {
        let platform = self.pocket_platform()?;
        let HostPocketRemoveCardRequest::V1(mut request) = request;
        // A card id is a product-chosen label the host renders in its own
        // chrome, so it is screened before the host ever sees it: trimmed,
        // NFC-normalized, and rejected if it carries characters that let two
        // distinct ids render identically.
        request.card_id =
            normalize_chat_identifier("cardId", &request.card_id).map_err(pocket_field_error)?;
        platform
            .remove_pocket_card(&self.product, request)
            .await
            .map(|()| HostPocketRemoveCardResponse::V1)
            .map_err(|error| CallError::Domain(HostPocketRemoveCardError::V1(error)))
    }
}

/// Longest profile reference the core forwards. Seity blob references are
/// about 150 bytes; the bound leaves room for other formats without letting a
/// product push arbitrary payloads into host UI.
const MAX_PROFILE_REFERENCE_BYTES: usize = 2048;

#[truapi::async_trait]
impl Profile for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "profile.present"))]
    async fn present(
        &self,
        _cx: &CallContext,
        request: HostProfilePresentRequest,
    ) -> Result<HostProfilePresentResponse, CallError<HostProfilePresentError>> {
        let platform = self.profile_platform()?;
        let HostProfilePresentRequest::V1(request) = request;
        // The reference is opaque here; parsing it is the host's. The core
        // screens only its shape: bounded, non-empty, printable ASCII without
        // whitespace, so no control or bidi character reaches host code.
        if !is_screened_profile_reference(&request.reference) {
            return Err(CallError::Domain(HostProfilePresentError::V1(
                v01::HostProfilePresentError::InvalidReference,
            )));
        }
        platform
            .present_profile(&self.product, request)
            .await
            .map(|()| HostProfilePresentResponse::V1)
            .map_err(|error| CallError::Domain(HostProfilePresentError::V1(error)))
    }

    #[instrument(skip_all, fields(runtime.method = "profile.disclose"))]
    async fn disclose(
        &self,
        _cx: &CallContext,
        request: HostProfileDiscloseRequest,
    ) -> Result<HostProfileDiscloseResponse, CallError<HostProfileDiscloseError>> {
        // The user's own profile is disclosed from where they manage it, an
        // App, not from a background Worker.
        if self.product.execution_kind != crate::platform::ProductExecutionKind::App {
            return Err(CallError::Denied);
        }
        use truapi::latest::ProfileAudience;
        use truapi::versioned::{FromLatest, IntoLatest, Versioned};
        let version = request.version();
        let domain =
            |error| CallError::Domain(HostProfileDiscloseError::from_latest(error, version));
        let request = request.into_latest();
        if !is_screened_profile_reference(&request.reference) {
            return Err(domain(v01::HostProfileDiscloseError::InvalidReference));
        }
        let owner = self
            .profile_owner()
            .ok_or_else(|| domain(v01::HostProfileDiscloseError::NotConnected))?;
        if request.audiences.len() > 64 {
            return Err(domain(v01::HostProfileDiscloseError::Unknown {
                reason: "too many profile audiences".to_string(),
            }));
        }
        let mut all_chat_apps = false;
        let mut app_products = Vec::new();
        let mut requested_handles = Vec::new();
        for audience in request.audiences {
            match audience {
                ProfileAudience::ChatApps => all_chat_apps = true,
                ProfileAudience::App { product_id } => {
                    let product_id = normalize_product_identifier(&product_id).map_err(|_| {
                        domain(v01::HostProfileDiscloseError::Unknown {
                            reason: "invalid profile audience".to_string(),
                        })
                    })?;
                    app_products.push(product_id);
                }
                ProfileAudience::Contacts { handles } => {
                    if handles.len() > 4096usize.saturating_sub(requested_handles.len()) {
                        return Err(domain(v01::HostProfileDiscloseError::Unknown {
                            reason: "too many profile contacts".to_string(),
                        }));
                    }
                    requested_handles.extend(handles.into_iter().map(|handle| handle.bytes));
                }
            }
        }
        app_products.sort_unstable();
        app_products.dedup();
        requested_handles.sort_unstable();
        requested_handles.dedup();
        match self.profile_disclosure_authorization().await {
            Ok(PermissionAuthorizationStatus::Authorized) => {}
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => {
                return Err(domain(v01::HostProfileDiscloseError::PermissionDenied));
            }
            Err(reason) => return Err(domain(v01::HostProfileDiscloseError::Unknown { reason })),
        }
        let guard = self.services.profile_state_gate.lock().await;
        let generation = self.services.contact_handles.generation();
        let mut contacts = self
            .resolve_contact_handles(&requested_handles)
            .await
            .map_err(|_| {
                domain(v01::HostProfileDiscloseError::Unknown {
                    reason: "profile contacts unavailable".to_string(),
                })
            })?
            .into_iter()
            .map(|(_, account)| account)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                domain(v01::HostProfileDiscloseError::Unknown {
                    reason: "invalid profile audience".to_string(),
                })
            })?;
        contacts.sort_unstable();
        contacts.dedup();
        if self.profile_owner() != Some(owner) {
            return Err(domain(v01::HostProfileDiscloseError::NotConnected));
        }
        let storage = self.platform.as_ref();
        if self.services.contact_handles.generation() != generation && !contacts.is_empty() {
            return Err(domain(v01::HostProfileDiscloseError::Unknown {
                reason: "profile contacts changed".to_string(),
            }));
        }
        let now = crate::unix_time::current_unix_secs().saturating_mul(1000);
        let disclosure = profile::Disclosure {
            product_id: self.product_id(),
            reference: request.reference,
            revision: now,
            all_chat_apps,
            app_products,
            contacts,
        };
        profile::write_disclosure(storage, owner, &disclosure)
            .await
            .map_err(|reason| domain(v01::HostProfileDiscloseError::Unknown { reason }))?;
        drop(guard);
        self.profile_disclosure_changed();
        self.services
            .contact_avatars
            .redraw_owner(owner, &self.services.spawner);
        Ok(HostProfileDiscloseResponse::from_latest((), version))
    }

    #[instrument(skip_all, fields(runtime.method = "profile.retract"))]
    async fn retract(
        &self,
        _cx: &CallContext,
        _request: HostProfileRetractRequest,
    ) -> Result<HostProfileRetractResponse, CallError<HostProfileRetractError>> {
        if self.product.execution_kind != crate::platform::ProductExecutionKind::App {
            return Err(CallError::Denied);
        }
        let domain = |error| CallError::Domain(HostProfileRetractError::V1(error));
        let unknown = |reason| domain(v01::HostProfileRetractError::Unknown { reason });
        let owner = self
            .profile_owner()
            .ok_or_else(|| domain(v01::HostProfileRetractError::NotConnected))?;
        let storage = self.platform.as_ref();
        let guard = self.services.profile_state_gate.lock().await;
        match profile::read_disclosure(storage, owner)
            .await
            .map_err(unknown)?
        {
            None => Ok(HostProfileRetractResponse::V1),
            // One product may not withdraw what another disclosed.
            Some(disclosure) if disclosure.product_id != self.product_id() => {
                Err(domain(v01::HostProfileRetractError::NotDiscloser))
            }
            Some(_) => {
                profile::clear_disclosure(storage, owner)
                    .await
                    .map_err(unknown)?;
                drop(guard);
                self.profile_disclosure_changed();
                self.services
                    .contact_avatars
                    .redraw_owner(owner, &self.services.spawner);
                Ok(HostProfileRetractResponse::V1)
            }
        }
    }

    #[instrument(skip_all, fields(runtime.method = "profile.present_contact"))]
    async fn present_contact(
        &self,
        _cx: &CallContext,
        request: HostProfilePresentContactRequest,
    ) -> Result<HostProfilePresentContactResponse, CallError<HostProfilePresentContactError>> {
        let platform = self.profile_platform()?;
        use truapi::latest::ProfileContact;
        use truapi::versioned::{FromLatest, IntoLatest, Versioned};
        let version = request.version();
        let domain =
            |error| CallError::Domain(HostProfilePresentContactError::from_latest(error, version));
        let success = || HostProfilePresentContactResponse::from_latest((), version);
        let generation = self.services.contact_handles.generation();
        let contact = request.into_latest().contact;
        let owner = self
            .profile_owner()
            .ok_or_else(|| domain(v01::HostProfilePresentContactError::NotConnected))?;
        let peer_identity = match contact {
            ProfileContact::Peer { peer_identity } => peer_identity,
            ProfileContact::Handle { handle } => {
                let resolved = self.resolve_contact_handles(&[handle.bytes]).await;
                let Some(account) = resolved
                    .ok()
                    .and_then(|mut resolved| resolved.pop())
                    .and_then(|(_, account)| account)
                else {
                    return Ok(success());
                };
                account
            }
        };
        let received = if version == 1 {
            profile::received_app_reference(
                self.platform.as_ref(),
                owner,
                &self.product_id(),
                &peer_identity,
            )
            .await
        } else {
            profile::received_reference(
                self.platform.as_ref(),
                owner,
                &self.product_id(),
                &peer_identity,
            )
            .await
        };
        let received = match received {
            Ok(received) => received,
            Err(_) if version >= 2 => return Ok(success()),
            Err(reason) => {
                return Err(domain(v01::HostProfilePresentContactError::Unknown {
                    reason,
                }));
            }
        };
        let Some((reference, shared_at)) = received.and_then(|received| {
            received
                .reference
                .map(|reference| (reference, received.timestamp))
        }) else {
            return if version >= 2 {
                Ok(success())
            } else {
                Err(domain(v01::HostProfilePresentContactError::NotShared))
            };
        };
        // A stored reference passed the same screen when it arrived; check
        // again rather than trust storage.
        if !is_screened_profile_reference(&reference) {
            if version >= 2 {
                return Ok(success());
            }
            return Err(domain(
                v01::HostProfilePresentContactError::InvalidReference,
            ));
        }
        let username = self.contact_username(&peer_identity).await;
        if self.profile_owner() != Some(owner)
            || (matches!(contact, ProfileContact::Handle { .. })
                && self.services.contact_handles.generation() != generation)
        {
            return Ok(success());
        }
        let result = platform
            .present_contact_profile(
                &self.product,
                crate::platform::PresentedContactProfile {
                    reference,
                    peer_identity,
                    shared_at,
                    username,
                },
            )
            .await;
        if version >= 2 {
            return Ok(success());
        }
        result.map(|()| success()).map_err(|error| {
            domain(match error {
                v01::HostProfilePresentError::InvalidReference => {
                    v01::HostProfilePresentContactError::InvalidReference
                }
                v01::HostProfilePresentError::Unknown { reason } => {
                    v01::HostProfilePresentContactError::Unknown { reason }
                }
            })
        })
    }

    #[instrument(skip_all, fields(runtime.method = "profile.place_contact_avatars"))]
    async fn place_contact_avatars(
        &self,
        _cx: &CallContext,
        request: HostProfilePlaceContactAvatarsRequest,
    ) -> Result<
        HostProfilePlaceContactAvatarsResponse,
        CallError<HostProfilePlaceContactAvatarsError>,
    > {
        // Contacts' avatars are drawn over what the user is looking at, an
        // App, not a background Worker.
        if self.product.execution_kind != crate::platform::ProductExecutionKind::App {
            return Err(CallError::Denied);
        }
        let platform = self.profile_platform()?;
        use truapi::versioned::{FromLatest, IntoLatest, Versioned};
        let version = request.version();
        let request = request.into_latest();
        let domain = |error| {
            CallError::Domain(HostProfilePlaceContactAvatarsError::from_latest(
                error, version,
            ))
        };
        profile::avatars::validate(&request).map_err(|reason| {
            domain(v01::HostProfilePlaceContactAvatarsError::Unknown { reason })
        })?;
        let placement = self
            .services
            .contact_avatars
            .for_runtime(self.core_instance, || {
                profile::avatars::ContactAvatarPlacement::new(
                    platform,
                    self.platform.clone(),
                    self.product.clone(),
                    Arc::downgrade(&self.services),
                )
            });
        let Some(owner) = self.profile_owner() else {
            // Avatars drawn for a wallet that signed out come down with it.
            placement.clear().await;
            return Err(domain(
                v01::HostProfilePlaceContactAvatarsError::NotConnected,
            ));
        };
        let authority = request
            .slots
            .iter()
            .any(|slot| matches!(slot.contact, truapi::latest::ProfileContact::Handle { .. }))
            .then(|| Arc::downgrade(&self.authority));
        placement
            .place(owner, request, authority)
            .await
            .map(|()| HostProfilePlaceContactAvatarsResponse::from_latest((), version))
            .map_err(domain)
    }

    #[instrument(skip_all, fields(runtime.method = "profile.own_status"))]
    async fn own_status(
        &self,
        _cx: &CallContext,
        _request: HostProfileOwnStatusRequest,
    ) -> Result<HostProfileOwnStatusResponse, CallError<HostProfileOwnStatusError>> {
        let domain = |error| CallError::Domain(HostProfileOwnStatusError::V1(error));
        let owner = self
            .profile_owner()
            .ok_or_else(|| domain(v01::HostProfileOwnStatusError::NotConnected))?;
        let disclosure = profile::read_disclosure(self.platform.as_ref(), owner)
            .await
            .map_err(|reason| domain(v01::HostProfileOwnStatusError::Unknown { reason }))?;
        if disclosure
            .as_ref()
            .is_some_and(|disclosure| !is_screened_profile_reference(&disclosure.reference))
        {
            return Err(domain(v01::HostProfileOwnStatusError::Unknown {
                reason: "stored profile disclosure is invalid".into(),
            }));
        }
        Ok(HostProfileOwnStatusResponse::V1(
            v01::HostProfileOwnStatusResponse {
                configured: disclosure.is_some(),
            },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "profile.present_own"))]
    async fn present_own(
        &self,
        _cx: &CallContext,
        _request: HostProfilePresentOwnRequest,
    ) -> Result<HostProfilePresentOwnResponse, CallError<HostProfilePresentOwnError>> {
        let platform = self.profile_platform()?;
        let domain = |error| CallError::Domain(HostProfilePresentOwnError::V1(error));
        let owner = self
            .profile_owner()
            .ok_or_else(|| domain(v01::HostProfilePresentOwnError::NotConnected))?;
        let reference = profile::read_disclosure(self.platform.as_ref(), owner)
            .await
            .map_err(|reason| domain(v01::HostProfilePresentOwnError::Unknown { reason }))?
            .map(|disclosure| disclosure.reference)
            .ok_or_else(|| domain(v01::HostProfilePresentOwnError::NotConfigured))?;
        if !is_screened_profile_reference(&reference) {
            return Err(domain(v01::HostProfilePresentOwnError::InvalidReference));
        }
        platform
            .present_profile(&self.product, v01::HostProfilePresentRequest { reference })
            .await
            .map(|()| HostProfilePresentOwnResponse::V1)
            .map_err(|error| {
                domain(match error {
                    v01::HostProfilePresentError::InvalidReference => {
                        v01::HostProfilePresentOwnError::InvalidReference
                    }
                    v01::HostProfilePresentError::Unknown { reason } => {
                        v01::HostProfilePresentOwnError::Unknown { reason }
                    }
                })
            })
    }
}

fn is_screened_profile_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= MAX_PROFILE_REFERENCE_BYTES
        && reference.bytes().all(|byte| byte.is_ascii_graphic())
}

/// Report a rejected card id as a removal domain error.
fn pocket_field_error(error: ChatFieldError) -> CallError<latest::HostPocketRemoveCardError> {
	CallError::Domain(latest::HostPocketRemoveCardError::Unknown { reason: error.to_string() })
}

/// Report a rejected chat bot field as a bot-registration domain error.
fn chat_register_bot_field_error(
	error: crate::platform::ChatFieldError,
) -> CallError<latest::HostChatRegisterBotError> {
	CallError::Domain(latest::HostChatRegisterBotError::Unknown { reason: error.to_string() })
}

/// Fields whose rejection a product resolves by sending less content, and the
/// only ones the fieldless `MessageTooLarge` can describe.
const CHAT_SIZED_CONTENT_FIELDS: [&str; 2] = ["text", "payload"];

/// Maps a rejected `post_message` field onto the wire error, reporting a size
/// rejection as the size variant the protocol declares for it.
fn chat_post_field_error(error: ChatFieldError) -> CallError<latest::HostChatPostMessageError> {
	let payload = match error {
		// `MessageTooLarge` names no field, so it is reserved for the content
		// a product shrinks by sending less. Every other rejection reports
		// through `Unknown`, whose reason names the field and its limit.
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			latest::HostChatPostMessageError::MessageTooLarge
		},
		error => latest::HostChatPostMessageError::Unknown { reason: error.to_string() },
	};
	CallError::Domain(payload)
}

/// Report a rejected chat room field as a room-creation domain error.
fn chat_create_room_field_error(
	error: crate::platform::ChatFieldError,
) -> CallError<latest::HostChatCreateRoomError> {
	CallError::Domain(latest::HostChatCreateRoomError::Unknown { reason: error.to_string() })
}

impl ProductRuntimeHost {
	/// Cache a just-submitted preimage under its key for immediate lookups.
	fn prime_preimage_cache(&self, key: &[u8], value: Vec<u8>) {
		if let Ok(key_bytes) = <[u8; 32]>::try_from(key) {
			debug_assert_eq!(key_bytes, preimage_key(&value));
			self.connection.services.cache_preimage(key_bytes, value);
		}
	}
}

/// Build the product-facing `Unknown` wire error carrying `reason`.
fn preimage_submit_error(reason: String) -> CallError<latest::PreimageSubmitError> {
	CallError::Domain(latest::PreimageSubmitError::Unknown { reason })
}

fn bulletin_allowance_error_reason(err: AuthorityError) -> String {
	match err {
		AuthorityError::Rejected => {
			"Bulletin allowance allocation was rejected by the signing host".to_string()
		},
		AuthorityError::Disconnected => {
			"Signing host disconnected while allocating Bulletin allowance".to_string()
		},
		other => other.to_string(),
	}
}

#[cfg(test)]
mod tests;
