//! Account calls a pairing host forwards to its paired signing host.

use super::{
	super::{
		authority::{
			AuthorityCancelError, AuthorityError, BulletinAllowanceKey,
			CreateTransactionAuthorityRequest, PaymentTopUpAuthorityError,
			ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
			SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		},
		sso_remote::{
			SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SsoRemoteResponseError,
			SsoSessionKey,
		},
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
			.map_err(remote_authority_error)?
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
			.map_err(|error| ProductDeviceChatAuthorityError::from(remote_authority_error(error)))?
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
		let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
		if let Some(public_key) = self.known_product_subtree(session, cache_key.clone()).await {
			return Ok(public_key);
		}
		let public_key = self
			.session
			.call(cx, session, ProductSubtreeRequest { product_id: product_id.clone() })
			.await
			.map_err(remote_authority_error)?
			.map_err(remote_authority_error)?;
		if !self
			.persist_product_subtree_if_current(session, lifecycle_epoch, cache_key, public_key)
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
			.await
			.map_err(remote_authority_error)?
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
			.await
			.map_err(remote_authority_error)?
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
				.await
				.map_err(remote_authority_error)?
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
						.await
						.map_err(remote_authority_error)?
						.map_err(remote_authority_error);
				}
				let signature = self
					.session
					.call(cx, session, request)
					.await
					.map_err(remote_authority_error)?
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
		signed
			.map_err(remote_authority_error)?
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
		self.session
			.call(cx, session, request)
			.await
			.map_err(ring_vrf_transport_error)?
	}

	/// Forward a ring-VRF proof request to the paired signing host.
	pub async fn remote_create_proof(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountCreateProofRequest>,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		self.session
			.call(cx, session, request)
			.await
			.map_err(ring_vrf_transport_error)?
	}

	/// Forward a ring-VRF key registration request to the paired signing host.
	pub async fn remote_register_ring_vrf_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
	) -> Result<[u8; 32], RingVrfError> {
		self.session
			.call(cx, session, request)
			.await
			.map_err(ring_vrf_transport_error)?
	}

	/// Forward a ring-VRF key listing request to the paired signing host.
	pub async fn remote_list_ring_vrf_keys(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountListRingVrfKeysRequest>,
	) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
		self.session
			.call(cx, session, request)
			.await
			.map_err(ring_vrf_transport_error)?
	}

	/// Forward a direct ring-VRF signing request to the paired signing host.
	pub async fn remote_ring_vrf_sign(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		request: ProductRequest<latest::HostAccountRingVrfSignRequest>,
	) -> Result<Vec<u8>, RingVrfError> {
		self.session
			.call(cx, session, request)
			.await
			.map_err(ring_vrf_transport_error)?
	}

	/// Ask the paired signing host to allocate product resources, caching any
	/// returned allowance keys.
	pub async fn remote_allocate_resources(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
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
			.await
			.map_err(remote_authority_error)?
			.map_err(remote_authority_error)?;
		let outcomes = self
			.cache_and_map_allocation_outcomes(cx, session, lifecycle_epoch, product_id, outcomes)
			.await?;
		Ok(latest::HostRequestResourceAllocationResponse { outcomes })
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
			.await
			.map_err(remote_authority_error)?
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
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		if let Some(cached) = self
			.cached_statement_store_allowance_key(session, lifecycle_epoch, product_id)
			.await?
		{
			return Ok(cached);
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
				self.cache_statement_store_allowance_key(
					session,
					lifecycle_epoch,
					product_id,
					slot_account_key,
				)
				.await
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
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		if let Some(cached) =
			self.cached_bulletin_allowance_key(session, lifecycle_epoch, product_id).await?
		{
			return Ok(cached);
		}
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			lifecycle_epoch,
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
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		self.evict_bulletin_allowance_key(session, lifecycle_epoch, product_id).await?;
		self.allocate_bulletin_allowance_key(
			cx,
			session,
			lifecycle_epoch,
			product_id,
			OnExistingAllowancePolicy::Increase,
		)
		.await
	}

	async fn allocate_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		lifecycle_epoch: u64,
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
				self.cache_bulletin_allowance_key(
					session,
					lifecycle_epoch,
					product_id,
					slot_account_key,
				)
				.await
			},
			other => Err(unexpected_resource("bulletin allowance", &other)),
		}
	}

	/// Retain delegated keys locally and expose only allocation status to the product.
	async fn cache_and_map_allocation_outcomes(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &ProductId,
		outcomes: Vec<SsoAllocationOutcome>,
	) -> Result<Vec<latest::AllocationOutcome>, AuthorityError> {
		let mut mapped = Vec::with_capacity(outcomes.len());
		for outcome in outcomes {
			let outcome = match outcome {
				SsoAllocationOutcome::Allocated(resource) => match resource {
					SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
						self.cache_statement_store_allowance_key(
							session,
							lifecycle_epoch,
							product_id,
							slot_account_key,
						)
						.await?;
						latest::AllocationOutcome::Allocated
					},
					SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
						self.cache_bulletin_allowance_key(
							session,
							lifecycle_epoch,
							product_id,
							slot_account_key,
						)
						.await?;
						latest::AllocationOutcome::Allocated
					},
					// These register on-chain targets without delegating keys. In particular,
					// a product-account slot must never replace its host-managed allowance key.
					SsoAllocatedResource::ProductStatementStoreAllowance |
					SsoAllocatedResource::SmartContractAllowance => latest::AllocationOutcome::Allocated,
					SsoAllocatedResource::AutoSigning {
						product_root_private_key,
						ring_vrf_domain_entropy,
					} => {
						let expected_product_subtree_public_key =
							self.remote_product_subtree_public_key(cx, session, product_id).await?;
						self.remember_auto_signing_key(
							session,
							lifecycle_epoch,
							product_id,
							expected_product_subtree_public_key,
							product_root_private_key,
							ring_vrf_domain_entropy,
						)
						.await?;
						latest::AllocationOutcome::Allocated
					},
				},
				SsoAllocationOutcome::Rejected => latest::AllocationOutcome::Rejected,
				SsoAllocationOutcome::NotAvailable => latest::AllocationOutcome::NotAvailable,
			};
			mapped.push(outcome);
		}
		Ok(mapped)
	}
}

