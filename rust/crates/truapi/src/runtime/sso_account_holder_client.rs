//! Canonical account operations translated to the existing paired SSO protocol.

use super::HostSession;
use super::allowances::AllowanceResource;
use super::authority::{
    AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
    AuthorityCancelError, AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
    CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey, require_current_session,
};
use super::sso_remote::{
    SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SsoRemoteResponseError,
};
use super::sso_request_service::SsoRequestService;
use crate::host_internal::sso_messages::{
    CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
    CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, ProductRequest,
    ProductSubtreeRequest, ResourceAllocationRequest, RingVrfError,
    SignRawWithLegacyAccountRequest, SignRequest, SsoAllocatedResource, SsoAllocationOutcome,
    SsoProductTxPayload,
};
use crate::host_internal::sso_wire::SsoRequest;
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::session::SessionInfo;
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use std::sync::Arc;
use truapi::latest;

/// Account-holder requests over one runtime's selected SSO channel.
pub struct SsoAccountHolderClient {
    service: Arc<SsoRequestService>,
}

impl SsoAccountHolderClient {
    /// Use the runtime's existing session and outbound transport.
    pub fn new(service: Arc<SsoRequestService>) -> Self {
        Self { service }
    }

    async fn call<R: SsoRequest>(
        &self,
        invocation: &AccountInvocation<'_>,
        request: R,
    ) -> Result<R::Response, AuthorityError> {
        self.require_current_session(invocation.session)?;
        self.service.approve(invocation).await?;
        let session = self.require_current_session(invocation.session)?;
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.service
                .call(&cx, &session, request)
                .await
                .map_err(remote_authority_error)
        })
        .await
    }
}

#[async_trait::async_trait]
impl AccountHolder for SsoAccountHolderClient {
    fn current_session(&self) -> Option<AuthoritySession> {
        self.service.current_session()
    }

