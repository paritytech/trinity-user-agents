//! Account calls a pairing host forwards to its paired signing host.

use super::{
	super::{
		allowances::AllowanceResource,
		authority::{
			AccountGrant, Allowance, AuthorityError, AutoSigningKey, BulletinAllowanceKey,
			CreateTransactionAuthorityRequest, PaymentTopUpAuthorityError,
			ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
			SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		},
		sso_remote::SsoRemoteResponseError,
	},
	PairingHost,
};
use crate::{
	host_internal::sso_messages::{
		CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
		CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, PaymentTopUpRequest,
		ProductRequest, ProductSubtreeRequest, ResourceAllocationRequest, RingVrfError,
		SignRawWithLegacyAccountRequest, SignRequest, SsoAllocatedResource, SsoAllocationOutcome,
		SsoProductDeviceChatOperation, SsoProductTxPayload,
	},
	host_logic::session::SessionInfo,
	platform::ProductId,
};

use tracing::instrument;
use truapi::{CallContext, latest};

impl PairingHost {
	/// Forward incoming funding without disclosing its keys to products.
	pub(super) async fn remote_payment_top_up(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: PaymentTopUpRequest,
	) -> Result<(), PaymentTopUpAuthorityError> {
		self.session
			.call(cx, session, request)
			.await
			.map_err(AuthorityError::from)?
			.map_err(|error| PaymentTopUpAuthorityError::Domain(error.0))
	}

	/// Forward a narrow Chat authority operation without exposing outgoing wallet keys.
	pub(super) async fn remote_product_device_chat(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductDeviceChatAuthorityRequest,
	) -> Result<latest::HostProductDeviceChatResponse, ProductDeviceChatAuthorityError> {
		let response = self
			.session
			.call(
				cx,
				session,
				ProductRequest {
					calling_product_id: request.calling_product_id,
					payload: SsoProductDeviceChatOperation::V3(request.operation),
				},
			)
			.await
			.map_err(|error| ProductDeviceChatAuthorityError::from(AuthorityError::from(error)))?
			.map_err(|error| {
				let truapi::versioned::account::HostProductDeviceChatError::V1(error) = error;
				ProductDeviceChatAuthorityError::Domain(error)
			})?;
		let truapi::versioned::account::HostProductDeviceChatResponse::V2(response) = response;
		Ok(response)
	}