fn ring_vrf_transport_error(reason: SsoRemoteResponseError) -> RingVrfError {
	remote_authority_error(reason).into()
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

fn remote_authority_error(reason: impl Into<SsoRemoteResponseError>) -> AuthorityError {
	match reason.into() {
		SsoRemoteResponseError::Cancelled(err) => AuthorityError::Cancelled(
			AuthorityCancelError::new(err.remote_message_id(), err.reason()),
		),
		SsoRemoteResponseError::LocalDisconnected | SsoRemoteResponseError::PeerDisconnected => {
			AuthorityError::Disconnected
		},
		SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
			"Rejected" | "User rejected" => AuthorityError::Rejected,
			SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => {
				AuthorityError::Disconnected
			},
			_ => AuthorityError::Unknown { reason },
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn product_account_allocation_neither_creates_nor_replaces_delegated_allowance_keys() {
		use crate::{
			runtime::ProductRuntimeHost,
			test_support::{StubPlatform, runtime_config, sso_session_info, test_spawner},
		};
		use std::sync::Arc;

		for delegated in [false, true] {
			let platform = Arc::new(StubPlatform::default());
			let (config, product) = runtime_config("myapp.dot");
			let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
				platform,
				config,
				product,
				test_spawner(),
			);
			let session = sso_session_info();
			host.test_session_state().set_session(session.clone());
			let epoch = pairing_host.session.current_session_lifecycle_epoch();
			let product_id = ProductId::for_test("myapp.dot");
			let other_product = ProductId::for_test("other.dot");
			let key =
				crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16])
					.unwrap();
			let secret = key.secret.to_bytes();

			futures::executor::block_on(async {
				for owner in [&product_id, &other_product] {
					if delegated || owner == &other_product {
						let mapped = pairing_host
							.cache_and_map_allocation_outcomes(
								&CallContext::default(),
								&session,
								epoch,
								owner,
								vec![SsoAllocationOutcome::Allocated(
									SsoAllocatedResource::StatementStoreAllowance {
										slot_account_key: secret.to_vec(),
									},
								)],
							)
							.await
							.unwrap();
						assert_eq!(mapped, vec![latest::AllocationOutcome::Allocated]);
					}
				}
				let mapped = pairing_host
					.cache_and_map_allocation_outcomes(
						&CallContext::default(),
						&session,
						epoch,
						&product_id,
						vec![
							SsoAllocationOutcome::Allocated(
								SsoAllocatedResource::ProductStatementStoreAllowance,
							),
							SsoAllocationOutcome::Rejected,
							SsoAllocationOutcome::NotAvailable,
							SsoAllocationOutcome::Allocated(
								SsoAllocatedResource::SmartContractAllowance,
							),
						],
					)
					.await
					.unwrap();
				assert_eq!(
					mapped,
					vec![
						latest::AllocationOutcome::Allocated,
						latest::AllocationOutcome::Rejected,
						latest::AllocationOutcome::NotAvailable,
						latest::AllocationOutcome::Allocated,
					]
				);
				let cached = pairing_host
					.cached_statement_store_allowance_key(&session, epoch, &product_id)
					.await
					.unwrap();
				assert_eq!(cached.map(|key| key.secret), delegated.then_some(secret));
				let other = pairing_host
					.cached_statement_store_allowance_key(&session, epoch, &other_product)
					.await
					.unwrap()
					.expect("another product keeps its allowance");
				assert_eq!(other.secret, secret);
			});
		}
	}

	#[test]
	fn unexpected_resource_reasons_include_only_safe_discriminants() {
		let resource = SsoAllocatedResource::AutoSigning {
			product_root_private_key: [0xA5; 64],
			ring_vrf_domain_entropy: [0x5A; 32],
		};

		let AuthorityError::Unknown { reason } =
			unexpected_resource("statement-store allowance", &resource)
		else {
			panic!("expected an unknown authority error");
		};

		assert_eq!(reason, "Unexpected statement-store allowance response resource: auto-signing");
		assert!(!reason.contains("165, 165"));
	}
}
