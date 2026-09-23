//! Role-neutral account authority contracts used by product runtimes.
//!
//! Pairing and signing hosts implement these traits differently, but
//! `ProductRuntimeHost` can use this module's shared request/session types
//! without knowing where the key material lives.
//! Alias, proof, and ring-VRF operations reuse the request payloads in
//! `host_logic::sso::messages` for both local calls and SSO transport.

use async_trait::async_trait;
use std::sync::Arc;
use truapi::latest::{
    AccountId, HostAccountCreateProofRequest, HostAccountCreateProofResponse,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountListRingVrfKeysRequest,
    HostAccountListRingVrfKeysResponse, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignRequest,
    HostAccountRingVrfSignResponse, HostAccountSignVrfError, HostAccountSignVrfRequest,
    HostCreateTransactionResponse, HostProductDeviceChatError, HostProductDeviceChatRequest,
    HostProductDeviceChatResponse, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse, HostSignPayloadRequest, HostSignPayloadResponse,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
    HostSignRawWithLegacyAccountRequest, LegacyAccountTxPayload, ProductAccountId,
    ProductAccountTxPayload, VrfSignature,
};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, CancellationReason};
use truapi_platform::ProductContext;

use crate::host_logic::extrinsic::LocalTransactionError;
use crate::host_logic::raw_signing::RawPayloadError;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::sso::messages::{PaymentTopUpRequest, ProductRequest, RingVrfError};
use crate::host_logic::statement_store::statement_public_key_from_secret;
use crate::host_logic::transaction::ExtrinsicPayloadError;

/// Secret key allocated for Bulletin preimage submission.
///
/// The core is the sole holder: the secret never crosses the host boundary.
/// Zeroized on drop, and its `Debug` redacts the material.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub(crate) struct BulletinAllowanceKey {
    #[debug("\"<redacted>\"")]
    secret: [u8; 64],
}

impl BulletinAllowanceKey {
    /// Wrap a 64-byte sr25519 secret; other lengths are `Unavailable`.
    pub(crate) fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
        let secret: [u8; 64] =
            secret
                .try_into()
                .map_err(|secret: Vec<u8>| AuthorityError::Unavailable {
                    reason: format!(
                        "bulletin allowance key must be 64 bytes, got {}",
                        secret.len()
                    ),
                })?;
        Ok(Self { secret })
    }

    /// Raw secret for the in-core Bulletin signer.
    pub(crate) fn as_secret_bytes(&self) -> &[u8; 64] {
        &self.secret
    }
}

/// Persisted AutoSigning capability for one hard product subtree.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub(crate) struct AutoSigningKey {
    #[debug("\"<redacted>\"")]
    secret: [u8; 64],
    #[debug("\"<redacted>\"")]
    ring_vrf_domain_entropy: [u8; 32],
}

impl AutoSigningKey {
    pub(crate) fn from_parts(secret: [u8; 64], ring_vrf_domain_entropy: [u8; 32]) -> Self {
        Self {
            secret,
            ring_vrf_domain_entropy,
        }
    }

    pub(crate) fn as_secret_bytes(&self) -> &[u8; 64] {
        &self.secret
    }

    pub(crate) fn ring_vrf_domain_entropy(&self) -> &[u8; 32] {
        &self.ring_vrf_domain_entropy
    }
}
/// Snapshot of an account-authority session selected by the authority.
///
/// This is the neutral session projection product runtimes can use while
/// preserving authority-private material inside the concrete authority
/// implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthoritySession {
    /// Root account public key for the active authority session.
    pub public_key: [u8; 32],
    /// Identity account resolved from the signing host, when available.
    pub identity_account_id: Option<[u8; 32]>,
    /// Lightweight username resolved from the dotNS contracts on Asset Hub, when available.
    pub lite_username: Option<String>,
    /// Fully qualified username resolved from the dotNS contracts on Asset Hub, when available.
    pub full_username: Option<String>,
    /// Opaque session token used to reject stale pre-confirmation snapshots.
    pub validation_id: Vec<u8>,
}

impl AuthoritySession {
    /// Project the neutral snapshot out of a concrete session.
    pub(crate) fn from_session_info(info: &SessionInfo, validation_id: Vec<u8>) -> Self {
        Self {
            public_key: info.public_key,
            identity_account_id: info.identity_account_id,
            lite_username: info.lite_username.clone(),
            full_username: info.full_username.clone(),
            validation_id,
        }
    }

