//! Canonical account operations translated to the existing paired SSO protocol.

use super::{
	HostSession,
	allowances::AllowanceResource,
	authority::{
		AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation, Allowance,
		AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
		CreateTransactionAuthorityRequest, GrantKeeping, SignPayloadAuthorityRequest,
		SignRawAuthorityRequest, StatementStoreAllowanceKey, SubtreeResolution,
		require_current_session,
	},
	ring_vrf_registry::{RingVrfRegistryStore, apply_ring_vrf_disclosure, validate_owner_listing},
	sso_remote::SsoRemoteResponseError,
	sso_request_service::SsoRequestService,
};
use crate::{
	Spawner,
	host_internal::{
		sso_messages::{
			CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
			CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, ProductRequest,
			ProductSubtreeRequest, ResourceAllocationRequest, RingVrfError,
			SignRawWithLegacyAccountRequest, SignRequest, SsoAllocatedResource,
			SsoAllocationOutcome, SsoProductTxPayload,
		},
		sso_wire::SsoRequest,
	},
	host_logic::entropy::derive_product_entropy_from_source,
	platform::ProductId,
};
use futures::{
	StreamExt,
	stream::{self, BoxStream},
};
use std::sync::Arc;
use truapi::{CallContext, latest};

/// Account-holder requests over one runtime's selected SSO channel.
pub struct RemoteAccountHolder {
	service: Arc<SsoRequestService>,
	registry: Arc<RingVrfRegistryStore>,
	spawner: Spawner,
}

impl RemoteAccountHolder {
	/// Use the runtime's existing session and outbound transport, and keep
	/// `registry` as this host's copy of the phone's ring-VRF registrations.
	pub fn new(
		service: Arc<SsoRequestService>,
		registry: Arc<RingVrfRegistryStore>,
		spawner: Spawner,
	) -> Self {
		Self { service, registry, spawner }
	}

	async fn remember_registration(
		&self,
		invocation: &AccountInvocation<'_>,
		request: latest::HostAccountRegisterRingVrfKeyRequest,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		let owner = invocation.caller.product_id().ok_or(RingVrfError::NotAllowlisted)?;
		let handle = latest::ProductAccountId {
			dot_ns_identifier: owner.clone(),
			derivation_index: request.index,
		};
		let mut update = self.registry.prepare_update(invocation.session.public_key).await?;
		self.require_current_session(invocation.session)?;
		update.register(handle, request.ring, public_key)?;
		update.persist().await
	}

	/// Register on the phone in the background; the host already answered from its kept key.
	fn forward_registration(
		&self,
		invocation: &AccountInvocation<'_>,
		request: latest::HostAccountRegisterRingVrfKeyRequest,
	) -> Result<(), RingVrfError> {
		let calling_product_id =
			invocation.caller.product_id().ok_or(RingVrfError::NotAllowlisted)?.clone();
		let service = Arc::downgrade(&self.service);
		let session = invocation.session.clone();
		self.spawner.spawn(async move {
			let Some(service) = service.upgrade() else {
				return;
			};
			let cx = CallContext::with_request_id(format!(
				"ring-vrf-registration-mirror:{}",
				super::sso_remote::sso_message_id()
			));
			let request = ProductRequest { calling_product_id, payload: request };
			let result = forward(&service, &cx, &session, request)
				.await
				.map_err(RingVrfError::from)
				.and_then(|answer| answer);
			if let Err(error) = result {
				tracing::warn!(?error, "ring-VRF registration mirror failed");
			}
		});
		Ok(())
	}

	async fn call_product<R: Send, T>(
		&self,
		invocation: &AccountInvocation<'_>,
		payload: R,
	) -> Result<T, RingVrfError>
	where
		ProductRequest<R>: SsoRequest<Response = Result<T, RingVrfError>>,
	{
		self.call(
			invocation,
			ProductRequest {
				calling_product_id: invocation
					.caller
					.product_id()
					.ok_or(RingVrfError::NotAllowlisted)?
					.clone(),
				payload,
			},
		)
		.await?
	}