	/// Resolve a product's hard-subtree public key, asking the Account Holder
	/// only when neither the memory cache nor storage already holds it.
	pub async fn remote_product_subtree_public_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		if session.sso.is_none() {
			return Err(AuthorityError::Disconnected);
		}
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(public_key) = self.grants.known_product_subtree(session, product_id).await {
			return Ok(public_key);
		}
		let public_key = self
			.session
			.call(cx, session, ProductSubtreeRequest { product_id: product_id.clone() })
			.await?
			.map_err(remote_authority_error)?;
		if !self
			.grants
			.persist_product_subtree_if_current(session, grant_revision, product_id, public_key)
			.await
		{
			return Err(AuthorityError::Disconnected);
		}
		Ok(public_key)
	}

	/// Forward RFC-0023 VRF signing to the paired Account Holder.
	pub async fn remote_sign_vrf(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		calling_product_id: &ProductId,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		self.session
			.call(
				cx,
				session,
				ProductRequest { calling_product_id: calling_product_id.clone(), payload: request },
			)
			.await?
			.map_err(|err| match err {
				latest::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
				latest::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
				latest::HostAccountSignVrfError::Unknown { reason } => {
					AuthorityError::Unknown { reason }
				},
			})
	}

	/// Forward a payload-signing request to the paired signing host.
	#[instrument(skip_all, fields(account_kind = match &request {
        SignPayloadAuthorityRequest::Product(_) => "product",
        SignPayloadAuthorityRequest::LegacyAccount { .. } => "legacy",
    }))]
	pub async fn remote_sign_payload(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: SignPayloadAuthorityRequest,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let request = match request {
			SignPayloadAuthorityRequest::Product(request) => request,
			SignPayloadAuthorityRequest::LegacyAccount { product_account, request } => {
				latest::HostSignPayloadRequest {
					account: product_account,
					payload: request.payload,
				}
			},
		};
		self.session
			.call(cx, session, SignRequest::Payload(Box::new(request)))
			.await?
			.map_err(remote_authority_error)
	}

	/// Forward a raw-signing request to the paired signing host.
	#[instrument(skip_all, fields(account_kind = match &request {
        SignRawAuthorityRequest::Product(_) => "product",
        SignRawAuthorityRequest::LegacyAccount { .. } => "legacy",
    }))]
	pub async fn remote_sign_raw(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		match request {
			SignRawAuthorityRequest::Product(request) => self
				.session
				.call(
					cx,
					session,
					if watermarked {
						SignRequest::Raw(request)
					} else {
						SignRequest::RawUnwatermarkedDeprecated(request)
					},
				)
				.await?
				.map_err(remote_authority_error),
			SignRawAuthorityRequest::LegacyAccount { account, request } => {
				let request = SignRawWithLegacyAccountRequest { account, data: request.payload };
				if !watermarked {
					return self
						.session
						.call(
							cx,
							session,
							SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request),
						)
						.await?
						.map_err(remote_authority_error);
				}
				let signature = self
					.session
					.call(cx, session, request)
					.await?
					.map_err(remote_authority_error)?;
				Ok(latest::HostSignPayloadResponse { signature, signed_transaction: None })
			},
		}
	}

	/// Forward a transaction-creation request to the paired signing host.
	#[instrument(skip_all, fields(account_kind = match &request {
        CreateTransactionAuthorityRequest::Product(_) => "product",
        CreateTransactionAuthorityRequest::LegacyAccount { .. } => "legacy",
        CreateTransactionAuthorityRequest::IdentityAccount(_) => "identity",
    }))]
	pub async fn remote_create_transaction(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		let signed = match request {
			CreateTransactionAuthorityRequest::Product(payload) => {
				self.session
					.call(
						cx,
						session,
						CreateTransactionRequest {
							payload: CreateTransactionPayload::V1(
								SsoProductTxPayload::from_resolved(payload),
							),
						},
					)
					.await
			},
			CreateTransactionAuthorityRequest::LegacyAccount { product_account, request } => {
				self.session
					.call(
						cx,
						session,
						CreateTransactionRequest {
							payload: CreateTransactionPayload::V1(SsoProductTxPayload {
								signer: product_account,
								genesis_hash: request.genesis_hash,
								call_data: request.call_data,
								extensions: request.extensions,
								tx_ext_version: request.tx_ext_version,
							}),
						},
					)
					.await
			},
			CreateTransactionAuthorityRequest::IdentityAccount(payload) => {
				self.session
					.call(
						cx,
						session,
						CreateTransactionWithLegacyAccountRequest {
							payload: CreateTransactionLegacyPayload::V1(payload),
						},
					)
					.await
			},
		};
		signed?
			.map(|transaction| latest::HostCreateTransactionResponse { transaction })
			.map_err(remote_authority_error)
	}

	/// Forward a contextual-alias request to the paired signing host.
	pub async fn remote_account_alias(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountGetAliasRequest>,
	) -> Result<latest::HostAccountGetAliasResponse, RingVrfError> {
		self.session.call(cx, session, request).await?
	}

	/// Forward a ring-VRF proof request to the paired signing host.
	pub async fn remote_create_proof(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		self.session.call(cx, session, request).await?
	}

	/// Forward a ring-VRF key registration request to the paired signing host.
	pub async fn remote_register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		self.session.call(cx, session, request).await?
	}

	/// Forward a ring-VRF key listing request to the paired signing host.
	pub async fn remote_list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		self.session.call(cx, session, request).await?
	}

	/// Forward a direct ring-VRF signing request to the paired signing host.
	pub async fn remote_ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		self.session.call(cx, session, request).await?
	}

	/// Ask the paired signing host to allocate product resources and keep the grants.
	///
	/// `grant_revision` is read before the allocation was confirmed, so a
	/// product reset during the review discards the result.
	/// Private resource material is consumed by the grant store before the
	/// corresponding public outcome is returned; products receive no keys.
	pub async fn remote_allocate_resources(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		self.grants.lifecycle().require_product_revision(grant_revision, product_id)?;
		let outcomes = self
			.session
			.call(
				cx,
				session,
				ResourceAllocationRequest {
					calling_product_id: product_id.clone(),
					resources: request.resources,
					on_existing: OnExistingAllowancePolicy::Increase,
				},
			)
			.await?
			.map_err(remote_authority_error)?;
		let mut public_outcomes = Vec::with_capacity(outcomes.len());
		for outcome in outcomes {
			public_outcomes.push(match outcome {
				SsoAllocationOutcome::Allocated(resource) => {
					self.retain_grant(
						cx,
						session,
						grant_revision,
						product_id,
						decode_grant(resource)?,
					)
					.await?;
					latest::AllocationOutcome::Allocated
				},
				SsoAllocationOutcome::Rejected => latest::AllocationOutcome::Rejected,
				SsoAllocationOutcome::NotAvailable => latest::AllocationOutcome::NotAvailable,
			});
		}
		Ok(latest::HostRequestResourceAllocationResponse { outcomes: public_outcomes })
	}

	/// Allocate exactly one allowance for the product and return its material.
	async fn remote_allowance_slot(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
		resource: latest::AllocatableResource,
		on_existing: OnExistingAllowancePolicy,
	) -> Result<SsoAllocatedResource, AuthorityError> {
		let name = allowance_name(&resource);
		let outcomes = self
			.session
			.call(
				cx,
				session,
				ResourceAllocationRequest {
					calling_product_id: product_id.clone(),
					resources: vec![resource],
					on_existing,
				},
			)
			.await?
			.map_err(remote_authority_error)?;
		match outcomes.into_iter().next() {
			Some(SsoAllocationOutcome::Allocated(resource)) => Ok(resource),
			Some(SsoAllocationOutcome::Rejected) => Err(AuthorityError::Rejected),
			Some(SsoAllocationOutcome::NotAvailable) => {
				Err(AuthorityError::Unavailable { reason: format!("{name} is not available") })
			},
			None => Err(AuthorityError::Unknown { reason: format!("Empty {name} response") }),
		}
	}

	/// Statement-store allowance key for the product, served from the cache
	/// or allocated by the paired signing host.
	pub async fn remote_statement_store_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(Allowance::StatementStore { key, .. }) = self
			.grants
			.cached_allowance(
				session,
				grant_revision,
				product_id,
				AllowanceResource::StatementStore,
			)
			.await?
		{
			return Ok(key);
		}
		match self
			.remote_allowance_slot(
				cx,
				session,
				product_id,
				latest::AllocatableResource::StatementStoreAllowance,
				OnExistingAllowancePolicy::Ignore,
			)
			.await?
		{
			SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
				let key = StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?;
				self.grants
					.retain_allowance(
						session,
						grant_revision,
						product_id,
						&Allowance::StatementStore { key: key.clone(), period: None },
					)
					.await?;
				Ok(key)
			},
			other => Err(unexpected_resource("statement-store allowance", &other)),
		}
	}

	/// Bulletin allowance key for the product, served from the cache or
	/// allocated by the paired signing host.
	pub async fn remote_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(Allowance::Bulletin(key)) = self
			.grants
			.cached_allowance(session, grant_revision, product_id, AllowanceResource::Bulletin)
			.await?
		{
			return Ok(key);
		}
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			grant_revision,
			product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
	}

	/// Evict the cached Bulletin allowance key and allocate a fresh one with
	/// an increased allowance, so a stale or exhausted slot is never reused.
	pub async fn remote_refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		self.grants
			.evict_bulletin_allowance(session, grant_revision, product_id)
			.await?;
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			grant_revision,
			product_id,
			OnExistingAllowancePolicy::Increase,
		)
		.await
	}

	async fn allocate_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		on_existing: OnExistingAllowancePolicy,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		match self
			.remote_allowance_slot(
				cx,
				session,
				product_id,
				latest::AllocatableResource::BulletinAllowance,
				on_existing,
			)
			.await?
		{
			SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
				let key = BulletinAllowanceKey::from_secret_bytes(slot_account_key)?;
				self.grants
					.retain_allowance(
						session,
						grant_revision,
						product_id,
						&Allowance::Bulletin(key.clone()),
					)
					.await?;
				Ok(key)
			},
			other => Err(unexpected_resource("bulletin allowance", &other)),
		}
	}

	/// Keep one grant the paired signing host issued for this host's product.
	async fn retain_grant(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		grant: AccountGrant,
	) -> Result<(), AuthorityError> {
		match grant {
			AccountGrant::Allowance(allowance) => {
				self.grants
					.retain_allowance(session, grant_revision, product_id, &allowance)
					.await
			},
			AccountGrant::AutoSigning(key) => {
				let expected =
					self.remote_product_subtree_public_key(cx, session, product_id).await?;
				self.grants
					.remember_auto_signing_key(session, grant_revision, product_id, expected, key)
					.await
			},
			AccountGrant::SmartContract | AccountGrant::ProductStatementStore => {
				let lifecycle = self.grants.lifecycle();
				self.current_private_session(&super::super::authority::authority_session(session))?;
				lifecycle.require_product_revision(grant_revision, product_id)
			},
		}
	}
}

