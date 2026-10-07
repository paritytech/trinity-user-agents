//! Incoming SSO account requests and wallet grant responses.

use std::sync::Arc;

use futures::StreamExt;
use tracing::warn;
use truapi::latest as api;

use super::authority::{AccountGrant, AccountGrantOutcome};
use super::signing_host::WalletAccountHolder;
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
    AccountHolder, AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest,
};
use crate::runtime::sso_service::{Dispatch, SsoReply, SsoRequestContext, SsoWithdrawals};

/// Incoming requests for one authenticated wallet activation and peer.
pub struct SsoAccountHolderService {
    wallet: Arc<WalletAccountHolder>,
    session: AuthoritySession,
    withdrawals: SsoWithdrawals,
}

/// Withdrawals shared with this peer's transport reader.
pub fn withdrawals(service: &SsoAccountHolderService) -> &SsoWithdrawals {
    &service.withdrawals
}

impl SsoAccountHolderService {
    /// Bind one peer to the activation that authenticated its transport.
    pub fn new(wallet: Arc<WalletAccountHolder>, session: AuthoritySession) -> Self {
        Self {
            wallet,
            session,
            withdrawals: SsoWithdrawals::default(),
        }
    }

    /// Require the activation that authenticated this peer.
    pub fn require_current_session(&self) -> Result<(), AuthorityError> {
        self.wallet.require_current_session(&self.session)
    }

    /// Apply a withdrawal without waiting behind the request it cancels.
    pub fn handle_control(&self, message: &RemoteMessage) -> Option<Dispatch> {
        let target = message.withdrawn_request_id()?;
        self.withdrawals.withdraw(target);
        Some(Dispatch::Withdraw(target.to_string()))
    }

    /// Answer a request without releasing stale or withdrawn wallet results.
    pub async fn answer(&self, message: RemoteMessage) -> Result<Dispatch, AuthorityError> {
        if let Some(control) = self.handle_control(&message) {
            return Ok(control);
        }
        let Some(request) = self.withdrawals.begin(&message.message_id) else {
            return Ok(Dispatch::Withdrawn);
        };
        if self.require_current_session().is_err() {
            return Ok(self.dispatch(None, message).await);
        }
        let cx = SsoRequestContext::new(
            &message.message_id,
            self.session.clone(),
            request.cancel.clone(),
        );
        let dispatch = self.dispatch(Some(cx), message).await;
        if matches!(dispatch, Dispatch::Response(_)) {
            if request.cancel.is_cancelled() {
                return Ok(Dispatch::Withdrawn);
            }
            self.require_current_session()?;
        }
        Ok(dispatch)
    }