    /// Preferred display username: full over lite, skipping empty values.
    pub(crate) fn primary_username(&self) -> Option<&str> {
        self.full_username
            .as_deref()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.lite_username
                    .as_deref()
                    .filter(|value| !value.is_empty())
            })
    }
}

/// Typed account-authority failure before it is mapped to an API-specific error.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub(crate) enum AuthorityError {
    /// User or authority rejected the request.
    #[display("Rejected")]
    Rejected,
    /// The selected authority session is no longer active.
    #[display("Disconnected")]
    Disconnected,
    /// The authority call was cancelled before completion.
    #[display("{_0}")]
    Cancelled(AuthorityCancelError),
    /// The authority cannot service the request.
    #[display("{reason}")]
    Unavailable { reason: String },
    /// The authority cannot service this request shape (e.g. an unsupported
    /// transaction-extension version).
    #[display("{reason}")]
    NotSupported { reason: String },
    /// Catch-all authority failure.
    #[display("{reason}")]
    Unknown { reason: String },
}

/// A preimage this host cannot assemble is a shape failure, not a transport one.
impl From<ExtrinsicPayloadError> for AuthorityError {
    fn from(err: ExtrinsicPayloadError) -> Self {
        match err {
            ExtrinsicPayloadError::UnsupportedPayloadVersion { .. }
            | ExtrinsicPayloadError::UnsupportedSignedExtension { .. } => Self::NotSupported {
                reason: err.to_string(),
            },
            other => Self::Unknown {
                reason: other.to_string(),
            },
        }
    }
}

/// A chain the host cannot reach is `Unavailable`; a request shape it cannot
/// assemble is `NotSupported`.
impl From<LocalTransactionError> for AuthorityError {
    fn from(err: LocalTransactionError) -> Self {
        match err {
            LocalTransactionError::UnsupportedTxExtVersion { .. }
            | LocalTransactionError::UnsupportedExtensions(_) => Self::NotSupported {
                reason: err.to_string(),
            },
            LocalTransactionError::ChainUnavailable(_) => Self::Unavailable {
                reason: err.to_string(),
            },
            LocalTransactionError::Other(_) => Self::Unknown {
                reason: err.to_string(),
            },
        }
    }
}

impl From<RawPayloadError> for AuthorityError {
    fn from(err: RawPayloadError) -> Self {
        Self::Unknown {
            reason: err.to_string(),
        }
    }
}

impl From<AuthorityError> for RingVrfError {
    fn from(err: AuthorityError) -> Self {
        match err {
            AuthorityError::Rejected => RingVrfError::Rejected,
            other => RingVrfError::Unknown {
                reason: other.to_string(),
            },
        }
    }
}

impl From<AuthorityError> for HostAccountSignVrfError {
    fn from(err: AuthorityError) -> Self {
        match err {
            AuthorityError::Disconnected => Self::NotConnected,
            AuthorityError::Rejected => Self::Rejected,
            AuthorityError::Cancelled(err) => Self::Unknown {
                reason: err.to_string(),
            },
            AuthorityError::Unavailable { reason }
            | AuthorityError::NotSupported { reason }
            | AuthorityError::Unknown { reason } => Self::Unknown { reason },
        }
    }
}

/// Cancellation cause for an account-authority call.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display(
    "Account authority request {reason}{}",
    if request_id.is_empty() { String::new() } else { format!(" for {request_id}") }
)]
pub(crate) struct AuthorityCancelError {
    request_id: String,
    reason: CancellationReason,
}

impl AuthorityCancelError {
    /// Cancellation attributed to the request it interrupted.
    pub(crate) fn new(request_id: &str, reason: CancellationReason) -> Self {
        Self {
            request_id: request_id.to_string(),
            reason,
        }
    }
}

/// Payload-signing request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignPayloadAuthorityRequest {
    /// Sign a payload with a product-derived account.
    Product(HostSignPayloadRequest),
    /// Sign a payload through the legacy-account API.
    LegacyAccount {
        /// Product slot-zero account that backs the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account request.
        request: HostSignPayloadWithLegacyAccountRequest,
    },
}

