//! Account authority contracts shared by product runtimes.
//!
//! Local and paired hosts use the same request and session types.

use super::{allowances::AllowanceResource, product_consent::ProductConsent};
use crate::platform::{ProductContext, ProductId};
use truapi::{
	CallContext, CancellationReason,
	latest::{
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
	},
};

use crate::{
	host_internal::{
		extrinsic::LocalTransactionError,
		sso_messages::{OnExistingAllowancePolicy, ProductRequest, RingVrfError},
		transaction::ExtrinsicPayloadError,
	},
	host_logic::{
		product_account::ProductAccountError,
		raw_signing::RawPayloadError,
		session::{SessionInfo, SessionState},
		statement_store::statement_public_key_from_secret,
	},
};

use super::sso_remote::{
	SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SsoRemoteResponseError,
};

/// Wallet call bound to a session and the origin that supplied its caller.
pub struct AccountInvocation<'a> {
	/// Cancellation and deadline for this operation.
	pub call: &'a CallContext,
	/// Session selected before the operation began.
	pub session: &'a AuthoritySession,
	/// This host's product binding or a paired host's reported identity.
	pub caller: AccountCaller<'a>,
}

/// Trust boundary for product identity and host permissions.
#[derive(Clone)]
pub enum AccountCaller<'a> {
	/// Product identity bound by this host's runtime.
	Local {
		/// Product bound by the host runtime.
		product: &'a ProductContext,
	},
	/// Product identity reported by an authenticated paired host.
	Remote {
		/// Some SSO operations carry no product identity.
		product_id: Option<&'a ProductId>,
	},
}

impl AccountCaller<'_> {
	/// Product named by this invocation, when the transport supplied one.
	pub fn product_id(&self) -> Option<&ProductId> {
		match self {
			Self::Local { product } => Some(&product.product_id),
			Self::Remote { product_id } => *product_id,
		}
	}
}

/// Validated Bulletin signing material, redacted in diagnostics and zeroized on drop.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop, derive_more::Debug)]
pub struct BulletinAllowanceKey {
	#[debug("\"<redacted>\"")]
	secret: [u8; 64],
}

impl BulletinAllowanceKey {
	/// Reject invalid sr25519 material before it can be retained.
	pub fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
		let secret: [u8; 64] =
			secret.try_into().map_err(|secret: Vec<u8>| AuthorityError::Unavailable {
				reason: format!("bulletin allowance key must be 64 bytes, got {}", secret.len()),
			})?;
		schnorrkel::SecretKey::from_bytes(&secret).map_err(|_| AuthorityError::Unavailable {
			reason: "invalid bulletin allowance key".to_string(),
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
	/// Keep both delegated secrets together for the same product subtree.
	pub fn from_parts(secret: [u8; 64], ring_vrf_domain_entropy: [u8; 32]) -> Self {
		Self { secret, ring_vrf_domain_entropy }
	}

	/// Retained subtree secret for delegated account signing.
	pub fn as_secret_bytes(&self) -> &[u8; 64] {
		&self.secret
	}

	/// Product-scoped entropy for delegated ring signing.
	pub fn ring_vrf_domain_entropy(&self) -> &[u8; 32] {
		&self.ring_vrf_domain_entropy
	}
}
/// Snapshot of an account-authority session selected by the authority.
///
/// This is the neutral session projection product runtimes can use while
/// preserving authority-private material inside the concrete authority
/// implementation.
#[derive(Clone)]
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
			.or_else(|| self.lite_username.as_deref().filter(|value| !value.is_empty()))
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
	/// The chain stayed unreachable, such as during a halt, so nothing was
	/// signed. A later call can succeed.
	#[display("{reason}")]
	ChainUnavailable { reason: String },
	/// The authority cannot service this request shape (e.g. an unsupported
	/// transaction-extension version).
	#[display("{reason}")]
	NotSupported { reason: String },
	/// Catch-all authority failure.
	#[display("{reason}")]
	Unknown { reason: String },
}

/// Starts the reason a signing host answers with when it could not reach the
/// chain, since an SSO answer carries only a string.
const SSO_CHAIN_UNAVAILABLE_PREFIX: &str = "ChainUnavailable: ";

impl AuthorityError {
	/// The reason a signing host answers a failed SSO request with. A chain it
	/// could not reach is marked, so the pairing host reports it as
	/// `ChainUnavailable` rather than `Unknown`.
	pub fn into_sso_reason(self) -> String {
		match self {
			Self::ChainUnavailable { reason } => format!("{SSO_CHAIN_UNAVAILABLE_PREFIX}{reason}"),
			failure => failure.to_string(),
		}
	}
}

impl From<ProductAccountError> for AuthorityError {
	fn from(err: ProductAccountError) -> Self {
		Self::Unavailable { reason: err.to_string() }
	}
}

impl From<SsoRemoteResponseError> for AuthorityError {
	fn from(reason: SsoRemoteResponseError) -> Self {
		match reason {
			SsoRemoteResponseError::Cancelled(err) => {
				Self::Cancelled(AuthorityCancelError::new(err.remote_message_id(), err.reason()))
			},
			SsoRemoteResponseError::LocalDisconnected |
			SsoRemoteResponseError::PeerDisconnected => Self::Disconnected,
			SsoRemoteResponseError::ChainUnavailable(reason) => Self::ChainUnavailable { reason },
			SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
				"Rejected" | "User rejected" => Self::Rejected,
				SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => Self::Disconnected,
				other => match other.strip_prefix(SSO_CHAIN_UNAVAILABLE_PREFIX) {
					Some(cause) => Self::ChainUnavailable { reason: cause.to_string() },
					None => Self::Unknown { reason },
				},
			},
		}
	}
}

