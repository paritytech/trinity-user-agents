//! Account calls a pairing host forwards to its paired signing host.

use super::{
	super::{
		authority::{
			AuthorityError, BulletinAllowanceKey, CreateTransactionAuthorityRequest,
			SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		},
		sso_remote::{SsoRemoteResponseError, SsoSessionKey},
	},
	PairingHost,
};
use crate::{
	host_internal::sso_messages::{
		CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
		CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, ProductRequest,
		ProductSubtreeRequest, ResourceAllocationRequest, RingVrfError,
		SignRawWithLegacyAccountRequest, SignRequest, SsoAllocatedResource, SsoAllocationOutcome,
		SsoProductTxPayload,
	},
	host_logic::session::SessionInfo,
	platform::ProductId,
};

use tracing::instrument;
use truapi::{CallContext, latest};

impl PairingHost {
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
			.await?
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

	/// Ask the paired signing host to allocate product resources, caching any
	/// returned allowance keys.
	///
	/// Each request asks for one more slot, unless an earlier one never
	/// answered the product: the phone may still have granted that one, so
	/// the next asks for the existing slot instead. A product retrying a slow
	/// approval must not cost the person a second slot.
	pub async fn remote_allocate_resources(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		product_id: &ProductId,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		let lifecycle_epoch = self.session.current_session_lifecycle_epoch();
		let unanswered = session
			.sso
			.as_ref()
			.map(|sso| (SsoSessionKey::from_session(sso), product_id.clone()));
		// Marked before the request goes out, so a call dropped mid-wait
		// still leaves the mark for the product's next request.
		let retrying = unanswered.clone().is_some_and(|key| {
			!self
				.unanswered_allocations
				.lock()
				.expect("unanswered allocation mutex poisoned")
				.insert(key)
		});
		let on_existing = if retrying {
			OnExistingAllowancePolicy::Ignore
		} else {
			OnExistingAllowancePolicy::Increase
		};
		let answer = self
			.session
			.call(
				cx,
				session,
				ResourceAllocationRequest {
					calling_product_id: product_id.clone(),
					resources: request.resources,
					on_existing,
				},
			)
			.await;
		// A withdrawn call answers `Cancelled` to the product even when the
		// phone granted it, so only an answer the product receives clears it.
		if let Some(key) = unanswered &&
			answer.is_ok() &&
			!cx.cancel().is_cancelled()
		{
			self.unanswered_allocations
				.lock()
				.expect("unanswered allocation mutex poisoned")
				.remove(&key);
		}
		let outcomes = answer?.map_err(remote_authority_error)?;
		self.cache_allowance_outcomes(cx, session, lifecycle_epoch, product_id, &outcomes)
			.await?;
		Ok(latest::HostRequestResourceAllocationResponse {
			outcomes: outcomes.into_iter().map(Into::into).collect(),
		})
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

	async fn cache_allowance_outcomes(
		&self,
		cx: &CallContext,
		session: &SessionInfo,
		lifecycle_epoch: u64,
		product_id: &ProductId,
		outcomes: &[SsoAllocationOutcome],
	) -> Result<(), AuthorityError> {
		for outcome in outcomes {
			if let SsoAllocationOutcome::Allocated(resource) = outcome {
				match resource {
					SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
						self.cache_statement_store_allowance_key(
							session,
							lifecycle_epoch,
							product_id,
							slot_account_key.clone(),
						)
						.await?;
					},
					SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
						self.cache_bulletin_allowance_key(
							session,
							lifecycle_epoch,
							product_id,
							slot_account_key.clone(),
						)
						.await?;
					},
					SsoAllocatedResource::SmartContractAllowance => {},
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
							*product_root_private_key,
							*ring_vrf_domain_entropy,
						)
						.await?;
					},
				}
			}
		}
		Ok(())
	}
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

impl From<SsoAllocationOutcome> for latest::AllocationOutcome {
	fn from(outcome: SsoAllocationOutcome) -> Self {
		match outcome {
			SsoAllocationOutcome::Allocated(_) => Self::Allocated,
			SsoAllocationOutcome::Rejected => Self::Rejected,
			SsoAllocationOutcome::NotAvailable => Self::NotAvailable,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