/// Raw-signing request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignRawAuthorityRequest {
    /// Sign raw data with a product-derived account.
    Product(HostSignRawRequest),
    /// Sign raw data through the legacy-account API.
    LegacyAccount {
        /// Account selected by the product and validated against the session.
        account: AccountId,
        /// Original legacy-account request.
        request: HostSignRawWithLegacyAccountRequest,
    },
}

/// Transaction-creation request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreateTransactionAuthorityRequest {
    /// Create a transaction with a product-derived account.
    Product(ProductAccountTxPayload),
    /// Create a transaction through the legacy-account API using the product slot-zero account.
    LegacyAccount {
        /// Product slot-zero account that backs the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account transaction request.
        request: LegacyAccountTxPayload,
    },
    /// Create a transaction with the active wallet's identity account.
    IdentityAccount(LegacyAccountTxPayload),
}

/// Host-owned Chat operation after the caller's capability authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductDeviceChatAuthorityRequest {
    pub calling_product_id: String,
    pub operation: HostProductDeviceChatRequest,
}

/// Only trusted attachment preparation uploads preimages; products submit Chat statements.
pub(crate) fn chat_requires_preimage_submit(operation: &HostProductDeviceChatRequest) -> bool {
    match operation {
        HostProductDeviceChatRequest::PrepareAttachments { .. } => true,
        HostProductDeviceChatRequest::Initialize
        | HostProductDeviceChatRequest::Bind { .. }
        | HostProductDeviceChatRequest::Prepare { .. }
        | HostProductDeviceChatRequest::Open { .. }
        | HostProductDeviceChatRequest::SendPayment { .. }
        | HostProductDeviceChatRequest::PaymentStatus { .. }
        | HostProductDeviceChatRequest::ReconcilePayments
        | HostProductDeviceChatRequest::OpenAttachment { .. }
        | HostProductDeviceChatRequest::CommitMigration { .. }
        | HostProductDeviceChatRequest::ContinueOpen { .. }
        | HostProductDeviceChatRequest::ContinueState { .. }
        | HostProductDeviceChatRequest::PaymentDenomination => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProductDeviceChatAuthorityError {
    Disconnected,
    Rejected,
    Unavailable(String),
    Domain(HostProductDeviceChatError),
}

impl From<AuthorityError> for ProductDeviceChatAuthorityError {
    fn from(error: AuthorityError) -> Self {
        match error {
            AuthorityError::Disconnected => Self::Disconnected,
            AuthorityError::Rejected => Self::Rejected,
            other => Self::Unavailable(other.to_string()),
        }
    }
}

impl From<ProductDeviceChatAuthorityError> for truapi::v02::HostProductDeviceChatError {
    fn from(error: ProductDeviceChatAuthorityError) -> Self {
        match error {
            ProductDeviceChatAuthorityError::Disconnected => Self::NotConnected,
            ProductDeviceChatAuthorityError::Rejected => Self::UserRejected,
            ProductDeviceChatAuthorityError::Unavailable(_) => Self::NetworkUnavailable,
            ProductDeviceChatAuthorityError::Domain(error) => error,
        }
    }
}

/// Payment failures retain typed wallet results without exposing backend diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentTopUpAuthorityError {
    Authority(AuthorityError),
    Domain(truapi::v01::HostPaymentTopUpError),
}

impl From<AuthorityError> for PaymentTopUpAuthorityError {
    fn from(error: AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<PaymentTopUpAuthorityError> for truapi::v01::HostPaymentTopUpError {
    fn from(error: PaymentTopUpAuthorityError) -> Self {
        match error {
            PaymentTopUpAuthorityError::Domain(error) if !matches!(error, Self::Unknown { .. }) => {
                error
            }
            PaymentTopUpAuthorityError::Authority(AuthorityError::Disconnected) => Self::Unknown {
                reason: "Wallet session is not active".to_string(),
            },
            PaymentTopUpAuthorityError::Authority(AuthorityError::Cancelled(_)) => Self::Unknown {
                reason: "Payment top-up cancelled".to_string(),
            },
            // Backend failures may contain supplied source keys; never echo them.
            _ => Self::Unknown {
                reason: "Payment top-up unavailable".to_string(),
            },
        }
    }
}

/// Whether an active AutoSigning grant covers one product-account call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoSigningGrant {
    /// Covered: the matching authority call serves this request from local key
    /// material and raises no prompt anywhere.
    Active,
    /// Not covered: the caller must obtain user consent.
    Absent,
}
/// Statement-store allowance signing material held by the authority layer.
#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub(crate) struct StatementStoreAllowanceKey {
    /// sr25519 secret used to sign allowance statements.
    #[debug("\"<redacted>\"")]
    pub(crate) secret: [u8; 64],
    /// Public key derived from `secret`.
    pub(crate) public_key: [u8; 32],
}

impl StatementStoreAllowanceKey {
    /// Wrap a 64-byte sr25519 secret and derive its public key; other lengths
    /// are `Unavailable`.
    pub(crate) fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
        let secret: [u8; 64] =
            secret
                .try_into()
                .map_err(|secret: Vec<u8>| AuthorityError::Unavailable {
                    reason: format!(
                        "statement-store allowance key must be 64 bytes, got {}",
                        secret.len()
                    ),
                })?;
        let public_key = statement_public_key_from_secret(secret)
            .map_err(|reason| AuthorityError::Unavailable { reason })?;
        Ok(Self { secret, public_key })
    }
}