	async fn call<R: SsoRequest>(
		&self,
		invocation: &AccountInvocation<'_>,
		request: R,
	) -> Result<R::Response, AuthorityError> {
		forward(&self.service, invocation.call, invocation.session, request).await
	}
}

/// Send `request` over `service`'s channel while `session` stays selected.
async fn forward<R: SsoRequest>(
	service: &SsoRequestService,
	cx: &CallContext,
	session: &AuthoritySession,
	request: R,
) -> Result<R::Response, AuthorityError> {
	let session = require_current_session(&service.session_state(), session)?;
	let cx = super::remote_authority_context(cx);
	super::remote_authority_call(&cx, async { Ok(service.call(&cx, &session, request).await?) })
		.await
}

#[async_trait::async_trait]
impl AccountHolder for RemoteAccountHolder {
	const GRANT_KEEPING: GrantKeeping = GrantKeeping::Paired;

	fn current_session(&self) -> Option<AuthoritySession> {
		self.service.current_session()
	}

	fn require_current_session(&self, session: &AuthoritySession) -> Result<(), AuthorityError> {
		require_current_session(&self.service.session_state(), session).map(|_| ())
	}

	async fn product_subtree_public_key<'a>(
		&'a self,
		invocation: AccountInvocation<'a>,
		product_id: &'a ProductId,
	) -> Result<SubtreeResolution<'a>, AuthorityError> {
		let session = require_current_session(&self.service.session_state(), invocation.session)?;
		let request = ProductSubtreeRequest { product_id: product_id.clone() };
		Ok(SubtreeResolution::Ask(Box::pin(async move {
			self.service
				.call(invocation.call, &session, request)
				.await?
				.map_err(remote_authority_error)
		})))
	}

	async fn sign_vrf(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountSignVrfRequest,
	) -> Result<latest::VrfSignature, AuthorityError> {
		self.call(
			&invocation,
			ProductRequest {
				calling_product_id: invocation
					.caller
					.product_id()
					.ok_or(AuthorityError::Rejected)?
					.clone(),
				payload: request,
			},
		)
		.await?
		.map_err(|error| match error {
			latest::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
			latest::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
			latest::HostAccountSignVrfError::Unknown { reason } => {
				AuthorityError::Unknown { reason }
			},
		})
	}

	async fn sign_payload(
		&self,
		invocation: AccountInvocation<'_>,
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
		self.call(&invocation, SignRequest::Payload(Box::new(request)))
			.await?
			.map_err(remote_authority_error)
	}

	async fn sign_raw(
		&self,
		invocation: AccountInvocation<'_>,
		request: SignRawAuthorityRequest,
		watermarked: bool,
	) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
		let request = match request {
			SignRawAuthorityRequest::Product(request) => request,
			SignRawAuthorityRequest::LegacyAccount { product_account, request } => {
				latest::HostSignRawRequest { account: product_account, payload: request.payload }
			},
			SignRawAuthorityRequest::IdentityAccount { account, request } => {
				let request = SignRawWithLegacyAccountRequest { account, data: request.payload };
				if !watermarked {
					return self
						.call(
							&invocation,
							SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request),
						)
						.await?
						.map_err(remote_authority_error);
				}
				let signature =
					self.call(&invocation, request).await?.map_err(remote_authority_error)?;
				return Ok(latest::HostSignPayloadResponse { signature, signed_transaction: None });
			},
		};
		self.call(
			&invocation,
			if watermarked {
				SignRequest::Raw(request)
			} else {
				SignRequest::RawUnwatermarkedDeprecated(request)
			},
		)
		.await?
		.map_err(remote_authority_error)
	}

	async fn create_transaction(
		&self,
		invocation: AccountInvocation<'_>,
		request: CreateTransactionAuthorityRequest,
	) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
		let signed = match request {
			CreateTransactionAuthorityRequest::Product(payload) => {
				self.call(
					&invocation,
					CreateTransactionRequest {
						payload: CreateTransactionPayload::V1(SsoProductTxPayload::from_resolved(
							payload,
						)),
					},
				)
				.await
			},
			CreateTransactionAuthorityRequest::LegacyAccount { product_account, request } => {
				self.call(
					&invocation,
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
				self.call(
					&invocation,
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

	async fn account_alias(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountGetAliasRequest,
	) -> Result<latest::HostAccountGetAliasResponse, RingVrfError> {
		self.call_product(&invocation, request).await
	}

	async fn create_proof(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountCreateProofRequest,
	) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
		self.call_product(&invocation, request).await
	}

	async fn register_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountRegisterRingVrfKeyRequest,
	) -> Result<latest::HostAccountRegisterRingVrfKeyResponse, RingVrfError> {
		let public_key = self.call_product(&invocation, request.clone()).await?;
		self.remember_registration(&invocation, request, public_key).await?;
		Ok(public_key)
	}

	async fn record_ring_vrf_key(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountRegisterRingVrfKeyRequest,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		self.remember_registration(&invocation, request.clone(), public_key).await?;
		self.forward_registration(&invocation, request)
	}

	async fn list_ring_vrf_keys(
		&self,
		invocation: AccountInvocation<'_>,
		mut request: latest::HostAccountListRingVrfKeysRequest,
	) -> Result<latest::HostAccountListRingVrfKeysResponse, RingVrfError> {
		let owner = request.owner.clone();
		let disclosure = request.disclosure;
		if invocation.caller.product_id() == Some(&owner) {
			request.disclosure = latest::RingVrfKeyDisclosure::PublicKey;
		}
		let mut entries: Vec<latest::RegisteredRingVrfKey> =
			self.call_product(&invocation, request).await?;
		validate_owner_listing(&owner, &entries)?;
		if entries.iter().all(|entry| entry.public_key.is_some()) {
			let mut update = self.registry.prepare_update(invocation.session.public_key).await?;
			self.require_current_session(invocation.session)?;
			entries = update.reconcile_owner(&owner, entries)?;
			update.persist().await?;
		}
		self.require_current_session(invocation.session)?;
		apply_ring_vrf_disclosure(&mut entries, disclosure);
		Ok(entries)
	}

	async fn ring_vrf_sign(
		&self,
		invocation: AccountInvocation<'_>,
		request: latest::HostAccountRingVrfSignRequest,
	) -> Result<latest::HostAccountRingVrfSignResponse, RingVrfError> {
		self.call_product(&invocation, request).await
	}

	async fn sign_statement_store_product_payload(
		&self,
		invocation: AccountInvocation<'_>,
		_account: latest::ProductAccountId,
		_payload: Vec<u8>,
	) -> Result<[u8; 64], AuthorityError> {
		self.require_current_session(invocation.session)?;
		Err(AuthorityError::Unavailable { reason: "pairing host: exact statement proof signing needs an AutoSigning capability; the current SSO raw-signing protocol cannot carry it".to_string() })
	}

	fn derive_entropy(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		let session = require_current_session(&self.service.session_state(), session)?;
		if session.sso.is_none() {
			return Err(AuthorityError::Disconnected);
		}
		let source = session.root_entropy_source.ok_or_else(|| AuthorityError::Unavailable {
			reason: "Session secret missing".to_string(),
		})?;
		derive_product_entropy_from_source(&source, product_id, context)
			.map_err(|error| AuthorityError::Unknown { reason: error.to_string() })
	}

	fn contact_handles(
		&self,
		session: &AuthoritySession,
	) -> Result<crate::runtime::contacts::ContactHandles, AuthorityError> {
		let session = require_current_session(&self.service.session_state(), session)?;
		let source = session.root_entropy_source.ok_or_else(|| AuthorityError::Unavailable {
			reason: "Session secret missing".to_string(),
		})?;
		Ok(crate::runtime::contacts::ContactHandles::from_root_entropy_source(&source))
	}

	async fn allocate_grants<'a>(
		&'a self,
		invocation: AccountInvocation<'a>,
		request: latest::HostRequestResourceAllocationRequest,
		policy: OnExistingAllowancePolicy,
	) -> Result<BoxStream<'a, Result<AccountGrantOutcome, AuthorityError>>, AuthorityError> {
		let session = require_current_session(&self.service.session_state(), invocation.session)?;
		let request = ResourceAllocationRequest {
			calling_product_id: invocation
				.caller
				.product_id()
				.ok_or(AuthorityError::Rejected)?
				.clone(),
			resources: request.resources,
			on_existing: policy,
		};
		Ok(stream::once(async move {
			self.service
				.call(invocation.call, &session, request)
				.await?
				.map_err(remote_authority_error)
		})
		.flat_map(|result| {
			stream::iter(match result {
				Ok(outcomes) => outcomes
					.into_iter()
					.map(|outcome| match outcome {
						SsoAllocationOutcome::Allocated(resource) => {
							decode_grant(resource).map(AccountGrantOutcome::Allocated)
						},
						SsoAllocationOutcome::Rejected => Ok(AccountGrantOutcome::Rejected),
						SsoAllocationOutcome::NotAvailable => {
							Ok(AccountGrantOutcome::NotAvailable { reason: None })
						},
					})
					.collect::<Vec<_>>(),
				Err(error) => vec![Err(error)],
			})
		})
		.boxed())
	}

	async fn ensure_allowance(
		&self,
		invocation: AccountInvocation<'_>,
		resource: AllowanceResource,
		policy: OnExistingAllowancePolicy,
	) -> Result<AccountGrant, AuthorityError> {
		let expected = resource;
		let resource = match resource {
			AllowanceResource::StatementStore => {
				latest::AllocatableResource::StatementStoreAllowance
			},
			AllowanceResource::Bulletin => latest::AllocatableResource::BulletinAllowance,
		};
		let name = match resource {
			latest::AllocatableResource::StatementStoreAllowance => "statement-store allowance",
			_ => "bulletin allowance",
		};
		let outcomes = self
			.call(
				&invocation,
				ResourceAllocationRequest {
					calling_product_id: invocation
						.caller
						.product_id()
						.ok_or(AuthorityError::Rejected)?
						.clone(),
					resources: vec![resource],
					on_existing: policy,
				},
			)
			.await?
			.map_err(remote_authority_error)?;
		match outcomes.into_iter().next() {
			Some(SsoAllocationOutcome::Allocated(resource)) => {
				if !matches!(
					(expected, &resource),
					(
						AllowanceResource::StatementStore,
						SsoAllocatedResource::StatementStoreAllowance { .. }
					) | (
						AllowanceResource::Bulletin,
						SsoAllocatedResource::BulletinAllowance { .. }
					)
				) {
					return Err(unexpected_resource(name, &resource));
				}
				decode_grant(resource)
			},
			Some(SsoAllocationOutcome::Rejected) => Err(AuthorityError::Rejected),
			Some(SsoAllocationOutcome::NotAvailable) => {
				Err(AuthorityError::Unavailable { reason: format!("{name} is not available") })
			},
			None => Err(AuthorityError::Unknown { reason: format!("Empty {name} response") }),
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
		SsoAllocatedResource::AutoSigning { product_root_private_key, ring_vrf_domain_entropy } => {
			AccountGrant::AutoSigning(AutoSigningKey::from_parts(
				product_root_private_key,
				ring_vrf_domain_entropy,
			))
		},
	})
}

fn remote_authority_error(reason: String) -> AuthorityError {
	SsoRemoteResponseError::from(reason).into()
}

/// Reason for an allocation that came back as a different resource kind; names
/// only the kind so no key material reaches logs.
fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
	AuthorityError::Unknown {
		reason: format!("Unexpected {label} response resource: {}", resource.kind()),
	}
}

#[cfg(test)]
mod tests;