    async fn serve_sign(
        &self,
        cx: &SsoRequestContext,
        request: SignRequest,
    ) -> Result<api::HostSignPayloadResponse, String> {
        match request {
            SignRequest::Payload(request) => {
                self.wallet
                    .sign_payload(
                        cx.account_invocation(None),
                        SignPayloadAuthorityRequest::Product(*request),
                    )
                    .await
            }
            SignRequest::Raw(request) => {
                self.wallet
                    .sign_raw(
                        cx.account_invocation(None),
                        SignRawAuthorityRequest::Product(request),
                        true,
                    )
                    .await
            }
            SignRequest::RawUnwatermarkedDeprecated(request) => {
                self.wallet
                    .sign_raw(
                        cx.account_invocation(None),
                        SignRawAuthorityRequest::Product(request),
                        false,
                    )
                    .await
            }
            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request) => {
                self.serve_sign_raw_with_legacy_account(cx, request, false)
                    .await
            }
        }
        .map_err(|error| error.to_string())
    }

    async fn serve_sign_raw_with_legacy_account(
        &self,
        cx: &SsoRequestContext,
        request: SignRawWithLegacyAccountRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        let public_request = api::HostSignRawWithLegacyAccountRequest {
            signer: product_public_key_to_address(request.account),
            payload: request.data,
        };
        self.wallet
            .sign_raw(
                cx.account_invocation(None),
                SignRawAuthorityRequest::IdentityAccount {
                    account: request.account,
                    request: public_request,
                },
                watermarked,
            )
            .await
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
        self.wallet
            .account_alias(
                cx.account_invocation(Some(&request.calling_product_id)),
                request.payload,
            )
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
            let count = request.resources.len();
            let mut grants = match self
                .wallet
                .allocate_grants(
                    cx.account_invocation(Some(&request.calling_product_id)),
                    api::HostRequestResourceAllocationRequest {
                        resources: request.resources,
                    },
                    request.on_existing,
                )
                .await
            {
                Ok(grants) => grants,
                Err(AuthorityError::Rejected) => {
                    return Ok(vec![SsoAllocationOutcome::Rejected; count]);
                }
                Err(error) => return Err(error.to_string()),
            };
            let mut outcomes = Vec::with_capacity(count);
            while let Some(grant) = grants.next().await {
                outcomes.push(match grant {
                    Ok(AccountGrantOutcome::Allocated(grant)) => {
                        SsoAllocationOutcome::Allocated(match grant {
                            AccountGrant::StatementStore { key, .. } => {
                                SsoAllocatedResource::StatementStoreAllowance {
                                    slot_account_key: key.secret.to_vec(),
                                }
                            }
                            AccountGrant::Bulletin(key) => {
                                SsoAllocatedResource::BulletinAllowance {
                                    slot_account_key: key.as_secret_bytes().to_vec(),
                                }
                            }
                            AccountGrant::SmartContract => {
                                SsoAllocatedResource::SmartContractAllowance
                            }
                            AccountGrant::AutoSigning(key) => SsoAllocatedResource::AutoSigning {
                                product_root_private_key: *key.as_secret_bytes(),
                                ring_vrf_domain_entropy: *key.ring_vrf_domain_entropy(),
                            },
                            AccountGrant::WalletAuthorization(_) => {
                                unreachable!("remote wallet allocation exports a signing key")
                            }
                        })
                    }
                    Ok(AccountGrantOutcome::Rejected) => {
                        failures.push(AuthorityError::Rejected.to_string());
                        SsoAllocationOutcome::NotAvailable
                    }
                    Ok(AccountGrantOutcome::NotAvailable { reason }) => {
                        if let Some(reason) = reason {
                            warn!(%reason, "resource allocation item failed");
                            failures.push(reason);
                        }
                        SsoAllocationOutcome::NotAvailable
                    }
                    Err(error) => return Err(error.to_string()),
                });
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
        let payload = payload.into_product_payload();
        self.wallet
            .create_transaction(
                cx.account_invocation(None),
                CreateTransactionAuthorityRequest::Product(payload),
            )
            .await
            .map(|response| response.transaction)
            .map_err(|error| error.to_string())
    }

    /// Build a signed transaction for the wallet's identity account.
    async fn create_transaction_with_legacy_account(
        &self,
        cx: &SsoRequestContext,
        request: CreateTransactionWithLegacyAccountRequest,
    ) -> CreateTransactionResponse {
        let CreateTransactionLegacyPayload::V1(payload) = request.payload;
        self.wallet
            .create_transaction(
                cx.account_invocation(None),
                CreateTransactionAuthorityRequest::IdentityAccount(payload),
            )
            .await
            .map(|response| response.transaction)
            .map_err(|error| error.to_string())
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
            .map_err(|error| error.to_string())
    }

    /// Create a ring-VRF proof bound to a context and message.
    async fn create_account_proof(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountCreateProofRequest>,
    ) -> CreateAccountProofResponse {
        self.wallet
            .create_proof(
                cx.account_invocation(Some(&request.calling_product_id)),
                request.payload,
            )
            .await
    }

    /// Sign an RFC-0023 VRF transcript.
    async fn sign_vrf(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountSignVrfRequest>,
    ) -> SignVrfResponse {
        self.wallet
            .sign_vrf(
                cx.account_invocation(Some(&request.calling_product_id)),
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
        self.wallet
            .product_subtree_public_key(cx.account_invocation(None), request.product_id)
            .await
            .map_err(|err| err.to_string())?
            .await
            .map_err(|err| err.to_string())
    }

    /// Register a ring-VRF key owned by the calling product.
    async fn register_ring_vrf_key(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountRegisterRingVrfKeyRequest>,
    ) -> RegisterRingVrfKeyResponse {
        self.wallet
            .register_ring_vrf_key(
                cx.account_invocation(Some(&request.calling_product_id)),
                request.payload,
            )
            .await
    }

    /// List registered ring-VRF keys.
    async fn list_ring_vrf_keys(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountListRingVrfKeysRequest>,
    ) -> ListRingVrfKeysResponse {
        self.wallet
            .list_ring_vrf_keys(
                cx.account_invocation(Some(&request.calling_product_id)),
                request.payload,
            )
            .await
    }

    /// Sign bytes directly with a registered ring-VRF key.
    async fn ring_vrf_sign(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<api::HostAccountRingVrfSignRequest>,
    ) -> RingVrfSignResponse {
        self.wallet
            .ring_vrf_sign(
                cx.account_invocation(Some(&request.calling_product_id)),
                request.payload,
            )
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
