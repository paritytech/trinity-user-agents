//! The signing host's answers to paired hosts: consent prompts, then the
//! local authority.

use std::sync::Arc;

use tracing::warn;
use truapi::latest as api;
use truapi_platform::{
    CreateTransactionReview, PermissionAuthorizationStatus, ResourceAllocationReview,
    SignPayloadReview, SignRawReview, StatementStoreProductSignReview, UserConfirmationReview,
    normalize_product_identifier,
};

use super::SigningHost;
use super::sso_responder::{
    AllowanceAllocationError, allocate_bulletin_allowance,
    allocate_product_statement_store_allowance, allocate_smart_contract_allowance,
    allocate_statement_store_allowance,
};
use crate::host_logic::permissions::PermissionsService;
use crate::host_logic::product_account::{
    derive_ring_vrf_domain_entropy, product_public_key_to_address,
};
use crate::host_logic::sso::messages::{
    CreateAccountProofResponse, CreateTransactionLegacyPayload, CreateTransactionPayload,
    CreateTransactionRequest, CreateTransactionResponse, CreateTransactionWithLegacyAccountRequest,
    GetAccountAliasResponse, ListRingVrfKeysResponse, OnExistingAllowancePolicy, PaymentTopUpError,
    PaymentTopUpRequest, PaymentTopUpResponse, ProductDeviceChatResponse, ProductRequest,
    ProductSubtreeRequest, ProductSubtreeResponse, RegisterRingVrfKeyResponse,
    ResourceAllocationRequest, ResourceAllocationResponse, RingVrfSignResponse,
    SignRawWithLegacyAccountRequest, SignRawWithLegacyAccountResponse, SignRequest, SignResponse,
    SignVrfResponse, SsoAllocatedResource, SsoAllocationOutcome, SsoProductDeviceChatOperation,
    StatementStoreProductSignRequest, StatementStoreProductSignResponse,
};
use crate::host_logic::sso::wire::ResponseOutcome;
use crate::host_logic::statement_store::validate_unsigned_statement_signing_payload;
use crate::runtime::authority::{
    AuthoritySession, CreateTransactionAuthorityRequest, ProductAuthority,
    ProductDeviceChatAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    chat_requires_preimage_submit,
};
use crate::runtime::sso_service::{SsoReply, SsoRequestContext};

/// SSO handlers served by a locally activated [`SigningHost`].
pub(crate) struct SigningHostSsoService {
    signing_host: Arc<SigningHost>,
}

impl SigningHostSsoService {
    /// Serve requests and prompt through the signing host's platform.
    pub(crate) fn new(signing_host: Arc<SigningHost>) -> Self {
        Self { signing_host }
    }

    /// The signing session captured before dispatching one request.
    pub(crate) fn current_session(&self) -> Option<AuthoritySession> {
        self.signing_host.current_session()
    }

    /// Run the platform confirmation seam; rejection and failure both refuse
    /// the operation with an opaque reason (host-spec B.7).
    async fn confirm(&self, review: UserConfirmationReview) -> Result<(), String> {
        match self.signing_host.platform.confirm_user_action(review).await {
            Ok(true) => Ok(()),
            Ok(false) => Err("Rejected".to_string()),
            Err(err) => Err(format!("confirmation failed: {}", err.reason)),
        }
    }

