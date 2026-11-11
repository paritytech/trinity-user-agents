//! Product-facing resources capability adapters.

use crate::platform::{
	PermissionAuthorizationRequest, PermissionAuthorizationStatus, ResourceAllocationReview,
	UserConfirmationReview,
};
use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::{Entropy, ResourceAllocation},
	latest,
	versioned::{
		entropy::{HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse},
		resource_allocation::{
			HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
			HostRequestResourceAllocationResponse,
		},
	},
};

use crate::runtime::{
	ProductRuntimeHost, RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
	authority::AuthorityError, remote_authority_call, remote_authority_context_with_default,
	versioning::version,
};

impl ProductRuntimeHost {
	/// Resolve a durable allowance grant in the product connection's storage,
	/// never the signing host's (potentially differently scoped) storage.
	pub(in crate::runtime) async fn require_statement_store_allowance(
		&self,
		session: &crate::runtime::authority::AuthoritySession,
		derivation_index: Option<latest::DerivationIndex>,
	) -> Result<(), String> {
		let product_id = self.connection.product_id();
		let revision = self.accounts.grant_revision();
		let require_session = || {
			if self.accounts.grant_is_current(session, revision) {
				Ok(())
			} else {
				Err("Statement allowance session changed".to_string())
			}
		};
		require_session()?;
		let service = self.connection.permissions_service();
		let request = PermissionAuthorizationRequest::StatementStoreAllowance {
			derivation_index: derivation_index.clone(),
		};
		let mut status = service.authorization_status(&request).await.map_err(|err| {
			format!("statement allowance authorization read failed: {}", err.reason)
		})?;
		require_session()?;
		if status == PermissionAuthorizationStatus::NotDetermined {
			let resource = match derivation_index {
				Some(index) => latest::AllocatableResource::ProductStatementStoreAllowance(index),
				None => latest::AllocatableResource::StatementStoreAllowance,
			};
			let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
				calling_product_id: product_id.clone(),
				resources: vec![resource],
			});
			let confirmed = match self.confirm(&CallContext::default(), review).await {
				Ok(()) => true,
				Err(AuthorityError::Rejected) => false,
				Err(error) => {
					return Err(format!("statement allowance confirmation failed: {error}"));
				},
			};
			require_session()?;
			// An administrative decision made while the prompt was open wins.
			status = service.authorization_status(&request).await.map_err(|err| {
				format!("statement allowance authorization read failed: {}", err.reason)
			})?;
			require_session()?;
			if status == PermissionAuthorizationStatus::Authorized {
				return Ok(());
			}
			if status != PermissionAuthorizationStatus::NotDetermined {
				return Err(
					"Statement allowance authorization changed during confirmation".to_string()
				);
			}
			status = if confirmed {
				PermissionAuthorizationStatus::Authorized
			} else {
				PermissionAuthorizationStatus::Denied
			};
			service.set_authorization_status(&request, status).await.map_err(|err| {
				format!("statement allowance authorization write failed: {}", err.reason)
			})?;
			require_session()?;
		}
		if status != PermissionAuthorizationStatus::Authorized {
			return Err("Statement allowance authorization denied".to_string());
		}
		Ok(())
	}

	pub(in crate::runtime) async fn check_statement_store_allowance(
		&self,
		session: &crate::runtime::authority::AuthoritySession,
		derivation_index: Option<latest::DerivationIndex>,
	) -> Result<(), String> {
		let status = self
			.connection
			.permission_authorization_status(
				PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index },
			)
			.await
			.map_err(|err| {
				format!("statement allowance authorization read failed: {}", err.reason)
			})?;
		if !self.accounts.session_is_current(session) {
			return Err("Statement allowance session changed".to_string());
		}
		if status != PermissionAuthorizationStatus::Authorized {
			return Err("Statement allowance authorization denied".to_string());
		}
		Ok(())
	}
}

