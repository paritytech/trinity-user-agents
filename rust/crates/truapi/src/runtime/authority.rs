//! Role-neutral account authority contracts used by product runtimes.
//!
//! Pairing and signing hosts implement these traits differently, but
//! `ProductRuntimeHost` can use this module's shared request/session types
//! without knowing where the key material lives.
//! Alias, proof, and ring-VRF operations reuse the request payloads in
//! `host_internal::sso_messages` for both local calls and SSO transport.

use async_trait::async_trait;
use std::sync::Arc;
use truapi::latest::{
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasRequest,
    HostAccountGetAliasResponse, HostAccountListRingVrfKeysRequest,
    HostAccountListRingVrfKeysResponse, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignRequest,
    HostAccountRingVrfSignResponse, HostAccountSignVrfError, HostAccountSignVrfRequest,
    HostCreateTransactionResponse, HostRequestResourceAllocationRequest,
    HostSignPayloadRequest, HostSignPayloadResponse,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
    HostSignRawWithLegacyAccountRequest, LegacyAccountTxPayload, ProductAccountId,
    ProductAccountTxPayload, VrfSignature,
};
use truapi::{CallContext, CancellationReason};

use crate::host_internal::extrinsic::LocalTransactionError;
use crate::host_internal::sso_messages::{ProductRequest, RingVrfError};
use crate::host_internal::transaction::ExtrinsicPayloadError;
use crate::host_logic::raw_signing::RawPayloadError;
use crate::host_logic::session::SessionInfo;
use crate::host_logic::statement_store::statement_public_key_from_secret;

/// Secret key allocated for Bulletin preimage submission.
///
/// The core is the sole holder: the secret never crosses the host boundary.
/// Zeroized on drop, and its `Debug` redacts the material.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub struct BulletinAllowanceKey {
    #[debug("\"<redacted>\"")]
    secret: [u8; 64],
}

impl BulletinAllowanceKey {
    /// Wrap a 64-byte sr25519 secret; other lengths are `Unavailable`.
    pub fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
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
    pub fn as_secret_bytes(&self) -> &[u8; 64] {
        &self.secret
    }
}

/// Persisted AutoSigning capability for one hard product subtree.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub struct AutoSigningKey {
    #[debug("\"<redacted>\"")]
    secret: [u8; 64],
    #[debug("\"<redacted>\"")]
    ring_vrf_domain_entropy: [u8; 32],
}

impl AutoSigningKey {
    pub fn from_parts(secret: [u8; 64], ring_vrf_domain_entropy: [u8; 32]) -> Self {
        Self {
            secret,
            ring_vrf_domain_entropy,
        }
    }

    pub fn as_secret_bytes(&self) -> &[u8; 64] {
        &self.secret
    }

    pub fn ring_vrf_domain_entropy(&self) -> &[u8; 32] {
        &self.ring_vrf_domain_entropy
    }
}
/// Snapshot of an account-authority session selected by the authority.
///
/// This is the neutral session projection product runtimes can use while
/// preserving authority-private material inside the concrete authority
/// implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoritySession {
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
    remote: bool,
    authorization: Option<WalletAuthorization>,
}

impl AuthoritySession {
    /// Project the neutral snapshot out of a concrete session.
    pub fn from_session_info(info: &SessionInfo, validation_id: Vec<u8>) -> Self {
        Self {
            public_key: info.public_key,
            identity_account_id: info.identity_account_id,
            lite_username: info.lite_username.clone(),
            full_username: info.full_username.clone(),
            validation_id,
            remote: false,
            authorization: None,
        }
    }

    /// Bind a request to an authenticated paired host, without native authorization.
    pub fn for_remote(mut self) -> Self {
        self.remote = true;
        self.authorization = None;
        self
    }

    /// Whether this invocation arrived through the paired-host service.
    pub fn is_remote(&self) -> bool {
        self.remote
    }

    /// Apply an activation-bound wallet authorization retained by this host.
    pub fn with_authorization(mut self, authorization: WalletAuthorization) -> Self {
        self.authorization = Some(authorization);
        self
    }

    /// Whether this local activation authorized the calling product's own account.
    pub fn authorizes(&self, caller: Option<&str>, owner: &str) -> bool {
        !self.remote
            && caller == Some(owner)
            && self.authorization.as_ref().is_some_and(|grant| {
                grant.owner == self.public_key
                    && grant.activation == self.validation_id
                    && grant.product == owner
                    && !grant.revoked.load(std::sync::atomic::Ordering::SeqCst)
            })
    }