    async fn serve_sign(
        &self,
        cx: &SsoRequestContext,
        request: SignRequest,
    ) -> Result<api::HostSignPayloadResponse, String> {
        match request {
            SignRequest::Payload(request) => {
                let request = *request;
                self.confirm(UserConfirmationReview::SignPayload(
                    SignPayloadReview::Product(request.clone()),
                ))
                .await?;
                self.signing_host
                    .sign_payload(
                        &cx.call,
                        &cx.session,
                        // A relayed request carries no caller identity, and
                        // this role confirms every one of them anyway.
                        None,
                        SignPayloadAuthorityRequest::Product(request),
                    )
                    .await
                    .map_err(|err| err.to_string())
            }
            SignRequest::Raw(request) => self.serve_sign_raw(cx, request, true).await,
            SignRequest::RawUnwatermarkedDeprecated(request) => {
                self.serve_sign_raw(cx, request, false).await
            }
            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request) => {
                self.serve_sign_raw_with_legacy_account(cx, request, false)
                    .await
            }
        }
    }

    async fn serve_sign_raw(
        &self,
        cx: &SsoRequestContext,
        request: api::HostSignRawRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, String> {
        self.confirm(UserConfirmationReview::SignRaw(SignRawReview::Product {
            request: request.clone(),
            watermarked,
        }))
        .await?;
        self.signing_host
            .sign_raw(
                &cx.call,
                &cx.session,
                None,
                SignRawAuthorityRequest::Product(request),
                watermarked,
            )
            .await
            .map_err(|err| err.to_string())
    }

    async fn serve_sign_raw_with_legacy_account(
        &self,
        cx: &SsoRequestContext,
        request: SignRawWithLegacyAccountRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, String> {
        let public_request = api::HostSignRawWithLegacyAccountRequest {
            signer: product_public_key_to_address(request.account),
            payload: request.data,
        };
        self.confirm(UserConfirmationReview::SignRaw(
            SignRawReview::LegacyAccount {
                request: public_request.clone(),
                watermarked,
            },
        ))
        .await?;
        self.signing_host
            .sign_raw(
                &cx.call,
                &cx.session,
                None,
                SignRawAuthorityRequest::LegacyAccount {
                    account: request.account,
                    request: public_request,
                },
                watermarked,
            )
            .await
            .map_err(|err| err.to_string())
    }

    async fn serve_create_transaction(
        &self,
        cx: &SsoRequestContext,
        review: CreateTransactionReview,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<Vec<u8>, String> {
        self.confirm(UserConfirmationReview::CreateTransaction(review))
            .await?;
        self.signing_host
            .create_transaction(&cx.call, &cx.session, None, request)
            .await
            .map(|response| response.transaction)
            .map_err(|err| err.to_string())
    }

    async fn allocate(
        &self,
        session: &AuthoritySession,
        calling_product_id: &str,
        resource: api::AllocatableResource,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<SsoAllocationOutcome, AllowanceAllocationError> {
        let signing_host = &self.signing_host;
        let services = &signing_host.services;
        match resource {
            api::AllocatableResource::StatementStoreAllowance => {
                allocate_statement_store_allowance(
                    services,
                    signing_host,
                    session,
                    calling_product_id,
                    on_existing,
                )
                .await
                .map(|slot_account_key| {
                    SsoAllocationOutcome::Allocated(SsoAllocatedResource::StatementStoreAllowance {
                        slot_account_key,
                    })
                })
            }
            api::AllocatableResource::BulletinAllowance => allocate_bulletin_allowance(
                services,
                signing_host,
                session,
                calling_product_id,
                on_existing,
            )
            .await
            .map(|slot_account_key| {
                SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                    slot_account_key,
                })
            }),
            api::AllocatableResource::SmartContractAllowance(index) => {
                allocate_smart_contract_allowance(
                    services,
                    signing_host,
                    session,
                    calling_product_id,
                    index,
                    on_existing,
                )
                .await
                .map(|()| {
                    SsoAllocationOutcome::Allocated(SsoAllocatedResource::SmartContractAllowance)
                })
            }
            api::AllocatableResource::AutoSigning => {
                let product_root_private_key = signing_host
                    .product_subtree_secret(calling_product_id)
                    .map_err(AllowanceAllocationError::Authority)?;
                let root_entropy = signing_host.root_entropy()?;
                let ring_vrf_domain_entropy =
                    derive_ring_vrf_domain_entropy(&root_entropy, calling_product_id)
                        .map_err(super::product_authority_error)
                        .map_err(AllowanceAllocationError::Authority)?;
                Ok(SsoAllocationOutcome::Allocated(
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                        ring_vrf_domain_entropy,
                    },
                ))
            }
            api::AllocatableResource::ProductStatementStoreAllowance(index) => {
                allocate_product_statement_store_allowance(
                    services,
                    signing_host,
                    session,
                    calling_product_id,
                    &index,
                    on_existing,
                )
                .await
                .map(|()| {
                    SsoAllocationOutcome::Allocated(
                        SsoAllocatedResource::ProductStatementStoreAllowance,
                    )
                })
            }
        }
    }
}

fn allocation_reply(
    payload: Result<Vec<SsoAllocationOutcome>, String>,
    failures: Vec<String>,
) -> SsoReply<ResourceAllocationResponse> {
    let mut outcome = resource_allocation_outcome(&payload);
    if !failures.is_empty() {
        let details = failures.join("; ").replace(['\r', '\n'], " ");
        outcome.reason = Some(match outcome.reason {
            Some(summary) => format!("{summary}: {details}"),
            None => details,
        });
    }
    SsoReply::from(payload).with_outcome(outcome)
}

