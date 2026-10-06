//! Account calls and host grant contracts.
//!
//! Caller origin separates local host permissions from remote wallet consent.

use super::WalletAuthorization;
use crate::platform::ProductContext;
use async_trait::async_trait;
use truapi::latest::{
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasRequest,
    HostAccountGetAliasResponse, HostAccountListRingVrfKeysRequest,
    HostAccountListRingVrfKeysResponse, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignRequest,
    HostAccountRingVrfSignResponse, HostAccountSignVrfError, HostAccountSignVrfRequest,
    HostCreateTransactionResponse, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse, HostSignPayloadRequest, HostSignPayloadResponse,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
    HostSignRawWithLegacyAccountRequest, LegacyAccountTxPayload, ProductAccountId,
    ProductAccountTxPayload, VrfSignature,
};
use truapi::{CallContext, CancellationReason};

use crate::host_internal::extrinsic::LocalTransactionError;
use crate::host_internal::sso_messages::RingVrfError;
use crate::host_internal::transaction::ExtrinsicPayloadError;
use crate::host_logic::raw_signing::RawPayloadError;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::statement_store::statement_public_key_from_secret;

/// Wallet call bound to a session and the origin that supplied its caller.
pub struct AccountInvocation<'a> {
    /// Cancellation and deadline for this operation.
    pub call: &'a CallContext,
    /// Session selected before the operation began.
    pub session: &'a AuthoritySession,
    /// This host's product binding or a paired host's reported identity.
    pub caller: AccountCaller<'a>,
}

impl AccountInvocation<'_> {
    /// Review wallet work, preserving the local product's trusted-review policy.
    pub async fn confirm(
        &self,
        platform: &dyn crate::platform::Platform,
        review: crate::platform::UserConfirmationReview,
    ) -> Result<(), AuthorityError> {
        use crate::platform::{
            CreateTransactionReview, SignPayloadReview, SignRawReview, UserConfirmationReview,
        };
        if let AccountCaller::Local { product, .. } = self.caller
            && crate::platform::has_trusted_remote_permissions(&product.product_id)
            && matches!(
                review,
                UserConfirmationReview::SignPayload(SignPayloadReview::Product { .. })
                    | UserConfirmationReview::SignRaw(SignRawReview::Product { .. })
                    | UserConfirmationReview::CreateTransaction(
                        CreateTransactionReview::Product { .. }
                    )
                    | UserConfirmationReview::StatementStoreProductSign(_)
            )
        {
            return Ok(());
        }
        let approved = super::until_cancelled(self.call, platform.confirm_user_action(review))
            .await?
            .map_err(AuthorityError::ConfirmationFailed)?;
        if approved {
            Ok(())
        } else {
            Err(AuthorityError::Rejected)
        }
    }
}

/// Trust boundary for product identity and host permissions.
#[derive(Clone, Copy)]
pub enum AccountCaller<'a> {
    /// Product identity bound by this host's runtime.
    Local {
        /// Product bound by the host runtime.
        product: &'a ProductContext,
        /// Wallet-issued permission retained by this host.
        authorization: Option<&'a WalletAuthorization>,
    },
    /// Product identity reported by an authenticated paired host.
    Remote {
        /// Some SSO operations carry no product identity.
        product_id: Option<&'a str>,
    },
}

impl AccountCaller<'_> {
    /// Product named by this invocation, when the transport supplied one.
    pub fn product_id(&self) -> Option<&str> {
        match self {
            Self::Local { product, .. } => Some(&product.product_id),
            Self::Remote { product_id } => *product_id,
        }
    }
}

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
        }
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

/// A product operation bound to its account session and host grants.
#[derive(Debug)]
pub struct HostOperation {
    /// Account activation selected before product approval.
    pub session: AuthoritySession,
    revision: u64,
}

impl HostOperation {
    /// Capture while holding the host's grant lifecycle lock.
    pub fn new(session: AuthoritySession, revision: u64) -> Self {
        Self { session, revision }
    }