    fn require_current_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        require_current_session(&self.service.session_state(), session)
    }

    async fn product_subtree_public_key<'a>(
        &'a self,
        invocation: AccountInvocation<'a>,
        product_id: String,
    ) -> Result<futures::future::BoxFuture<'a, Result<[u8; 32], AuthorityError>>, AuthorityError>
    {
        self.require_current_session(invocation.session)?;
        self.service.approve(&invocation).await?;
        let session = self.require_current_session(invocation.session)?;
        Ok(Box::pin(async move {
            self.service
                .call(
                    invocation.call,
                    &session,
                    ProductSubtreeRequest { product_id },
                )
                .await
                .map_err(remote_authority_error)?
                .map_err(remote_authority_error)
        }))
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
                    .to_string(),
                payload: request,
            },
        )
        .await?
        .map_err(|error| match error {
            latest::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
            latest::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
            latest::HostAccountSignVrfError::Unknown { reason } => {
                AuthorityError::Unknown { reason }
            }
        })
    }

    async fn sign_payload(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let request = match request {
            SignPayloadAuthorityRequest::Product(request) => request,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => latest::HostSignPayloadRequest {
                account: product_account,
                payload: request.payload,
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
            SignRawAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => latest::HostSignRawRequest {
                account: product_account,
                payload: request.payload,
            },
            SignRawAuthorityRequest::IdentityAccount { account, request } => {
                let request = SignRawWithLegacyAccountRequest {
                    account,
                    data: request.payload,
                };
                if !watermarked {
                    return self
                        .call(
                            &invocation,
                            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request),
                        )
                        .await?
                        .map_err(remote_authority_error);
                }
                let signature = self
                    .call(&invocation, request)
                    .await?
                    .map_err(remote_authority_error)?;
                return Ok(latest::HostSignPayloadResponse {
                    signature,
                    signed_transaction: None,
                });
            }
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
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => {
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
            }
            CreateTransactionAuthorityRequest::IdentityAccount(payload) => {
                self.call(
                    &invocation,
                    CreateTransactionWithLegacyAccountRequest {
                        payload: CreateTransactionLegacyPayload::V1(payload),
                    },
                )
                .await
            }
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
        self.call(
            &invocation,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await?
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountCreateProofRequest,
    ) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
        self.call(
            &invocation,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await?
    }

    async fn register_ring_vrf_key(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<latest::HostAccountRegisterRingVrfKeyResponse, RingVrfError> {
        self.call(
            &invocation,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await?
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountListRingVrfKeysRequest,
    ) -> Result<latest::HostAccountListRingVrfKeysResponse, RingVrfError> {
        self.call(
            &invocation,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await?
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountRingVrfSignRequest,
    ) -> Result<latest::HostAccountRingVrfSignResponse, RingVrfError> {
        self.call(
            &invocation,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await?
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        _account: latest::ProductAccountId,
        _payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.require_current_session(invocation.session)?;
        self.service.approve(&invocation).await?;
        Err(AuthorityError::Unavailable { reason: "pairing host: exact statement proof signing needs an AutoSigning capability; the current SSO raw-signing protocol cannot carry it".to_string() })
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.require_current_session(session)?;
        if session.sso.is_none() {
            return Err(AuthorityError::Disconnected);
        }
        let source = session
            .root_entropy_source
            .ok_or_else(|| AuthorityError::Unavailable {
                reason: "Session secret missing".to_string(),
            })?;
        derive_product_entropy_from_source(&source, product_id, context).map_err(|error| {
            AuthorityError::Unknown {
                reason: error.to_string(),
            }
        })
    }

    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        let session = self.require_current_session(session)?;
        let source = session
            .root_entropy_source
            .ok_or_else(|| AuthorityError::Unavailable {
                reason: "Session secret missing".to_string(),
            })?;
        Ok(crate::runtime::contacts::handle_key_from_root_source(
            &source,
        ))
    }

    async fn allocate_grants<'a>(
        &'a self,
        invocation: AccountInvocation<'a>,
        request: latest::HostRequestResourceAllocationRequest,
        policy: OnExistingAllowancePolicy,
    ) -> Result<BoxStream<'a, Result<AccountGrantOutcome, AuthorityError>>, AuthorityError> {
        self.require_current_session(invocation.session)?;
        self.service.approve(&invocation).await?;
        let session = self.require_current_session(invocation.session)?;
        let request = ResourceAllocationRequest {
            calling_product_id: invocation
                .caller
                .product_id()
                .ok_or(AuthorityError::Rejected)?
                .to_string(),
            resources: request.resources,
            on_existing: policy,
        };
        Ok(stream::once(async move {
            self.service
                .call(invocation.call, &session, request)
                .await
                .map_err(remote_authority_error)?
                .map_err(remote_authority_error)
        })
        .flat_map(|result| {
            stream::iter(match result {
                Ok(outcomes) => outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        SsoAllocationOutcome::Allocated(resource) => {
                            decode_grant(resource).map(AccountGrantOutcome::Allocated)
                        }
                        SsoAllocationOutcome::Rejected => Ok(AccountGrantOutcome::Rejected),
                        SsoAllocationOutcome::NotAvailable => {
                            Ok(AccountGrantOutcome::NotAvailable { reason: None })
                        }
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
            }
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
                        .to_string(),
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
            }
            Some(SsoAllocationOutcome::Rejected) => Err(AuthorityError::Rejected),
            Some(SsoAllocationOutcome::NotAvailable) => Err(AuthorityError::Unavailable {
                reason: format!("{name} is not available"),
            }),
            None => Err(AuthorityError::Unknown {
                reason: format!("Empty {name} response"),
            }),
        }
    }
}

fn decode_grant(resource: SsoAllocatedResource) -> Result<AccountGrant, AuthorityError> {
    Ok(match resource {
        SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
            AccountGrant::StatementStore {
                key: StatementStoreAllowanceKey::from_secret_bytes(slot_account_key)?,
                period: None,
            }
        }
        SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
            AccountGrant::Bulletin(BulletinAllowanceKey::from_secret_bytes(slot_account_key)?)
        }
        SsoAllocatedResource::SmartContractAllowance => AccountGrant::SmartContract,
        SsoAllocatedResource::AutoSigning {
            product_root_private_key,
            ring_vrf_domain_entropy,
        } => AccountGrant::AutoSigning(AutoSigningKey::from_parts(
            product_root_private_key,
            ring_vrf_domain_entropy,
        )),
    })
}

fn remote_authority_error(reason: impl Into<SsoRemoteResponseError>) -> AuthorityError {
    match reason.into() {
        SsoRemoteResponseError::Cancelled(err) => AuthorityError::Cancelled(
            AuthorityCancelError::new(err.remote_message_id(), err.reason()),
        ),
        SsoRemoteResponseError::LocalDisconnected | SsoRemoteResponseError::PeerDisconnected => {
            AuthorityError::Disconnected
        }
        SsoRemoteResponseError::Failure(reason) => match reason.as_str() {
            "Rejected" | "User rejected" => AuthorityError::Rejected,
            SSO_LOCAL_DISCONNECT_REASON | SSO_PEER_DISCONNECT_REASON => {
                AuthorityError::Disconnected
            }
            _ => AuthorityError::Unknown { reason },
        },
    }
}

fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
    AuthorityError::Unknown {
        reason: format!("Unexpected {label} response resource: {}", resource.kind()),
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

        assert_eq!(
            reason,
            "Unexpected statement-store allowance response resource: auto-signing"
        );
        assert!(!reason.contains("165, 165"));
    }
}