fn decode_grant(resource: SsoAllocatedResource) -> Result<AccountGrant, AuthorityError> {
	Ok(match resource {
		SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
			AccountGrant::Allowance(Allowance::StatementStore {
				key: StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?,
				period: None,
			})
		},
		SsoAllocatedResource::BulletinAllowance { slot_account_key } => AccountGrant::Allowance(
			Allowance::Bulletin(BulletinAllowanceKey::from_secret_bytes(slot_account_key)?),
		),
		SsoAllocatedResource::SmartContractAllowance => AccountGrant::SmartContract,
		SsoAllocatedResource::ProductStatementStoreAllowance => AccountGrant::ProductStatementStore,
		SsoAllocatedResource::AutoSigning { product_root_private_key, ring_vrf_domain_entropy } => {
			AccountGrant::AutoSigning(AutoSigningKey::from_parts(
				product_root_private_key,
				ring_vrf_domain_entropy,
			))
		},
	})
}

fn allowance_name(resource: &latest::AllocatableResource) -> &'static str {
	match resource {
		latest::AllocatableResource::StatementStoreAllowance => "statement-store allowance",
		latest::AllocatableResource::BulletinAllowance => "bulletin allowance",
		_ => "resource",
	}
}

/// Reason for an allocation that came back as a different resource kind; names
/// only the kind so no key material reaches logs.
fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
	AuthorityError::Unknown {
		reason: format!("Unexpected {label} response resource: {}", resource.kind()),
	}
}

fn remote_authority_error(reason: String) -> AuthorityError {
	SsoRemoteResponseError::from(reason).into()
}

#[cfg(test)]
mod tests;