impl From<SsoRemoteResponseError> for RingVrfError {
	fn from(reason: SsoRemoteResponseError) -> Self {
		AuthorityError::from(reason).into()
	}
}

/// A preimage this host cannot assemble is a shape failure, not a transport one.
impl From<ExtrinsicPayloadError> for AuthorityError {
	fn from(err: ExtrinsicPayloadError) -> Self {
		match err {
			ExtrinsicPayloadError::UnsupportedPayloadVersion { .. } |
			ExtrinsicPayloadError::UnsupportedSignedExtension { .. } => {
				Self::NotSupported { reason: err.to_string() }
			},
			other => Self::Unknown { reason: other.to_string() },
		}
	}
}

/// A chain the host cannot connect to is `Unavailable`, and a connected node
/// that cannot be asked is `ChainUnavailable`; a request shape it cannot
/// assemble is `NotSupported`.
impl From<LocalTransactionError> for AuthorityError {
	fn from(err: LocalTransactionError) -> Self {
		match err {
			LocalTransactionError::UnsupportedTxExtVersion { .. } |
			LocalTransactionError::UnsupportedExtensions(_) => {
				Self::NotSupported { reason: err.to_string() }
			},
			LocalTransactionError::NoChainConnection(_) => {
				Self::Unavailable { reason: err.to_string() }
			},
			LocalTransactionError::ChainUnavailable(_) => {
				Self::ChainUnavailable { reason: err.to_string() }
			},
			LocalTransactionError::Other(_) => Self::Unknown { reason: err.to_string() },
		}
	}
}

impl From<RawPayloadError> for AuthorityError {
	fn from(err: RawPayloadError) -> Self {
		Self::Unknown { reason: err.to_string() }
	}
}

impl From<crate::stack::UnservedProduct> for AuthorityError {
	fn from(error: crate::stack::UnservedProduct) -> Self {
		Self::NotSupported { reason: error.to_string() }
	}
}

impl From<AuthorityError> for RingVrfError {
	fn from(err: AuthorityError) -> Self {
		match err {
			AuthorityError::Rejected => RingVrfError::Rejected,
			other => RingVrfError::Unknown { reason: other.to_string() },
		}
	}
}

impl From<AuthorityError> for HostAccountSignVrfError {
	fn from(err: AuthorityError) -> Self {
		match err {
			AuthorityError::Disconnected => Self::NotConnected,
			AuthorityError::Rejected => Self::Rejected,
			AuthorityError::ConfirmationFailed(error) => {
				Self::Unknown { reason: format!("VRF signing confirmation failed: {error:?}") }
			},
			AuthorityError::Cancelled(err) => Self::Unknown { reason: err.to_string() },
			AuthorityError::Unavailable { reason } |
			AuthorityError::ChainUnavailable { reason } |
			AuthorityError::NotSupported { reason } |
			AuthorityError::Unknown { reason } => Self::Unknown { reason },
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
		Self { request_id: request_id.to_string(), reason }
	}
}

/// Payload-signing request selected by the product API entrypoint.
#[derive(Clone)]
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
#[derive(Clone)]
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
#[derive(Clone)]
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
	pub fn review(&self, caller: &AccountCaller<'_>) -> crate::platform::UserConfirmationReview {
		use crate::platform::{SignPayloadReview, UserConfirmationReview};
		UserConfirmationReview::SignPayload(match self {
			Self::Product(request) => SignPayloadReview::Product {
				calling_product_id: caller.product_id().cloned(),
				request: request.clone(),
			},
			Self::LegacyAccount { product_account, request } => SignPayloadReview::LegacyAccount {
				account: product_account.clone(),
				request: request.clone(),
			},
		})
	}
}

