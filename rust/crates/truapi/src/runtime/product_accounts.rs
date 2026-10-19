//! The account operations one product connection may call, bound to its product.

use std::sync::Arc;

use truapi::{
	CallContext,
	latest::{
		HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasRequest,
		HostAccountGetAliasResponse, HostAccountListRingVrfKeysRequest,
		HostAccountListRingVrfKeysResponse, HostAccountRegisterRingVrfKeyRequest,
		HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignRequest,
		HostAccountRingVrfSignResponse, HostAccountSignVrfRequest, HostCreateTransactionResponse,
		HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
		HostSignPayloadResponse, ProductAccountId, VrfSignature,
	},
};

#[cfg(test)]
use crate::host_logic::session::SessionInfo;
use crate::{
	host_internal::sso_messages::{ProductRequest, RingVrfError},
	platform::ProductContext,
};

use super::authority::{
	AuthorityError, AuthoritySession, AutoSigningGrant, BulletinAllowanceKey,
	CreateTransactionAuthorityRequest, ProductAuthority, SignPayloadAuthorityRequest,
	SignRawAuthorityRequest, StatementStoreAllowanceKey,
};

/// The host's account authority as one product connection sees it: every call
/// acts for that product, and host-level operations stay out of reach.
pub struct ProductAccounts {
	authority: Arc<dyn ProductAuthority>,
	product: ProductContext,
}

impl ProductAccounts {
	/// Bind the host's account authority to the product of one connection.
	pub fn new(authority: Arc<dyn ProductAuthority>, product: ProductContext) -> Self {
		Self { authority, product }
	}

	/// Current account-authority session, if connected.
	pub fn current_session(&self) -> Option<AuthoritySession> {
		self.authority.current_session()
	}

	/// Seed a product subtree for authority tests.
	#[cfg(test)]
	pub fn cache_product_subtree_for_test(
		&self,
		session: &SessionInfo,
		product_id: &str,
		public_key: [u8; 32],
	) {
		self.authority.cache_product_subtree_for_test(session, product_id, public_key);
	}

	/// Resolve the public key of `product_id`'s hard subtree.
	pub async fn product_subtree_public_key(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		product_id: String,
	) -> Result<[u8; 32], AuthorityError> {
		self.authority.product_subtree_public_key(cx, session, product_id).await
	}

	/// Whether resolving `product_id`'s subtree needs SSO and host consent.
	pub async fn subtree_resolution_reaches_account_holder(
		&self,
		session: &AuthoritySession,
		product_id: &str,
	) -> bool {
		self.authority
			.subtree_resolution_reaches_account_holder(session, product_id)
			.await
	}

	/// Whether this product's account can sign without confirmation.
	pub async fn auto_signing_status(
		&self,
		session: &AuthoritySession,
		account: &ProductAccountId,
	) -> Result<AutoSigningGrant, AuthorityError> {
		self.authority
			.auto_signing_status(session, &self.product.product_id, account)
			.await
	}

	/// Sign an RFC-0023 Merlin transcript with a product account.
	pub async fn sign_vrf(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountSignVrfRequest,
	) -> Result<VrfSignature, AuthorityError> {
		self.authority
			.sign_vrf(cx, session, self.product.product_id.clone(), request)
			.await
	}

	/// Sign a SCALE transaction payload for a product account.
	pub async fn sign_payload(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: SignPayloadAuthorityRequest,
	) -> Result<HostSignPayloadResponse, AuthorityError> {
		self.authority
			.sign_payload(cx, session, Some(&self.product.product_id), request)
			.await
	}

