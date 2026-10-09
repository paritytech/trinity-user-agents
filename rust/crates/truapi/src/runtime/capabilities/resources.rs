//! Product-facing resources capability adapters.

use crate::platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus, ResourceAllocationReview, UserConfirmationReview};
use tracing::instrument;
use truapi::api::{Entropy, ResourceAllocation};
use truapi::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use truapi::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use truapi::{CallContext, CallError, v01};

use crate::runtime::{
    ProductRuntimeHost, RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
    remote_authority_call, remote_authority_context_with_default, until_cancelled,
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
        let HostRequestResourceAllocationRequest::V1(inner) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "No active session".to_string(),
                },
            )));
        };

        let product_id = self.product_id();
        let require_session = || {
            if self
                .authority
                .session_is_current(&session, Some(&product_id))
            {
                Ok(())
            } else {
                Err(CallError::HostFailure {
                    reason: "Resource allocation session changed".to_string(),
                })
            }
        };
        let service = self.permissions_service();
        let mut grants = Vec::new();
        for resource in &inner.resources {
            let derivation_index = match resource {
                v01::AllocatableResource::StatementStoreAllowance => None,
                v01::AllocatableResource::ProductStatementStoreAllowance(index) => {
                    Some(index.clone())
                }
                _ => continue,
            };
            let request =
                PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index };
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
        let confirmed = until_cancelled(
            cx,
            self.platform
                .confirm_user_action(UserConfirmationReview::ResourceAllocation(
                    ResourceAllocationReview {
                        calling_product_id: product_id.clone(),
                        resources: inner.resources.clone(),
                    },
                )),
        )
        .await
        .map_err(|err| {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: err.to_string(),
                },
            ))
        })?
        .map_err(|err| CallError::HostFailure {
            reason: format!("resource allocation confirmation failed: {err:?}"),
        })?;
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
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "User rejected resource allocation".to_string(),
                },
            )));
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
            self.authority
                .allocate_resources(&cx, &session, product_id, inner),
        )
        .await
        .map(HostRequestResourceAllocationResponse::V1)
        .map_err(|err| {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: err.to_string(),
                },
            ))
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
        let HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context }) = request;
        let Some(session) = self.authority.current_session() else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Not connected".to_string(),
                },
            )));
        };
        let entropy = self
            .authority
            .derive_entropy(&session, &self.product_id(), &context)
            .map_err(|err| {
                CallError::Domain(HostDeriveEntropyError::V1(
                    v01::HostDeriveEntropyError::Unknown {
                        reason: err.to_string(),
                    },
                ))
            })?;

        Ok(HostDeriveEntropyResponse::V1(
            v01::HostDeriveEntropyResponse { entropy },
        ))
    }
}
