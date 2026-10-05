//! Authenticated SSO dispatch directly to the wallet account holder.

use std::sync::Arc;

use crate::platform::CreateTransactionReview;
use tracing::warn;
use truapi::latest as api;

use super::WalletAccountHolder;
use crate::host_internal::sso_messages::{
    CreateAccountProofResponse, CreateTransactionLegacyPayload, CreateTransactionPayload,
    CreateTransactionRequest, CreateTransactionResponse, CreateTransactionWithLegacyAccountRequest,
    GetAccountAliasResponse, ListRingVrfKeysResponse, ProductRequest, ProductSubtreeRequest,
    ProductSubtreeResponse, RegisterRingVrfKeyResponse, RemoteMessage, ResourceAllocationRequest,
    ResourceAllocationResponse, RingVrfSignResponse, SignRawWithLegacyAccountRequest,
    SignRawWithLegacyAccountResponse, SignRequest, SignResponse, SignVrfResponse,
    SsoAllocatedResource, SsoAllocationOutcome,
};
use crate::host_internal::sso_wire::ResponseOutcome;
use crate::host_logic::product_account::product_public_key_to_address;
use crate::runtime::authority::{
    AccountHolder, AuthoritySession, CreateTransactionAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest,
};
use crate::runtime::sso_service::{Dispatch, SsoReply, SsoRequestContext};

/// SSO handlers served by a locally activated [`WalletAccountHolder`].
pub struct SsoAccountHolderService {
    signing_host: Arc<WalletAccountHolder>,
    services: Arc<crate::runtime::RuntimeServices>,
    activation: Option<AuthoritySession>,
}

impl SsoAccountHolderService {
    /// Serve requests and prompt through the signing host's platform.
    pub fn new(signing_host: Arc<WalletAccountHolder>) -> Self {
        let activation = signing_host.current_session().map(AuthoritySession::for_remote);
        Self { services: signing_host.services.clone(), signing_host, activation }
    }

    /// Bind a transport to the activation that established its encrypted session.
    pub fn for_activation(signing_host: Arc<WalletAccountHolder>, activation: AuthoritySession) -> Self {
        Self { services: signing_host.services.clone(), signing_host, activation: Some(activation.for_remote()) }
    }

    /// The signing session captured before dispatching one request.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        let activation = self.activation.as_ref()?;
        self.signing_host.require_current_session(activation).ok()?;
        Some(activation.clone())
    }

    /// Answer `message`, unless the pairing host withdraws it first.
    ///
    /// A `Cancel` withdraws the request it names and is itself not answered.
    /// A withdrawn request has no response to post.
    pub async fn answer(&self, message: RemoteMessage) -> Dispatch {
        let withdrawals = self.services.sso_withdrawals();
        let Some(request) = withdrawals.begin(&message.message_id) else {
            return Dispatch::Withdrawn;
        };
        let cx = self.current_session().map(|session| {
            SsoRequestContext::new(&message.message_id, session, request.cancel.clone())
        });
        match self.dispatch(cx, message).await {
            Dispatch::Withdraw(target) => {
                withdrawals.withdraw(&target);
                Dispatch::Withdraw(target)
            }
            Dispatch::Response(_) if request.cancel.is_cancelled() => Dispatch::Withdrawn,
            dispatch => dispatch,
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
        _review: CreateTransactionReview,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<Vec<u8>, String> {
        self.signing_host
            .create_transaction(&cx.call, &cx.session, None, request)
            .await
            .map(|response| response.transaction)
            .map_err(|err| err.to_string())
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
impl SsoAccountHolderService {
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
        use crate::runtime::authority::{AccountAllocationOutcome, AccountGrant};
        let result = self
            .signing_host
            .allocate_grants(
                &cx.call,
                &cx.session,
                request.calling_product_id,
                api::HostRequestResourceAllocationRequest {
                    resources: request.resources,
                },
                request.on_existing,
            )
            .await;
        let failures = result.as_ref().map(|outcomes| outcomes.iter().filter_map(|outcome| match outcome { AccountAllocationOutcome::NotAvailable { reason } => reason.clone(), _ => None }).collect()).unwrap_or_default();
        let payload = result
            .map_err(|error| error.to_string())
            .and_then(|outcomes| {
                outcomes
                    .into_iter()
                    .map(|outcome| {
                        Ok(match outcome {
                            AccountAllocationOutcome::Rejected => SsoAllocationOutcome::Rejected,
                            AccountAllocationOutcome::NotAvailable { .. } => {
                                SsoAllocationOutcome::NotAvailable
                            }
                            AccountAllocationOutcome::Allocated(grant) => {
                                SsoAllocationOutcome::Allocated(match grant {
                                    AccountGrant::StatementStore(key) => {
                                        SsoAllocatedResource::StatementStoreAllowance {
                                            slot_account_key: key.secret.to_vec(),
                                        }
                                    }
                                    AccountGrant::Bulletin(key) => {
                                        SsoAllocatedResource::BulletinAllowance {
                                            slot_account_key: key.as_secret_bytes().to_vec(),
                                        }
                                    }
                                    AccountGrant::DelegatedSigning(key) => {
                                        SsoAllocatedResource::AutoSigning {
                                            product_root_private_key: *key.as_secret_bytes(),
                                            ring_vrf_domain_entropy: *key.ring_vrf_domain_entropy(),
                                        }
                                    }
                                    AccountGrant::SmartContract => {
                                        SsoAllocatedResource::SmartContractAllowance
                                    }
                                    AccountGrant::WalletAuthorization(_) => {
                                        return Err(
                                            "wallet authorization cannot be exported".to_string()
                                        );
                                    }
                                })
                            }
                        })
                    })
                    .collect()
            });
        allocation_reply(payload, failures)
    }

    /// Build a signed transaction for a product account.
    async fn create_transaction(
        &self,
        cx: &SsoRequestContext,
        request: CreateTransactionRequest,
    ) -> CreateTransactionResponse {
        let CreateTransactionPayload::V1(payload) = request.payload;
        let payload = payload.into_product_payload();
        self.serve_create_transaction(
            cx,
            CreateTransactionReview::Product {
                calling_product_id: None,
                payload: payload.clone(),
            },
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
            .sign_vrf_request(
                &cx.call,
                &cx.session,
                request.calling_product_id,
                request.payload,
                false,
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
            crate::host_internal::sso_messages::v1::RemoteMessage::ResourceAllocationResponse,
        );

        assert_eq!(answer.outcome.outcome, "not_available");
        assert_eq!(
            answer.outcome.reason.as_deref(),
            Some("Requested resource is not available: rpc failed; provider down")
        );
    }
}
