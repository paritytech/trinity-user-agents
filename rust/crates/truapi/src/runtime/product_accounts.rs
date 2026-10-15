//! The account operations one product connection may call, bound to its product.

use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use truapi::{CallContext, CallError, latest as api};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(test)]
use crate::host_logic::session::SessionInfo;
use crate::{
	host_internal::{permissions::TemporaryPermissions, sso_messages::RingVrfError},
	platform::ProductContext,
};

use super::{
	HostAccounts,
	authority::{
		AccountHolder, AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
		SignPayloadAuthorityRequest, SignRawAuthorityRequest,
	},
	bulletin_rpc::BulletinSubmitError,
	contacts::ContactHandles,
	statement_store::StatementProofFailure,
};

/// The host's accounts as one product connection sees them: every call acts for
/// that product, and host-level operations stay out of reach.
pub struct ProductAccounts<H: AccountHolder> {
	accounts: Arc<HostAccounts<H>>,
	product: ProductContext,
}

impl<H: AccountHolder> ProductAccounts<H> {
	/// Bind the host's accounts to the product of one connection.
	pub fn new(accounts: Arc<HostAccounts<H>>, product: ProductContext) -> Self {
		Self { accounts, product }
	}

	/// The shared accounts behind this view, for tests that rebuild a connection.
	#[cfg(test)]
	pub fn host_accounts_for_tests(&self) -> &Arc<HostAccounts<H>> {
		&self.accounts
	}

	/// Current account-authority session, if connected.
	pub fn current_session(&self) -> Option<AuthoritySession> {
		self.accounts.current_session()
	}

	/// Seed a product subtree for authority tests.
	#[cfg(test)]
	pub fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &str,
		public_key: [u8; 32],
	) {
		self.accounts.cache_product_subtree_for_test(session, product_id, public_key);
	}

	/// The owner this product may act for when it names `dot_ns_identifier`,
	/// or `None` when it may not.
	pub async fn authorized_product_account(
		&self,
		dot_ns_identifier: &str,
		cx: &CallContext,
	) -> Option<String> {
		self.accounts
			.authorized_product_account(&self.product, dot_ns_identifier, cx)
			.await
	}

	/// Public key of `account`, after any account-access check it needs.
	pub async fn get_account(
		&self,
		cx: &CallContext,
		account: api::ProductAccountId,
	) -> Result<[u8; 32], CallError<api::HostAccountGetError>> {
		self.accounts.get_account(cx, &self.product, account).await
	}

	/// Public key of a product account this product may already use.
	pub async fn product_account_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		account: &api::ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		self.accounts
			.product_account_public_key(cx, session, &self.product, account)
			.await
	}

	/// Sign an RFC-0023 Merlin transcript with a product account.
	pub async fn sign_vrf(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountSignVrfRequest,
	) -> Result<api::VrfSignature, AuthorityError> {
		self.accounts.sign_vrf(session, cx, &self.product, request).await
	}

	/// Sign a SCALE transaction payload for a product account.
	pub async fn sign_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		approvals: &TemporaryPermissions,
		request: SignPayloadAuthorityRequest,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		self.accounts.sign_payload(session, cx, &self.product, approvals, request).await
	}

	/// Sign arbitrary bytes for a product account.
	pub async fn sign_raw(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		approvals: &TemporaryPermissions,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<api::HostSignPayloadResponse, AuthorityError> {
		self.accounts
			.sign_raw(session, cx, &self.product, approvals, request, watermarked)
			.await
	}

	/// Build a transaction for a product account.
	pub async fn create_transaction(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		approvals: &TemporaryPermissions,
		request: CreateTransactionAuthorityRequest,
	) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
		self.accounts
			.create_transaction(session, cx, &self.product, approvals, request)
			.await
	}

	/// Derive a contextual alias for an explicit registered ring-VRF key.
	pub async fn account_alias(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountGetAliasRequest,
	) -> Result<api::ContextualAlias, RingVrfError> {
		self.accounts.account_alias(session, cx, &self.product, request).await
	}

	/// Create a ring-VRF proof bound to a context and message.
	pub async fn create_proof(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountCreateProofRequest,
	) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
		self.accounts.create_proof(session, cx, &self.product, request).await
	}

	/// Register a ring-VRF key owned by this product.
	pub async fn register_ring_vrf_key(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountRegisterRingVrfKeyRequest,
	) -> Result<[u8; 32], RingVrfError> {
		self.accounts.register_ring_vrf_key(session, cx, &self.product, request).await
	}

	/// List registered ring-VRF keys.
	pub async fn list_ring_vrf_keys(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountListRingVrfKeysRequest,
	) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
		self.accounts.list_ring_vrf_keys(session, cx, &self.product, request).await
	}

	/// Sign bytes directly with a registered ring-VRF key.
	pub async fn ring_vrf_sign(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: api::HostAccountRingVrfSignRequest,
	) -> Result<Vec<u8>, RingVrfError> {
		self.accounts.ring_vrf_sign(session, cx, &self.product, request).await
	}

	/// Allocate resources for this product.
	pub async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: api::HostRequestResourceAllocationRequest,
	) -> Result<api::HostRequestResourceAllocationResponse, AuthorityError> {
		self.accounts.allocate_resources(cx, session, &self.product, request).await
	}

	/// Renew this product's retained statement sponsorship for `signer`.
	pub async fn renew_statement_sponsorship(
		&self,
		cx: &CallContext,
		signer: [u8; 32],
	) -> Result<(), AuthorityError> {
		self.accounts.renew_statement_sponsorship(cx, &self.product, signer).await
	}

	/// Whether preimage submissions stay in the core instead of reaching Bulletin.
	#[cfg(feature = "test-host")]
	pub fn submits_preimages_locally(&self) -> bool {
		self.accounts.submits_preimages_locally()
	}

	/// Store `value` on Bulletin under this product's allowance before `deadline`.
	pub async fn submit_preimage(
		&self,
		cx: &CallContext,
		deadline: Instant,
		session: &AuthoritySession,
		value: &[u8],
	) -> Result<Vec<u8>, BulletinSubmitError> {
		self.accounts
			.submit_preimage(cx, deadline, session, self.product.product_id.clone(), value)
			.await
	}

	/// Sign exact statement-store proof bytes with a product account.
	pub async fn sign_statement_store_product_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		approvals: &TemporaryPermissions,
		account: api::ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		self.accounts
			.sign_statement_store_product_payload(
				session,
				cx,
				&self.product,
				approvals,
				account,
				payload,
			)
			.await
	}

	/// Prove `statement` with this product's Statement Store allowance.
	pub async fn create_authorized_statement_proof(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		statement: api::Statement,
	) -> Result<api::StatementProof, StatementProofFailure> {
		self.accounts
			.create_authorized_statement_proof(
				cx,
				session,
				self.product.product_id.clone(),
				statement,
			)
			.await
	}

	/// Derive entropy scoped to this product.
	pub fn derive_entropy(
		&self,
		session: &AuthoritySession,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.accounts.derive_entropy(session, &self.product.product_id, context)
	}

	/// Contact handles that stay stable across products.
	pub fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<ContactHandles, AuthorityError> {
		self.accounts.contact_handles(session)
	}
}