impl SignRawAuthorityRequest {
	/// Canonical review, including the original legacy request and watermark.
	pub fn review(
		&self,
		caller: &AccountCaller<'_>,
		watermarked: bool,
	) -> crate::platform::UserConfirmationReview {
		use crate::platform::{LegacySigningAccount, SignRawReview, UserConfirmationReview};
		UserConfirmationReview::SignRaw(match self {
			Self::Product(request) => SignRawReview::Product {
				calling_product_id: caller.product_id().cloned(),
				request: request.clone(),
				watermarked,
			},
			Self::LegacyAccount { request, .. } => SignRawReview::LegacyAccount {
				account: LegacySigningAccount::Identity,
				request: request.clone(),
				watermarked,
			},
		})
	}
}

impl CreateTransactionAuthorityRequest {
	/// Canonical review for a transaction, including resolved contact data.
	pub fn review(&self, caller: &AccountCaller<'_>) -> crate::platform::UserConfirmationReview {
		use crate::platform::{
			CreateTransactionReview, LegacySigningAccount, UserConfirmationReview,
		};
		UserConfirmationReview::CreateTransaction(match self {
			Self::Product(payload) => CreateTransactionReview::Product {
				calling_product_id: caller.product_id().cloned(),
				payload: payload.clone(),
			},
			Self::LegacyAccount { product_account, request } => {
				CreateTransactionReview::LegacyAccount {
					account: LegacySigningAccount::Product(product_account.clone()),
					payload: request.clone(),
				}
			},
			Self::IdentityAccount(payload) => CreateTransactionReview::LegacyAccount {
				account: LegacySigningAccount::Identity,
				payload: payload.clone(),
			},
		})
	}
}

/// Whether blessed `calling_product_id` is using its own account, `owner`.
pub fn is_blessed_owner(calling_product_id: &ProductId, owner: &ProductId) -> bool {
	calling_product_id.has_trusted_remote_permissions() && owner == calling_product_id
}

/// Whether this product account can sign without confirmation.
#[derive(Clone, PartialEq, Eq)]
pub enum AutoSigningGrant {
	/// The authority holds the signing keys and needs no prompt.
	Active,
	/// User consent is required.
	Absent,
}

/// Statement-store allowance signing material held by the authority layer.
#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct StatementStoreAllowanceKey {
	secret: [u8; 64],
	/// Public key derived from the secret.
	pub public_key: [u8; 32],
}

impl StatementStoreAllowanceKey {
	/// Wrap a 64-byte sr25519 secret and derive its public key; other lengths
	/// are `Unavailable`.
	pub fn from_secret_bytes(secret: Vec<u8>) -> Result<Self, AuthorityError> {
		let secret: [u8; 64] =
			secret.try_into().map_err(|secret: Vec<u8>| AuthorityError::Unavailable {
				reason: format!(
					"statement-store allowance key must be 64 bytes, got {}",
					secret.len()
				),
			})?;
		let public_key = statement_public_key_from_secret(secret)
			.map_err(|reason| AuthorityError::Unavailable { reason })?;
		Ok(Self { secret, public_key })
	}

	/// sr25519 secret used to sign allowance statements.
	pub fn as_secret_bytes(&self) -> &[u8; 64] {
		&self.secret
	}
}

/// Allowance key that gives a product access to one chain resource.
pub enum Allowance {
	/// Statement signing material and a known on-chain allocation period, if supplied.
	StatementStore {
		/// Validated statement signing key.
		key: StatementStoreAllowanceKey,
		/// Actual period returned by wallet issuance; SSO does not provide one.
		period: Option<u32>,
	},
	/// Dedicated Bulletin allowance key.
	Bulletin(BulletinAllowanceKey),
}

impl Allowance {
	/// Chain resource this key grants access to.
	pub fn resource(&self) -> AllowanceResource {
		match self {
			Self::StatementStore { .. } => AllowanceResource::StatementStore,
			Self::Bulletin(_) => AllowanceResource::Bulletin,
		}
	}