/// Host-level account authority used by product runtimes.
///
/// Pairing hosts implement this by forwarding authority requests to a paired
/// signing host. A signing-host implementation can later provide the same
/// surface from local keys without changing product runtime code.
#[async_trait]
pub(crate) trait ProductAuthority: Send + Sync {
    /// Current account-authority session, if connected.
    fn current_session(&self) -> Option<AuthoritySession>;

    /// Shared session holder owned by this authority.
    ///
    /// Product runtimes use it for connection-status subscriptions. The
    /// concrete authority keeps ownership of the actual session material.
    fn session_state(&self) -> Arc<SessionState>;

    /// Seed a paired product subtree in unit tests that exercise later authority calls.
    #[cfg(test)]
    fn cache_product_subtree_for_test(
        &self,
        _session: &SessionInfo,
        _product_id: &str,
        _public_key: [u8; 32],
    ) {
    }

    /// Request account connection for the calling product.
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>>;

    /// Disconnect the current account-authority session.
    async fn disconnect(&self);

    /// Refresh identity fields for the current session if the authority can do
    /// so without user interaction.
    async fn refresh_session_identity(&self) -> Option<AuthoritySession> {
        self.current_session()
    }

    /// Return the public key of `//product//{product_id}`.
    ///
    /// Pairing hosts obtain this consent-free value from the Account Holder;
    /// signing hosts derive it locally from root entropy.
    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError>;

    /// Whether resolving `product_id`'s subtree would reach the Account Holder
    /// over SSO rather than resolve locally. Gates a host consent prompt: a
    /// pairing host returns `true` only on a cold cache; a signing host derives
    /// locally and returns `false`. Required rather than defaulted, so a new
    /// authority cannot skip the consent gate by omission.
    async fn subtree_resolution_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool;

    /// Whether an active AutoSigning grant covers `account` for
    /// `calling_product_id`.
    ///
    /// Only a product's own accounts are ever covered: the grant material is
    /// one product subtree secret, so legacy and identity accounts cannot be
    /// produced from it.
    ///
    /// [`AutoSigningGrant::Active`] is a promise, not a hint: the matching
    /// `sign_*` call on this authority serves the request from local key
    /// material, without reaching a paired host and without raising a prompt on
    /// either side. The capability layer skips its consent gate on that
    /// promise, so an authority that cannot keep it answers `Absent`.
    ///
    /// `Err` is a hard failure - a broken or foreign grant slot, a stale
    /// session, unreadable core storage - and is propagated rather than
    /// downgraded into a prompt. A grant slot the authority has just decided to
    /// erase must not produce a modal asking the user to approve it.
    ///
    /// Required rather than defaulted, so a new authority can neither skip the
    /// consent gate nor silently claim a grant by omission.
    async fn auto_signing_status(
        &self,
        session: &AuthoritySession,
        calling_product_id: &str,
        account: &ProductAccountId,
    ) -> Result<AutoSigningGrant, AuthorityError>;

    /// Sign an RFC-0023 Merlin transcript with a product account.
    async fn sign_vrf(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: HostAccountSignVrfRequest,
    ) -> Result<VrfSignature, AuthorityError>;