    /// Stop native acquisition before resuming work invalidated by a host reset.
    pub async fn run<T, E, F>(&self, authority: &dyn ProductAuthority, call: F) -> Result<T, E>
    where
        F: core::future::Future<Output = Result<T, E>>,
        E: From<AuthorityError>,
    {
        futures::pin_mut!(call);
        futures::future::poll_fn(|context| {
            if let Err(error) = authority.require_current_operation(self) {
                return core::task::Poll::Ready(Err(error.into()));
            }
            match call.as_mut().poll(context) {
                core::task::Poll::Ready(Ok(value)) => core::task::Poll::Ready(
                    authority
                        .require_current_operation(self)
                        .map(|()| value)
                        .map_err(Into::into),
                ),
                outcome => outcome,
            }
        })
        .await
    }

    /// Reject work whose host grants were reset after it began.
    pub fn require_revision(&self, revision: u64) -> Result<(), AuthorityError> {
        if self.revision != revision {
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }
}

/// Typed account-authority failure before it is mapped to an API-specific error.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum AuthorityError {
    /// User or authority rejected the request.
    #[display("Rejected")]
    Rejected,
    /// The platform could not present or complete a wallet review.
    #[display("confirmation failed: {}", _0.reason)]
    ConfirmationFailed(#[error(not(source))] crate::latest::GenericError),
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
            error @ AuthorityError::ConfirmationFailed(_) => Self::Unknown { reason: error.to_string() },
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
        /// Product slot-zero account backing the validated legacy signer.
        product_account: ProductAccountId,
        /// Original legacy-account request.
        request: HostSignRawWithLegacyAccountRequest,
    },
    /// Sign with the active identity through the legacy-account API.
    IdentityAccount {
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

impl SignPayloadAuthorityRequest {
    /// Canonical review for this selected signing request.
    pub fn review(&self, caller: AccountCaller<'_>) -> crate::platform::UserConfirmationReview {
        use crate::platform::{SignPayloadReview, UserConfirmationReview};
        UserConfirmationReview::SignPayload(match self {
            Self::Product(request) => SignPayloadReview::Product {
                calling_product_id: caller.product_id().map(str::to_string),
                request: request.clone(),
            },
            Self::LegacyAccount { request, .. } => {
                SignPayloadReview::LegacyAccount(request.clone())
            }
        })
    }
}

impl SignRawAuthorityRequest {
    /// Canonical review, including the original legacy request and watermark.
    pub fn review(
        &self,
        caller: AccountCaller<'_>,
        watermarked: bool,
    ) -> crate::platform::UserConfirmationReview {
        use crate::platform::{SignRawReview, UserConfirmationReview};
        UserConfirmationReview::SignRaw(match self {
            Self::Product(request) => SignRawReview::Product {
                calling_product_id: caller.product_id().map(str::to_string),
                request: request.clone(),
                watermarked,
            },
            Self::LegacyAccount { request, .. } | Self::IdentityAccount { request, .. } => {
                SignRawReview::LegacyAccount {
                    request: request.clone(),
                    watermarked,
                }
            }
        })
    }
}

impl CreateTransactionAuthorityRequest {
    /// Canonical review for a transaction, including resolved contact data.
    pub fn review(&self, caller: AccountCaller<'_>) -> crate::platform::UserConfirmationReview {
        use crate::platform::{CreateTransactionReview, UserConfirmationReview};
        UserConfirmationReview::CreateTransaction(match self {
            Self::Product(payload) => CreateTransactionReview::Product {
                calling_product_id: caller.product_id().map(str::to_string),
                payload: payload.clone(),
            },
            Self::LegacyAccount { request, .. } | Self::IdentityAccount(request) => {
                CreateTransactionReview::LegacyAccount(request.clone())
            }
        })
    }
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

/// Wallet account operations bound to a selected session.
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
        invocation: AccountInvocation<'_>,
        request: HostAccountSignVrfRequest,
    ) -> Result<VrfSignature, AuthorityError>;

    /// Sign a SCALE transaction payload for a product account.
    ///
    /// The wallet validates local authorization or reviews the request; remote calls require review.
    async fn sign_payload(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Sign arbitrary bytes for a product account.
    ///
    /// Uses the same caller and consent boundary as [`AccountHolder::sign_payload`].
    async fn sign_raw(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<HostSignPayloadResponse, AuthorityError>;

    /// Build a transaction for a product account, signed unless the request
    /// supplies its own V5 `VerifyMultiSignature` extension.
    /// Uses the same caller and consent boundary as [`AccountHolder::sign_payload`].
    async fn create_transaction(
        &self,
        invocation: AccountInvocation<'_>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<HostCreateTransactionResponse, AuthorityError>;

    /// Derive a product-scoped contextual alias for an explicit registered key.
    ///
    /// The Account Holder resolves `key_handle` from the registry and derives
    /// the alias bound to `context`; `create_proof` derives the same alias.
    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, RingVrfError>;

    /// Create a ring-VRF proof bound to a context and message.
    ///
    /// Uses the request's explicit registered key, so the returned
    /// `contextual_alias` matches `account_alias` for the same inputs.
    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, RingVrfError>;

    /// Register a ring-VRF key owned by the calling product.
    async fn register_ring_vrf_key(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<HostAccountRegisterRingVrfKeyResponse, RingVrfError>;

    /// List registered ring-VRF keys.
    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<HostAccountListRingVrfKeysResponse, RingVrfError>;

    /// Sign bytes directly with a registered ring-VRF key.
    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<HostAccountRingVrfSignResponse, RingVrfError>;

    /// Sign exact statement-store proof bytes with a product-derived account.
    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
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

    /// Key material for minting contact handles.
    ///
    /// Uses the session's secret root entropy source so handles match across
    /// products and host roles. The key must remain inaccessible to products
    /// to prevent recovering contacts by hashing candidate accounts.
    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError>;
}

/// Account selection, resource acquisition and retained grants for product runtimes.
#[async_trait]
pub trait ProductAuthority: Send + Sync {
    /// Account holder selected by this host.
    fn account_holder(&self) -> &dyn AccountHolder;

    /// Capture account identity and host grants before product approval.
    fn current_operation(&self) -> Option<HostOperation>;

    /// Reject a product operation invalidated by account or host changes.
    fn require_current_operation(&self, operation: &HostOperation) -> Result<(), AuthorityError>;

    /// Acquire and retain product-scoped resources for this host.
    async fn allocate_resources(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product: &ProductContext,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, AuthorityError>;

    /// Seed the paired subtree cache for account-operation tests.
    #[cfg(test)]
    fn cache_product_subtree_for_test(
        &self,
        _session: &SessionInfo,
        _product_id: &str,
        _public_key: [u8; 32],
    ) {
    }

    /// Whether subtree resolution needs SSO and therefore host consent.
    ///
    /// True for a paired cache miss; false for local derivation or a cached subtree.
    async fn subtree_resolution_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool;

    /// Select retained wallet permission under the original host operation fence.
    fn wallet_authorization(
        &self,
        operation: &HostOperation,
        product: &ProductContext,
    ) -> Result<Option<WalletAuthorization>, AuthorityError>;

    /// Return statement-store allowance key material for the calling product.
    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError>;

    /// Forget the cached key only if it matches `public_key`, preserving any
    /// replacement. Hosts without a local cache use the no-op default.
    fn forget_statement_store_allowance_key(&self, _product_id: &str, _public_key: [u8; 32]) {}

    /// Return Bulletin allowance key material for the calling product.
    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;

    /// Invalidate the cached Bulletin key and increase or recreate its allowance
    /// after a submission is rejected for an exhausted or missing allowance.
    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError>;
}

/// Build the neutral authority-session snapshot for `session`.
pub fn authority_session(session: &SessionInfo) -> AuthoritySession {
    AuthoritySession::from_session_info(session, authority_session_validation_id(session))
}

/// Revalidate a pre-confirmation snapshot against the live session, returning
/// the current [`SessionInfo`] when it still matches.
///
/// Both roles use this before touching key material: a snapshot taken before
/// user confirmation must still be the current authority session when the
/// signature or derivation happens, otherwise the request is rejected.
pub fn require_current_session(
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