/// Transcript outcome for an allocation batch: `ok` only when every requested
/// resource was allocated; otherwise `rejected`, `partial`, or `not_available`
/// with a count summary.
fn resource_allocation_outcome(
    payload: &Result<Vec<SsoAllocationOutcome>, String>,
) -> ResponseOutcome {
    let outcomes = match payload {
        Ok(outcomes) => outcomes,
        Err(reason) => {
            return ResponseOutcome {
                outcome: "error",
                reason: Some(reason.clone()),
            };
        }
    };
    let total = outcomes.len();
    let count = |wanted: fn(&SsoAllocationOutcome) -> bool| {
        outcomes.iter().filter(|outcome| wanted(outcome)).count()
    };
    let allocated = count(|outcome| matches!(outcome, SsoAllocationOutcome::Allocated(_)));
    let rejected = count(|outcome| matches!(outcome, SsoAllocationOutcome::Rejected));
    let unavailable = count(|outcome| matches!(outcome, SsoAllocationOutcome::NotAvailable));
    if allocated == total {
        return ResponseOutcome {
            outcome: "ok",
            reason: None,
        };
    }
    if allocated > 0 {
        let mut reason = format!("{allocated} of {total} requested resources allocated");
        if rejected > 0 {
            reason.push_str(&format!("; {rejected} rejected"));
        }
        if unavailable > 0 {
            reason.push_str(&format!("; {unavailable} unavailable"));
        }
        return ResponseOutcome {
            outcome: "partial",
            reason: Some(reason),
        };
    }
    if rejected > 0 {
        let reason = if rejected == total {
            if total == 1 {
                "Requested resource was rejected".to_string()
            } else {
                format!("All {total} requested resources were rejected")
            }
        } else {
            format!("No resources allocated; {rejected} rejected; {unavailable} unavailable")
        };
        return ResponseOutcome {
            outcome: "rejected",
            reason: Some(reason),
        };
    }
    ResponseOutcome {
        outcome: "not_available",
        reason: Some(if total == 1 {
            "Requested resource is not available".to_string()
        } else {
            format!("None of the {total} requested resources are available")
        }),
    }
}

#[truapi_macros::sso_service]
impl SigningHostSsoService {
    /// Incoming funding uses only supplied secrets, not any outgoing-spend grant.
    async fn payment_top_up(
        &self,
        cx: &SsoRequestContext,
        request: PaymentTopUpRequest,
    ) -> PaymentTopUpResponse {
        self.signing_host
            .payment_top_up(&cx.call, &cx.session, request)
            .await
            .map_err(|error| PaymentTopUpError(error.into()))
    }

    /// Sign a payload or raw bytes with a product account.
    async fn sign(&self, cx: &SsoRequestContext, request: SignRequest) -> SignResponse {
        let payload = self.serve_sign(cx, request).await;
        if let Err(reason) = &payload {
            warn!(%reason, "sign request failed");
        }
        payload
    }

    /// Derive a contextual alias for a registered ring-VRF key.
    async fn get_account_alias(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountGetAliasRequest>,
    ) -> GetAccountAliasResponse {
        self.signing_host
            .account_alias(&cx.call, &cx.session, request)
            .await
    }