	/// Sign arbitrary bytes for a product account.
	pub async fn sign_raw(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<HostSignPayloadResponse, AuthorityError> {
		self.authority
			.sign_raw(cx, session, Some(&self.product.product_id), request, watermarked)
			.await
	}

	/// Build a transaction for a product account.
	pub async fn create_transaction(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: CreateTransactionAuthorityRequest,
	) -> Result<HostCreateTransactionResponse, AuthorityError> {
		self.authority
			.create_transaction(cx, session, Some(&self.product.product_id), request)
			.await
	}

	/// Derive a contextual alias for an explicit registered ring-VRF key.
	pub async fn account_alias(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountGetAliasRequest,
	) -> Result<HostAccountGetAliasResponse, RingVrfError> {
		self.authority.account_alias(cx, session, self.request(request)).await
	}

	/// Create a ring-VRF proof bound to a context and message.
	pub async fn create_proof(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountCreateProofRequest,
	) -> Result<HostAccountCreateProofResponse, RingVrfError> {
		self.authority.create_proof(cx, session, self.request(request)).await
	}

	/// Register a ring-VRF key owned by this product.
	pub async fn register_ring_vrf_key(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountRegisterRingVrfKeyRequest,
	) -> Result<HostAccountRegisterRingVrfKeyResponse, RingVrfError> {
		self.authority.register_ring_vrf_key(cx, session, self.request(request)).await
	}

	/// List registered ring-VRF keys.
	pub async fn list_ring_vrf_keys(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountListRingVrfKeysRequest,
	) -> Result<HostAccountListRingVrfKeysResponse, RingVrfError> {
		self.authority.list_ring_vrf_keys(cx, session, self.request(request)).await
	}

	/// Sign bytes directly with a registered ring-VRF key.
	pub async fn ring_vrf_sign(
		&self,
		session: &AuthoritySession,
		cx: &CallContext,
		request: HostAccountRingVrfSignRequest,
	) -> Result<HostAccountRingVrfSignResponse, RingVrfError> {
		self.authority.ring_vrf_sign(cx, session, self.request(request)).await
	}

	/// Allocate resources for this product.
	pub async fn allocate_resources(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
		request: HostRequestResourceAllocationRequest,
	) -> Result<HostRequestResourceAllocationResponse, AuthorityError> {
		self.authority
			.allocate_resources(cx, session, self.product.product_id.clone(), request)
			.await
	}

	/// Statement-store allowance key material for this product.
	pub async fn statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		self.authority
			.statement_store_allowance_key(cx, session, self.product.product_id.clone())
			.await
	}

	/// Forget this product's kept Statement Store key if it is still `public_key`.
	pub fn forget_statement_store_allowance_key(&self, public_key: [u8; 32]) {
		self.authority
			.forget_statement_store_allowance_key(&self.product.product_id, public_key);
	}

	/// Whether preimage submissions stay in the core instead of reaching Bulletin.
	#[cfg(feature = "test-host")]
	pub fn submits_preimages_locally(&self) -> bool {
		self.authority.submits_preimages_locally()
	}

	/// Bulletin allowance key material for this product.
	pub async fn bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.authority
			.bulletin_allowance_key(cx, session, self.product.product_id.clone())
			.await
	}

	/// Replace this product's Bulletin key after a submission was refused.
	pub async fn refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &AuthoritySession,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		self.authority
			.refresh_bulletin_allowance_key(cx, session, self.product.product_id.clone())
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
		self.authority
			.sign_statement_store_product_payload(
				cx,
				session,
				Some(&self.product.product_id),
				account,
				payload,
			)
			.await
	}

	/// Derive entropy scoped to this product.
	pub fn derive_entropy(
		&self,
		session: &AuthoritySession,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.authority.derive_entropy(session, &self.product.product_id, context)
	}

	/// Root-derived key for contact handles that stay stable across products.
	pub fn contacts_handle_key(
		&self,
		session: &AuthoritySession,
	) -> Result<[u8; 32], AuthorityError> {
		self.authority.contacts_handle_key(session)
	}

	fn request<P>(&self, payload: P) -> ProductRequest<P> {
		ProductRequest { calling_product_id: self.product.product_id.clone(), payload }
	}
}
