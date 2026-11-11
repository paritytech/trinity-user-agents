//! The account operations one product connection may call, bound to its product.

use core::time::Duration;
use std::sync::Arc;

use truapi::{
	CallContext, CallError,
	latest::{
		HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasRequest,
		HostAccountGetAliasResponse, HostAccountGetError, HostAccountListRingVrfKeysRequest,
		HostAccountListRingVrfKeysResponse, HostAccountRegisterRingVrfKeyRequest,
		HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignRequest,
		HostAccountRingVrfSignResponse, HostAccountSignVrfRequest, HostCreateTransactionResponse,
		HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
		HostSignPayloadResponse, ProductAccountId, VrfSignature,
	},
};

#[cfg(test)]
use crate::host_logic::session::SessionInfo;
use crate::{host_internal::sso_messages::RingVrfError, platform::ProductId};

use super::{
	HostAccounts, ProductConnection,
	authority::{
		AccountHolder, AuthorityError, AuthoritySession, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
		StatementStoreAllowanceKey,
	},
	contacts::ContactHandles,
	product_consent::ProductConsent,
};

/// The host's accounts as one product connection sees them: every call acts
/// for that connection's product, and host-level operations stay out of reach.
pub struct ProductAccounts<H: AccountHolder> {
	accounts: Arc<HostAccounts<H>>,
	connection: Arc<ProductConnection>,
}

impl<H: AccountHolder> ProductAccounts<H> {
	/// Bind the host's accounts to one product connection.
	pub fn new(accounts: Arc<HostAccounts<H>>, connection: Arc<ProductConnection>) -> Self {
		Self { accounts, connection }
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
		product_id: &ProductId,
		public_key: [u8; 32],
	) {
		self.accounts.cache_product_subtree_for_test(session, product_id, public_key);
	}

	/// The host's account-access decisions and review prompts.
	pub fn consent(&self) -> &ProductConsent {
		self.accounts.consent()
	}

	/// Public key of `account`, after any account-access check it needs.
	pub async fn get_account(
		&self,
		cx: &CallContext,
		account: ProductAccountId,
	) -> Result<[u8; 32], CallError<HostAccountGetError>> {
		self.accounts.get_account(cx, &self.connection, account).await
	}

	/// Public key of a product account this product may already use.
	pub async fn product_account_public_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		account: &ProductAccountId,
	) -> Result<[u8; 32], AuthorityError> {
		self.accounts
			.product_account_public_key(cx, session, &self.connection.product, account)
			.await
	}

	/// `product_id`'s hard-subtree public key for host administration, or
	/// `None` while signed out. A resolution still running after `timeout` is dropped.
	pub async fn product_subtree_public_key_within(
		&self,
		product_id: &ProductId,
		timeout: Duration,
	) -> Result<Option<[u8; 32]>, AuthorityError> {
		self.accounts.product_subtree_public_key_within(product_id, timeout).await
	}

	/// Sign an RFC-0023 Merlin transcript with a product account.
	pub async fn sign_vrf(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountSignVrfRequest,
	) -> Result<VrfSignature, AuthorityError> {
		self.accounts.sign_vrf(session, cx, &self.connection, request).await
	}

	/// Sign a SCALE transaction payload for a product account.
	pub async fn sign_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: SignPayloadAuthorityRequest,
	) -> Result<HostSignPayloadResponse, AuthorityError> {
		self.accounts.sign_payload(session, cx, &self.connection, request).await
	}

	/// Sign arbitrary bytes for a product account.
	pub async fn sign_raw(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<HostSignPayloadResponse, AuthorityError> {
		self.accounts
			.sign_raw(session, cx, &self.connection, request, watermarked)
			.await
	}

	/// Build a transaction for a product account.
	pub async fn create_transaction(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: CreateTransactionAuthorityRequest,
	) -> Result<HostCreateTransactionResponse, AuthorityError> {
		self.accounts.create_transaction(session, cx, &self.connection, request).await
	}

	/// Derive a contextual alias for an explicit registered ring-VRF key.
	pub async fn account_alias(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountGetAliasRequest,
	) -> Result<HostAccountGetAliasResponse, RingVrfError> {
		self.accounts
			.account_alias(session, cx, &self.connection.product, request)
			.await
	}

	/// Create a ring-VRF proof bound to a context and message.
	pub async fn create_proof(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountCreateProofRequest,
	) -> Result<HostAccountCreateProofResponse, RingVrfError> {
		self.accounts.create_proof(session, cx, &self.connection.product, request).await
	}

	/// Register a ring-VRF key owned by this product.
	pub async fn register_ring_vrf_key(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<HostAccountRegisterRingVrfKeyResponse, RingVrfError> {
		self.accounts
			.register_ring_vrf_key(session, cx, &self.connection.product, request)
			.await
	}

	/// List registered ring-VRF keys.
	pub async fn list_ring_vrf_keys(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountListRingVrfKeysRequest,
	) -> Result<HostAccountListRingVrfKeysResponse, RingVrfError> {
		self.accounts
			.list_ring_vrf_keys(session, cx, &self.connection.product, request)
			.await
	}

	/// Sign bytes directly with a registered ring-VRF key.
	pub async fn ring_vrf_sign(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountRingVrfSignRequest,
	) -> Result<HostAccountRingVrfSignResponse, RingVrfError> {
		self.accounts
			.ring_vrf_sign(session, cx, &self.connection.product, request)
			.await
	}

	/// Confirm and allocate resources for this product.
	pub async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: HostRequestResourceAllocationRequest,
	) -> Result<HostRequestResourceAllocationResponse, AuthorityError> {
		self.accounts.allocate_resources(cx, session, &self.connection, request).await
	}

	/// Statement-store allowance key material for this product.
	pub async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		self.accounts
			.statement_store_allowance_key(cx, session, &self.connection.product)
			.await
	}

	/// Forget this product's kept Statement Store key if it is still `public_key`.
	pub fn forget_statement_store_allowance_key(&self, public_key: [u8; 32]) {
		self.accounts
			.forget_statement_store_allowance_key(&self.connection.product.product_id, public_key);
	}

	/// Whether preimage submissions stay in the core instead of reaching Bulletin.
	#[cfg(feature = "test-host")]
	pub fn submits_preimages_locally(&self) -> bool {
		self.accounts.submits_preimages_locally()
	}

	/// Bulletin allowance key material for this product.
	pub async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.accounts
			.bulletin_allowance_key(cx, session, &self.connection.product)
			.await
	}

	/// Replace this product's Bulletin key after a submission was refused.
	pub async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.accounts
			.refresh_bulletin_allowance_key(cx, session, &self.connection.product)
			.await
	}

	/// Sign exact statement-store proof bytes with a product account.
	pub async fn sign_statement_store_product_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		account: ProductAccountId,
		payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		self.accounts
			.sign_statement_store_product_payload(session, cx, &self.connection, account, payload)
			.await
	}

	/// Derive entropy scoped to this product.
	pub fn derive_entropy(
		&self,
		session: &AuthoritySession,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.accounts
			.derive_entropy(session, &self.connection.product.product_id, context)
	}

	/// Contact handles that stay stable across products.
	pub fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<ContactHandles, AuthorityError> {
		self.accounts.contact_handles(session)
	}
}