    /// Allocate SSO-backed resources for a product.
    async fn resource_allocation(
        &self,
        cx: &SsoRequestContext,
        request: ResourceAllocationRequest,
    ) -> ResourceAllocationResponse {
        let mut failures = Vec::new();
        let payload = async {
            let review = UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
                calling_product_id: request.calling_product_id.clone(),
                resources: request.resources.clone(),
            });
            match self.signing_host.platform.confirm_user_action(review).await {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(vec![
                        SsoAllocationOutcome::Rejected;
                        request.resources.len()
                    ]);
                }
                Err(err) => return Err(format!("confirmation failed: {}", err.reason)),
            }

            self.signing_host
                .require_current_session(&cx.session)
                .map_err(|err| err.to_string())?;
            let mut outcomes = Vec::with_capacity(request.resources.len());
            for resource in request.resources {
                self.signing_host
                    .require_current_session(&cx.session)
                    .map_err(|err| err.to_string())?;
                let outcome = self
                    .allocate(
                        &cx.session,
                        &request.calling_product_id,
                        resource,
                        request.on_existing,
                    )
                    .await;
                self.signing_host
                    .require_current_session(&cx.session)
                    .map_err(|err| err.to_string())?;
                outcomes.push(outcome.unwrap_or_else(|err| {
                    let reason = err.to_string();
                    warn!(%reason, "resource allocation item failed");
                    failures.push(reason);
                    SsoAllocationOutcome::NotAvailable
                }));
            }
            Ok(outcomes)
        }
        .await;
        if let Err(reason) = &payload {
            warn!(%reason, "resource allocation request failed");
        }
        allocation_reply(payload, failures)
    }

    /// Build a signed transaction for a product account.
    async fn create_transaction(
        &self,
        cx: &SsoRequestContext,
        request: CreateTransactionRequest,
    ) -> CreateTransactionResponse {
        let CreateTransactionPayload::V1(payload) = request.payload;
        self.serve_create_transaction(
            cx,
            CreateTransactionReview::Product(payload.clone()),
            CreateTransactionAuthorityRequest::Product(payload),
        )
        .await
    }

    /// Build a signed transaction for the wallet's identity account.
    async fn create_transaction_with_legacy_account(
        &self,
        cx: &SsoRequestContext,
        request: CreateTransactionWithLegacyAccountRequest,
    ) -> CreateTransactionResponse {
        let CreateTransactionLegacyPayload::V1(payload) = request.payload;
        self.serve_create_transaction(
            cx,
            CreateTransactionReview::LegacyAccount(payload.clone()),
            CreateTransactionAuthorityRequest::IdentityAccount(payload),
        )
        .await
    }

    /// Sign raw data with a legacy account.
    async fn sign_raw_with_legacy_account(
        &self,
        cx: &SsoRequestContext,
        request: SignRawWithLegacyAccountRequest,
    ) -> SignRawWithLegacyAccountResponse {
        self.serve_sign_raw_with_legacy_account(cx, request, true)
            .await
            .map(|response| response.signature)
    }

    /// Create a ring-VRF proof bound to a context and message.
    async fn create_account_proof(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountCreateProofRequest>,
    ) -> CreateAccountProofResponse {
        self.signing_host
            .create_proof(&cx.call, &cx.session, request)
            .await
    }

    /// Sign an RFC-0023 VRF transcript.
    async fn sign_vrf(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountSignVrfRequest>,
    ) -> SignVrfResponse {
        self.signing_host
            .sign_vrf(
                &cx.call,
                &cx.session,
                request.calling_product_id,
                request.payload,
            )
            .await
            .map_err(api::HostAccountSignVrfError::from)
    }

    /// Consent-free product hard-subtree public key.
    async fn product_subtree(
        &self,
        cx: &SsoRequestContext,
        request: ProductSubtreeRequest,
    ) -> ProductSubtreeResponse {
        self.signing_host
            .product_subtree_public_key(&cx.call, &cx.session, request.product_id)
            .await
            .map_err(|err| err.to_string())
    }

    /// Register a ring-VRF key owned by the calling product.
    async fn register_ring_vrf_key(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountRegisterRingVrfKeyRequest>,
    ) -> RegisterRingVrfKeyResponse {
        self.signing_host
            .register_ring_vrf_key(&cx.call, &cx.session, request)
            .await
    }

    /// List registered ring-VRF keys.
    async fn list_ring_vrf_keys(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountListRingVrfKeysRequest>,
    ) -> ListRingVrfKeysResponse {
        self.signing_host
            .list_ring_vrf_keys(&cx.call, &cx.session, request)
            .await
    }

    /// Sign bytes directly with a registered ring-VRF key.
    async fn ring_vrf_sign(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountRingVrfSignRequest>,
    ) -> RingVrfSignResponse {
        self.signing_host
            .ring_vrf_sign(&cx.call, &cx.session, request)
            .await
    }
    /// Sign a canonical unsigned Statement Store payload with a product account.
    async fn statement_store_product_sign(
        &self,
        cx: &SsoRequestContext,
        request: StatementStoreProductSignRequest,
    ) -> StatementStoreProductSignResponse {
        let calling_product_id = normalize_product_identifier(&request.calling_product_id)
            .map_err(|_| "invalid calling product identifier".to_string())?;
        let mut account = request.account;
        let account_product_id = normalize_product_identifier(&account.dot_ns_identifier)
            .map_err(|_| "invalid product account identifier".to_string())?;
        if account_product_id != calling_product_id {
            return Err("product account does not belong to the calling product".to_string());
        }
        account.dot_ns_identifier = calling_product_id.clone();
        validate_unsigned_statement_signing_payload(&request.payload)
            .map_err(|error| error.to_string())?;
        self.confirm(UserConfirmationReview::StatementStoreProductSign(
            StatementStoreProductSignReview {
                account: account.clone(),
                payload: request.payload.clone(),
            },
        ))
        .await?;
        self.signing_host
            .sign_statement_store_product_payload(
                &cx.call,
                &cx.session,
                Some(calling_product_id.as_str()),
                account,
                request.payload,
            )
            .await
            .map_err(|error| error.to_string())
    }

    /// Execute the same narrow Chat authority operations as local products.
    async fn product_device_chat(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<SsoProductDeviceChatOperation>,
    ) -> ProductDeviceChatResponse {
        use truapi::versioned::account::{
            HostProductDeviceChatError as WireError, HostProductDeviceChatResponse as WireResponse,
        };

        self.signing_host
            .require_current_session(&cx.session)
            .map_err(|_| WireError::V1(api::HostProductDeviceChatError::NotConnected))?;
        let calling_product_id = normalize_product_identifier(&request.calling_product_id)
            .map_err(|_| WireError::V1(api::HostProductDeviceChatError::InvalidRequest))?;
        let operation = match request.payload {
            SsoProductDeviceChatOperation::V3(operation) => operation,
            SsoProductDeviceChatOperation::V2(_) => {
                return Err(WireError::V1(
                    api::HostProductDeviceChatError::InvalidRequest,
                ));
            }
        };
        let permissions = PermissionsService::new(
            self.signing_host.platform.as_ref(),
            self.signing_host.platform.as_ref(),
            &calling_product_id,
        );
        if permissions
            .check_or_prompt_chat_authority()
            .await
            .map_err(|_| WireError::V1(api::HostProductDeviceChatError::StorageUnavailable))?
            != PermissionAuthorizationStatus::Authorized
        {
            return Err(WireError::V1(
                api::HostProductDeviceChatError::AccessNotGranted,
            ));
        }
        if chat_requires_preimage_submit(&operation)
            && permissions
                .check_or_prompt_remote(api::RemotePermissionRequest {
                    permission: api::RemotePermission::PreimageSubmit,
                })
                .await
                .map_err(|_| WireError::V1(api::HostProductDeviceChatError::StorageUnavailable))?
                != PermissionAuthorizationStatus::Authorized
        {
            return Err(WireError::V1(
                api::HostProductDeviceChatError::AccessNotGranted,
            ));
        }
        self.signing_host
            .product_device_chat(
                &cx.call,
                &cx.session,
                ProductDeviceChatAuthorityRequest {
                    calling_product_id,
                    operation,
                },
            )
            .await
            .map(WireResponse::V2)
            .map_err(|error| WireError::V1(error.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_allocation_summary_reflects_per_resource_outcomes() {
        let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::Allocated(
            SsoAllocatedResource::SmartContractAllowance,
        )]));
        assert_eq!((result.outcome, result.reason), ("ok", None));

        let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::Rejected]));
        assert_eq!(
            (result.outcome, result.reason.as_deref()),
            ("rejected", Some("Requested resource was rejected"))
        );

        let result = resource_allocation_outcome(&Ok(vec![
            SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
                slot_account_key: vec![1; 64],
            }),
            SsoAllocationOutcome::Rejected,
            SsoAllocationOutcome::NotAvailable,
        ]));
        assert_eq!(
            (result.outcome, result.reason.as_deref()),
            (
                "partial",
                Some("1 of 3 requested resources allocated; 1 rejected; 1 unavailable")
            )
        );

        let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::NotAvailable]));
        assert_eq!(
            (result.outcome, result.reason.as_deref()),
            ("not_available", Some("Requested resource is not available"))
        );
    }

    #[test]
    fn mixed_unallocated_resources_report_rejection() {
        let result = resource_allocation_outcome(&Ok(vec![
            SsoAllocationOutcome::Rejected,
            SsoAllocationOutcome::NotAvailable,
        ]));

        assert_eq!(
            (result.outcome, result.reason.as_deref()),
            (
                "rejected",
                Some("No resources allocated; 1 rejected; 1 unavailable")
            )
        );
    }

    #[test]
    fn allocation_transcript_includes_single_line_item_failures() {
        let answer = allocation_reply(
            Ok(vec![SsoAllocationOutcome::NotAvailable]),
            vec!["rpc\nfailed".to_string(), "provider\rdown".to_string()],
        )
        .finish(
            "allocation-1",
            crate::host_logic::sso::messages::v1::RemoteMessage::ResourceAllocationResponse,
        );

        assert_eq!(answer.outcome.outcome, "not_available");
        assert_eq!(
            answer.outcome.reason.as_deref(),
            Some("Requested resource is not available: rpc failed; provider down")
        );
    }
}