    /// Preferred display username: full over lite, skipping empty values.
    pub fn primary_username(&self) -> Option<&str> {
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
pub enum AuthorityError {
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
    /// The embedding host could not present an approval request.
    #[display("{reason}")]
    HostFailure { reason: String },
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
            | AuthorityError::HostFailure { reason }
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
pub struct AuthorityCancelError {
    request_id: String,
    reason: CancellationReason,
}

impl AuthorityCancelError {
    /// Cancellation attributed to the request it interrupted.
    pub fn new(request_id: &str, reason: CancellationReason) -> Self {
        Self {
            request_id: request_id.to_string(),
            reason,
        }
    }
}

/// Payload-signing request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignPayloadAuthorityRequest {
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
pub enum SignRawAuthorityRequest {
    /// Sign raw data with a product-derived account.
    Product(HostSignRawRequest),
    /// Sign raw data through the legacy-account API.
    LegacyAccount {
        /// Account selected by the product and validated against the session.
        account: [u8; 32],
        /// Original legacy-account request.
        request: HostSignRawWithLegacyAccountRequest,
    },
}

/// Transaction-creation request selected by the product API entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateTransactionAuthorityRequest {
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

/// Whether blessed `calling_product_id` is using its own account, `owner`.
pub(crate) fn is_blessed_owner(calling_product_id: &str, owner: &str) -> bool {
    use crate::platform::{has_trusted_remote_permissions, normalize_product_identifier};
    normalize_product_identifier(calling_product_id).is_ok_and(|caller| {
        has_trusted_remote_permissions(&caller)
            && normalize_product_identifier(owner).is_ok_and(|owner| owner == caller)
    })
}

/// Whether a product-account call can be signed without a confirmation prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoSigningGrant {
    /// Covered: the authority already holds the signing keys and raises no prompt.
    Active,
    /// Not covered: the caller must obtain user consent.
    Absent,
}

/// Statement-store allowance signing material held by the authority layer.
#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct StatementStoreAllowanceKey {
    /// sr25519 secret used to sign allowance statements.
    pub secret: [u8; 64],
    /// Public key derived from `secret`.
    pub public_key: [u8; 32],
}

impl StatementStoreAllowanceKey {
    /// Wrap a 64-byte sr25519 secret and derive its public key; other lengths
    /// are `Unavailable`.
    pub fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
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

/// Wallet permission whose lifetime is limited to one activation.
#[derive(Clone, Debug)]
pub struct WalletAuthorization {
    owner: [u8; 32],
    activation: Vec<u8>,
    product: String,
    revoked: Arc<std::sync::atomic::AtomicBool>,
}

impl WalletAuthorization {
    /// Invalidate every in-flight clone of this activation authorization.
    pub fn revoke(&self) {
        self.revoked
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Issue permission after wallet approval for this product and activation.
    pub fn new(session: &AuthoritySession, product: String) -> Self {
        Self {
            owner: session.public_key,
            activation: session.validation_id.clone(),
            product,
            revoked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl PartialEq for WalletAuthorization {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.activation == other.activation
            && self.product == other.product
            && Arc::ptr_eq(&self.revoked, &other.revoked)
    }
}
impl Eq for WalletAuthorization {}

/// Material or authorization returned by the account holder after approval.
pub enum AccountGrant {
    /// Exported statement-store signing material.
    StatementStore(StatementStoreAllowanceKey),
    /// Exported Bulletin signing material.
    Bulletin(BulletinAllowanceKey),
    /// Exported hard-subtree and ring entropy capability.
    DelegatedSigning(AutoSigningKey),
    /// Local wallet permission, never a durable private-key grant.
    WalletAuthorization(WalletAuthorization),
    /// Allocation completed without exported signing material.
    SmartContract,
}

/// One outcome in the caller's requested resource order.
pub enum AccountAllocationOutcome {
    /// Resource authorization or material was granted.
    Allocated(AccountGrant),
    /// The wallet user refused this resource.
    Rejected,
    /// The resource cannot currently be allocated.
    NotAvailable {
        /// Failure detail retained for the authenticated wallet transcript.
        reason: Option<String>,
    },
}

impl AccountAllocationOutcome {
    /// Public outcome without host-held signing material.
    pub fn outcome(&self) -> truapi::latest::AllocationOutcome {
        match self {
            Self::Allocated(_) => truapi::latest::AllocationOutcome::Allocated,
            Self::Rejected => truapi::latest::AllocationOutcome::Rejected,
            Self::NotAvailable { .. } => truapi::latest::AllocationOutcome::NotAvailable,
        }
    }
}

/// Wallet approval, derivation and execution used by the shared product host.
///
/// WalletAccountHolder keeps root custody; SsoAccountHolderClient forwards these domain operations.
#[async_trait]
pub trait AccountHolder: Send + Sync {
    /// Current account-authority session, if connected.
    fn current_session(&self) -> Option<AuthoritySession>;

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
    /// [`AccountHolder::sign_payload`].
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
    /// [`AccountHolder::sign_payload`].
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

    /// Check wallet-side allowance readiness independently of retained host keys.
    async fn ensure_allowance_ready(
        &self,
        _session: &AuthoritySession,
        _product: &str,
        _resource: &truapi::latest::AllocatableResource,
    ) -> Result<(), AuthorityError> {
        Ok(())
    }

    /// Approve and allocate resources, returning host capabilities without caching them.
    async fn allocate_grants(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: HostRequestResourceAllocationRequest,
        policy: crate::host_internal::sso_messages::OnExistingAllowancePolicy,
    ) -> Result<Vec<AccountAllocationOutcome>, AuthorityError>;

    /// Sign an exact statement payload with the authorized product key.
    async fn sign_statement_store_product_payload(&self, cx: &CallContext, session: &AuthoritySession, calling_product_id: Option<&str>, account: ProductAccountId, payload: Vec<u8>) -> Result<[u8; 64], AuthorityError>;

    /// Derive product-scoped entropy for a connected session.
    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError>;

    /// Key material for minting contact handles.
    ///
    /// Product-independent by construction, unlike [`Self::derive_entropy`]: one
    /// contact must hash to the same handle in every product. Derived from the
    /// session's root entropy source, which both roles hold and which no product
    /// can reach — a handle keyed on anything public would be recoverable by
    /// hashing candidate accounts.
    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError>;
}

/// Build the neutral authority-session snapshot for `session`.
pub fn authority_session(session: &SessionInfo) -> AuthoritySession {
    AuthoritySession::from_session_info(session, authority_session_validation_id(session))
}

/// Opaque token identifying which concrete session a snapshot was taken from.
pub fn authority_session_validation_id(session: &SessionInfo) -> Vec<u8> {
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