	/// sr25519 secret of the allowance account.
	pub fn as_secret_bytes(&self) -> &[u8; 64] {
		match self {
			Self::StatementStore { key, .. } => key.as_secret_bytes(),
			Self::Bulletin(key) => key.as_secret_bytes(),
		}
	}
}

/// Issued capability material, ready for the host to validate and retain.
pub enum AccountGrant {
	/// Allowance key for one chain resource.
	Allowance(Allowance),
	/// Funded product account.
	SmartContract,
	/// Exported product signing material.
	AutoSigning(AutoSigningKey),
}

/// One resource result in an otherwise valid allocation batch.
pub enum AccountGrantOutcome {
	/// Issued capability material.
	Allocated(AccountGrant),
	/// The holder declined this resource.
	Rejected,
	/// No grant is available; local issuers may supply diagnostic context.
	NotAvailable {
		/// Issuer detail absent from the existing SSO response.
		reason: Option<String>,
	},
}

/// How a host keeps the grants its account holder issues.
#[derive(PartialEq)]
pub enum GrantKeeping {
	/// The wallet's own host: grants last one wallet activation.
	Wallet,
	/// A host paired with a wallet on another device: grants persist under the pairing.
	Paired,
}

/// Wallet account operations bound to a selected session.
#[async_trait::async_trait]
pub trait AccountHolder: Send + Sync + 'static {
	/// How the host keeps the grants this holder issues.
	const GRANT_KEEPING: GrantKeeping;

	/// Current account-authority session, if connected.
	fn current_session(&self) -> Option<AuthoritySession>;

	/// Reject replacement of the originally selected wallet or paired channel.
	fn require_current_session(&self, session: &AuthoritySession) -> Result<(), AuthorityError>;

	/// Lazily issue the ordered grants of an explicit request the caller confirmed.
	async fn allocate_grants<'a>(
		&'a self,
		invocation: AccountInvocation<'a>,
		request: HostRequestResourceAllocationRequest,
		policy: OnExistingAllowancePolicy,
	) -> Result<
		futures::stream::BoxStream<'a, Result<AccountGrantOutcome, AuthorityError>>,
		AuthorityError,
	>;

	/// Obtain one allowance when the host has no usable grant.
	async fn ensure_allowance(
		&self,
		invocation: AccountInvocation<'_>,
		resource: AllowanceResource,
		policy: OnExistingAllowancePolicy,
	) -> Result<AccountGrant, AuthorityError>;

	/// Resolve the public hard subtree under the selected account holder.
	async fn product_subtree_public_key(
		&self,
		invocation: AccountInvocation<'_>,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError>;

	/// Sign an RFC-0023 Merlin transcript with a product account.
	async fn sign_vrf(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountSignVrfRequest,
	) -> Result<VrfSignature, AuthorityError>;

	/// Sign a SCALE transaction payload for a product account.
	///
	/// The caller confirms the request with the user first.
	async fn sign_payload(
		&self,
		invocation: AccountInvocation<'_>,
		request: SignPayloadAuthorityRequest,
	) -> Result<HostSignPayloadResponse, AuthorityError>;

	/// Sign arbitrary bytes for a product account.
	///
	/// Confirmed by the caller like [`AccountHolder::sign_payload`].
	async fn sign_raw(
		&self,
		invocation: AccountInvocation<'_>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<HostSignPayloadResponse, AuthorityError>;

	/// Build a transaction for a product account, signed unless the request
	/// supplies its own V5 `VerifyMultiSignature` extension.
	/// Confirmed by the caller like [`AccountHolder::sign_payload`].
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

	/// Register a ring-VRF key owned by the calling product and record it in
	/// the registry this host reads.
	async fn register_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<HostAccountRegisterRingVrfKeyResponse, RingVrfError>;

	/// Record a key the host derived from the calling product's kept
	/// AutoSigning key, so the holder stays the registry's only writer.
	async fn record_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: HostAccountRegisterRingVrfKeyRequest,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError>;

	/// List an owner's registered ring-VRF keys, keeping the registry this host reads current.
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
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError>;

	/// Mint and resolve contact handles under the session's root entropy source.
	///
	/// Handles match across products and host roles. The key stays inside
	/// `ContactHandles`, so no product can recover contacts by hashing
	/// candidate accounts.
	fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<super::contacts::ContactHandles, AuthorityError>;
}

/// Host account authority used by product runtimes.
#[async_trait::async_trait]
pub trait ProductAuthority: Send + Sync {
	/// Current account-authority session, if connected.
	fn current_session(&self) -> Option<AuthoritySession>;

	/// This runtime's account-access decisions and operation reviews.
	fn consent(&self) -> &ProductConsent;

	/// Seed a product subtree for authority tests.
	#[cfg(test)]
	fn cache_product_subtree_for_test(
		&self,
		_session: &SessionInfo,
		_product_id: &ProductId,
		_public_key: [u8; 32],
	) {
	}

	/// Resolve the public key for a product's hard subtree.
	async fn product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError>;

	/// Whether subtree resolution needs SSO and host consent.
	async fn subtree_resolution_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> bool;

	/// Whether this product account can sign without confirmation.
	async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		account: &ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError>;

	/// Sign an RFC-0023 Merlin transcript with a product account.
	async fn sign_vrf(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: &ProductId,
		request: HostAccountSignVrfRequest,
	) -> Result<VrfSignature, AuthorityError>;

	/// Sign a SCALE transaction payload for a product account.
	///
	/// Bind the caller to the account before local signing; `None` denotes an SSO relay.
	async fn sign_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignPayloadAuthorityRequest,
	) -> Result<HostSignPayloadResponse, AuthorityError>;

	/// Sign arbitrary bytes for a product account.
	///
	/// Uses the same caller binding as [`ProductAuthority::sign_payload`].
	async fn sign_raw(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<HostSignPayloadResponse, AuthorityError>;

	/// Build a transaction for a product account, signed unless the request
	/// supplies its own V5 `VerifyMultiSignature` extension.
	/// Uses the same caller binding as [`ProductAuthority::sign_payload`].
	async fn create_transaction(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
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

	/// Revision of this host's kept grants.
	///
	/// Read before a review, it lets a product reset during the review discard
	/// the grants it confirmed.
	fn grant_revision(&self) -> u64;

	/// Ask the account authority to allocate product-scoped resources.
	///
	/// The grants are kept only while this host's grants are still at
	/// `grant_revision`.
	async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		grant_revision: u64,
		product_id: &ProductId,
		request: HostRequestResourceAllocationRequest,
	) -> Result<HostRequestResourceAllocationResponse, AuthorityError>;

	/// Return statement-store allowance key material for the calling product.
	async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError>;

	/// Forget the cached key only if it matches `public_key`, preserving any
	/// replacement. Hosts without a local cache use the no-op default.
	fn forget_statement_store_allowance_key(&self, _product_id: &ProductId, _public_key: [u8; 32]) {
	}

	/// Whether a preimage submission is kept in the core instead of being sent
	/// to the Bulletin chain.
	///
	/// True only for a test host that answers allowances as granted without
	/// performing them: the Bulletin allowance it hands out was never
	/// authorized on chain, so a real `store` would always be refused.
	#[cfg(feature = "test-host")]
	fn submits_preimages_locally(&self) -> bool {
		false
	}

	/// Return Bulletin allowance key material for the calling product.
	async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError>;

	/// Invalidate the cached Bulletin key and increase or recreate its allowance
	/// after a submission is rejected for an exhausted or missing allowance.
	async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError>;

	/// Sign exact statement-store proof bytes with a product-derived account.
	async fn sign_statement_store_product_payload(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		calling_product_id: Option<&ProductId>,
		account: ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError>;

	/// Derive product-scoped entropy for a connected session.
	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError>;

	/// Core-only root-derived key for stable contact handles across products.
	fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError>;
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
	let current = session_state.current().ok_or(AuthorityError::Disconnected)?;
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

#[cfg(test)]
mod tests {
	use super::*;

	/// An SSO answer is only a string, so a chain the signing host could not
	/// reach must survive the trip to reach the product as `ChainUnavailable`,
	/// while every other failure reads back as it was sent.
	#[test]
	fn a_signing_host_failure_keeps_its_kind_through_the_sso_answer() {
		let failures = [
			AuthorityError::ChainUnavailable {
				reason: "cannot select a metadata block".to_string(),
			},
			AuthorityError::Rejected,
			AuthorityError::Unknown { reason: "the payload did not decode".to_string() },
		];

		let relayed: Vec<AuthorityError> = failures
			.iter()
			.cloned()
			.map(|failure| SsoRemoteResponseError::Failure(failure.into_sso_reason()).into())
			.collect();

		assert_eq!(relayed, failures);
	}
}