    /// Sign a SCALE transaction payload for a product account.
    ///
    /// `calling_product_id` is the product making the call, which is not always
    /// the product the account belongs to; an authority that can serve the
    /// account locally must bind the two before it does. `None` is a path that
    /// carries no caller identity — the SSO relay — and an authority that
    /// cannot identify the caller must not serve the account locally.
    async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: Option<&str>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Sign arbitrary bytes for a product account.
    ///
    /// `calling_product_id` carries the same binding obligation as
    /// [`ProductAuthority::sign_payload`].
    async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: Option<&str>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Build a transaction for a product account, signed unless the request
    /// supplies its own V5 `VerifyMultiSignature` extension.
    /// `calling_product_id` carries the same binding obligation as
    /// [`ProductAuthority::sign_payload`].
    async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: Option<&str>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<HostCreateTransactionResponse, AuthorityError>;

    /// Derive a product-scoped contextual alias for an explicit registered key.
    ///
    /// The Account Holder resolves `key_handle` from the registry and derives
    /// the alias bound to `context`; `create_proof` derives the same alias.
    async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountGetAliasRequest>,
    ) -> Result<HostAccountGetAliasResponse, RingVrfError>;

    /// Create a ring-VRF proof bound to a context and message.
    ///
    /// Uses the request's explicit registered key, so the returned
    /// `contextual_alias` matches `account_alias` for the same inputs.
    async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountCreateProofRequest>,
    ) -> Result<HostAccountCreateProofResponse, RingVrfError>;

    /// Register a ring-VRF key owned by the calling product.
    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<HostAccountRegisterRingVrfKeyResponse, RingVrfError>;

    /// List registered ring-VRF keys.
    async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountListRingVrfKeysRequest>,
    ) -> Result<HostAccountListRingVrfKeysResponse, RingVrfError>;

    /// Sign bytes directly with a registered ring-VRF key.
    async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRingVrfSignRequest>,
    ) -> Result<HostAccountRingVrfSignResponse, RingVrfError>;

    /// Execute an authorized operation on the Host-owned native Chat device.
    async fn product_device_chat(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductDeviceChatAuthorityRequest,
    ) -> Result<HostProductDeviceChatResponse, ProductDeviceChatAuthorityError>;

    /// Credit caller-supplied incoming funding; no outgoing-spend or Chat grant.
    async fn payment_top_up(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: PaymentTopUpRequest,
    ) -> Result<(), PaymentTopUpAuthorityError>;

    /// Ask the account authority to allocate product-scoped resources.
    async fn allocate_resources(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, AuthorityError>;

    /// Return statement-store allowance key material for the calling product.
    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError>;

    /// Return Bulletin allowance key material for the calling product.
    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;

    /// Evict any cached Bulletin allowance key for the product and allocate a
    /// fresh one, increasing the existing allowance.
    ///
    /// Called after a submission is rejected for an exhausted/missing
    /// allowance, where reusing the cached key would loop forever.
    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;

    /// Sign exact statement-store proof bytes with a product-derived account.
    async fn sign_statement_store_product_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: Option<&str>,
        account: ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError>;

    /// Derive product-scoped entropy for a connected session.
    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError>;
}

/// Build the neutral authority-session snapshot for `session`.
pub(super) fn authority_session(session: &SessionInfo) -> AuthoritySession {
    AuthoritySession::from_session_info(session, authority_session_validation_id(session))
}

/// Revalidate a pre-confirmation snapshot against the live session, returning
/// the current [`SessionInfo`] when it still matches.
///
/// Both roles use this before touching key material: a snapshot taken before
/// user confirmation must still be the current authority session when the
/// signature or derivation happens, otherwise the request is rejected.
pub(super) fn require_current_session(
    session_state: &SessionState,
    session: &AuthoritySession,
) -> Result<SessionInfo, AuthorityError> {
    let current = session_state
        .current()
        .ok_or(AuthorityError::Disconnected)?;
    if authority_session_validation_id(&current) == session.validation_id {
        Ok(current)
    } else {
        Err(AuthorityError::Disconnected)
    }
}

/// Opaque token identifying which concrete session a snapshot was taken from.
pub(super) fn authority_session_validation_id(session: &SessionInfo) -> Vec<u8> {
    let mut id = Vec::with_capacity(67);
    if let Some(sso) = &session.sso {
        id.extend_from_slice(b"sso");
        id.extend_from_slice(&sso.session_id_own);
        id.extend_from_slice(&sso.session_id_peer);
    } else {
        id.extend_from_slice(b"local");
        id.extend_from_slice(&session.public_key);
    }
    id
}
