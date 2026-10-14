//! Product-facing resources capability adapters.

use crate::platform::{ResourceAllocationReview, UserConfirmationReview};
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

use super::from_latest_error;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::runtime::{
	ProductRuntimeHost, RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
	remote_authority_call, remote_authority_context_with_default, until_cancelled,
	versioning::version,
};

impl ProductRuntimeHost {
    /// Resolve a durable allowance grant in the product connection's storage,
    /// never the signing host's (potentially differently scoped) storage.
    pub(in crate::runtime) async fn require_statement_store_allowance(
        &self,
        session: &crate::runtime::authority::AuthoritySession,
        derivation_index: Option<v01::DerivationIndex>,
    ) -> Result<(), String> {
        let product_id = self.product_id();
        let require_session = || {
            if self
                .authority
                .session_is_current(session, Some(&product_id))
            {
                Ok(())
            } else {
                Err("Statement allowance session changed".to_string())
            }
        };
        require_session()?;
        let service = self.permissions_service();
        let request = PermissionAuthorizationRequest::StatementStoreAllowance {
            derivation_index: derivation_index.clone(),
        };
        let mut status = service
            .authorization_status(&request)
            .await
            .map_err(|err| {
                format!(
                    "statement allowance authorization read failed: {}",
                    err.reason
                )
            })?;
        require_session()?;
        if status == PermissionAuthorizationStatus::NotDetermined {
            let resource = match derivation_index {
                Some(index) => v01::AllocatableResource::ProductStatementStoreAllowance(index),
                None => v01::AllocatableResource::StatementStoreAllowance,
            };
            let confirmed = self
                .platform
                .confirm_user_action(UserConfirmationReview::ResourceAllocation(
                    ResourceAllocationReview {
                        calling_product_id: product_id.clone(),
                        resources: vec![resource],
                    },
                ))
                .await
                .map_err(|err| {
                    format!("statement allowance confirmation failed: {}", err.reason)
                })?;
            require_session()?;
            // An administrative decision made while the prompt was open wins.
            status = service
                .authorization_status(&request)
                .await
                .map_err(|err| {
                    format!(
                        "statement allowance authorization read failed: {}",
                        err.reason
                    )
                })?;
            require_session()?;
            if status == PermissionAuthorizationStatus::Authorized {
                return Ok(());
            }
            if status != PermissionAuthorizationStatus::NotDetermined {
                return Err(
                    "Statement allowance authorization changed during confirmation".to_string(),
                );
            }
            status = if confirmed {
                PermissionAuthorizationStatus::Authorized
            } else {
                PermissionAuthorizationStatus::Denied
            };
            service
                .set_authorization_status(&request, status)
                .await
                .map_err(|err| {
                    format!(
                        "statement allowance authorization write failed: {}",
                        err.reason
                    )
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
        derivation_index: Option<v01::DerivationIndex>,
    ) -> Result<(), String> {
        let status = self
            .permission_authorization_status(
                PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index },
            )
            .await
            .map_err(|err| {
                format!(
                    "statement allowance authorization read failed: {}",
                    err.reason
                )
            })?;
        if !self
            .authority
            .session_is_current(session, Some(&self.product_id()))
        {
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
		let version = request.version();
		let inner = request.into_latest();
		let result = async {
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
					reason: "No active session".to_string(),
				}));
			};

			let confirmed = until_cancelled(
				cx,
				self.confirm_product_action(UserConfirmationReview::ResourceAllocation(
					ResourceAllocationReview {
						calling_product_id: self.product_id(),
						resources: inner.resources.clone(),
					},
				)),
			)
			.await
			.map_err(|err| {
				CallError::Domain(latest::ResourceAllocationError::Unknown {
					reason: err.to_string(),
				})
			})?
			.map_err(|err| CallError::HostFailure {
				reason: format!("resource allocation confirmation failed: {err:?}"),
			})?;
			if !confirmed {
				return Err(CallError::Domain(latest::ResourceAllocationError::Unknown {
					reason: "User rejected resource allocation".to_string(),
				}));
			}
			let cx = remote_authority_context_with_default(
				cx,
				RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
			);
			remote_authority_call(
				&cx,
				self.authority.allocate_resources(&cx, &session, self.product_id(), inner),
			)
			.await
			.map_err(|err| {
				CallError::Domain(latest::ResourceAllocationError::Unknown {
					reason: err.to_string(),
				})
			})
		}
		.await;
		result
			.map(|response| HostRequestResourceAllocationResponse::from_latest(response, version))
			.map_err(|error| {
				from_latest_error::<HostRequestResourceAllocationError>(error, version)
			})
	}
}

#[truapi::async_trait]
impl Entropy for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "entropy.derive"))]
	async fn derive(
		&self,
		_cx: &CallContext,
		request: HostDeriveEntropyRequest,
	) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
		let version = request.version();
		let latest::HostDeriveEntropyRequest { context } = request.into_latest();
		let result = async {
			let Some(session) = self.authority.current_session() else {
				return Err(CallError::Domain(latest::HostDeriveEntropyError::Unknown {
					reason: "Not connected".to_string(),
				}));
			};
			let entropy = self
				.authority
				.derive_entropy(&session, &self.product_id(), &context)
				.map_err(|err| {
					CallError::Domain(latest::HostDeriveEntropyError::Unknown {
						reason: err.to_string(),
					})
				})?;

			Ok(latest::HostDeriveEntropyResponse { entropy })
		}
		.await;
		result
			.map(|response| HostDeriveEntropyResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostDeriveEntropyError>(error, version))
	}
}