#[truapi::async_trait]
impl ResourceAllocation for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "resource_allocation.request"))]
	async fn request(
		&self,
		cx: &CallContext,
		request: HostRequestResourceAllocationRequest,
	) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
	{
		version(request)
			.exec(|inner| async move {
				let session = self.accounts.current_session().ok_or_else(|| {
					CallError::Domain(latest::ResourceAllocationError::Unknown {
						reason: "No active session".to_string(),
					})
				})?;
				let grant_revision = self.accounts.grant_revision();

				let product_id = self.connection.product_id();
				let require_session = || {
					if self.accounts.grant_is_current(&session, grant_revision) {
						Ok(())
					} else {
						Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
							reason: AuthorityError::Disconnected.to_string(),
						}))
					}
				};
				let service = self.connection.permissions_service();
				let mut grants = Vec::new();
				for resource in &inner.resources {
					let derivation_index = match resource {
						latest::AllocatableResource::StatementStoreAllowance => None,
						latest::AllocatableResource::ProductStatementStoreAllowance(index) => {
							Some(index.clone())
						},
						_ => continue,
					};
					let request = PermissionAuthorizationRequest::StatementStoreAllowance {
						derivation_index,
					};
					if grants.iter().any(|(existing, _)| existing == &request) {
						continue;
					}
					let status = service
						.authorization_status(&request)
						.await
						.map_err(|err| CallError::HostFailure { reason: err.reason })?;
					require_session()?;
					if status == PermissionAuthorizationStatus::Denied {
						return Err(CallError::Denied);
					}
					grants.push((request, status));
				}

				// An explicit request means additional quota, not merely ensure. Always
				// confirm it, even when implicit provisioning has a durable grant. The
				// same review establishes any missing grants without a second prompt.
				// A withdrawn call stops waiting on the review and authorizes nothing.
				let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
					calling_product_id: product_id.clone(),
					resources: inner.resources.clone(),
				});
				let confirmed = match self.confirm(cx, review).await {
					Ok(()) => true,
					Err(AuthorityError::Rejected) => false,
					Err(AuthorityError::ConfirmationFailed(error)) => {
						return Err(CallError::HostFailure {
							reason: format!("resource allocation confirmation failed: {error:?}"),
						});
					},
					Err(error) => {
						return Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
							reason: error.to_string(),
						}));
					},
				};
				require_session()?;
				for (request, before) in &grants {
					let current = service
						.authorization_status(request)
						.await
						.map_err(|err| CallError::HostFailure { reason: err.reason })?;
					require_session()?;
					// Never overwrite a decision changed by administration during the
					// review, including Authorized -> NotDetermined resets.
					if current != *before {
						return Err(CallError::Denied);
					}
				}
				for (request, before) in &grants {
					if *before == PermissionAuthorizationStatus::NotDetermined {
						let decision = if confirmed {
							PermissionAuthorizationStatus::Authorized
						} else {
							PermissionAuthorizationStatus::Denied
						};
						service
							.set_authorization_status(request, decision)
							.await
							.map_err(|err| CallError::HostFailure { reason: err.reason })?;
						require_session()?;
					}
				}
				if !confirmed {
					// Declining an increase does not revoke an existing durable grant.
					return Ok(latest::HostRequestResourceAllocationResponse {
						outcomes: vec![latest::AllocationOutcome::Rejected; inner.resources.len()],
					});
				}
				for (request, _) in &grants {
					let status = service
						.authorization_status(request)
						.await
						.map_err(|err| CallError::HostFailure { reason: err.reason })?;
					require_session()?;
					if status != PermissionAuthorizationStatus::Authorized {
						return Err(CallError::Denied);
					}
				}
				let cx = remote_authority_context_with_default(
					cx,
					RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
				);
				remote_authority_call(
					&cx,
					self.accounts.allocate_resources(&cx, &session, grant_revision, inner),
				)
				.await
				.map_err(|err| {
					CallError::Domain(latest::ResourceAllocationError::Unknown {
						reason: err.to_string(),
					})
				})
			})
			.await
	}
}

#[async_trait::async_trait]
impl Entropy for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "entropy.derive"))]
	async fn derive(
		&self,
		_cx: &CallContext,
		request: HostDeriveEntropyRequest,
	) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
		version(request)
			.exec(|latest::HostDeriveEntropyRequest { context }| async move {
				let session = self.accounts.current_session().ok_or_else(|| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: "Not connected".to_string(),
					})
				})?;
				let entropy = self.accounts.derive_entropy(&session, &context).map_err(|err| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: err.to_string(),
					})
				})?;

				Ok(latest::HostDeriveEntropyResponse { entropy })
			})
			.await
	}
}
