//! SSO statement-store channel to the paired remote signing host.

use crate::runtime::HostSession;

use super::super::authority::{
    AuthorityCancelError, AuthorityError, BulletinAllowanceKey, CreateTransactionAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
};
use super::super::sso_remote::{
    SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SsoRemoteResponseError, SsoSessionKey,
    sso_message_id,
};
use super::PairingHost;
use crate::host_internal::sso_messages::{
    CreateTransactionLegacyPayload, CreateTransactionPayload, CreateTransactionRequest,
    CreateTransactionWithLegacyAccountRequest, OnExistingAllowancePolicy, ProductRequest,
    ProductSubtreeRequest, ResourceAllocationRequest, RingVrfError,
    SignRawWithLegacyAccountRequest, SignRequest, SsoAllocatedResource, SsoAllocationOutcome,
    SsoProductTxPayload,
};
use crate::host_logic::session::SessionInfo;

use tracing::{instrument, warn};
use truapi::{CallContext, latest};

impl PairingHost {
    /// Mirror a local registration to the paired account holder.
    pub fn mirror_ring_vrf_registration(
        &self,
        session: SessionInfo,
        request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
    ) {
        let weak_self = std::sync::Arc::downgrade(&self.sso);
        (self.services.spawner)(Box::pin(async move {
            let Some(host) = weak_self.upgrade() else {
                return;
            };
            let cx = CallContext::with_request_id(format!(
                "ring-vrf-registration-mirror:{}",
                sso_message_id()
            ));
            if let Err(error) = host
                .call(&cx, &session, request)
                .await
                .map_err(ring_vrf_transport_error)
                .and_then(core::convert::identity)
            {
                warn!(?error, "ring-VRF registration mirror failed");
            }
        }));
    }

    /// Resolve a product's hard-subtree public key, asking the Account Holder
    /// only when neither the memory cache nor storage already holds it.
    pub async fn remote_product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
        let lifecycle_epoch = self.grants.lifecycle().revision();
        let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
        if let Some(public_key) = self
            .grants
            .known_product_subtree(&self.sso.session_state(), session, cache_key.clone())
            .await
        {
            return Ok(public_key);
        }
        let public_key = self.sso
            .call(cx, session, ProductSubtreeRequest { product_id })
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)?;
        if !self
            .grants
            .persist_product_subtree_if_current(
                &self.sso.session_state(),
                session,
                lifecycle_epoch,
                cache_key,
                public_key,
            )
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
        calling_product_id: String,
        request: latest::HostAccountSignVrfRequest,
    ) -> Result<latest::VrfSignature, AuthorityError> {
        self.sso.call(
            cx,
            session,
            ProductRequest {
                calling_product_id,
                payload: request,
            },
        )
        .await
        .map_err(remote_authority_error)?
        .map_err(|err| match err {
            latest::HostAccountSignVrfError::NotConnected => AuthorityError::Disconnected,
            latest::HostAccountSignVrfError::Rejected => AuthorityError::Rejected,
            latest::HostAccountSignVrfError::Unknown { reason } => {
                AuthorityError::Unknown { reason }
            }
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
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => latest::HostSignPayloadRequest {
                account: product_account,
                payload: request.payload,
            },
        };
        self.sso
            .call(cx, session, SignRequest::Payload(Box::new(request)))
            .await
            .map_err(remote_authority_error)?
            .map_err(remote_authority_error)
    }

    /// Forward a raw-signing request to the paired signing host.
    #[instrument(skip_all, fields(account_kind = match &request {
        SignRawAuthorityRequest::Product(_) => "product",
        SignRawAuthorityRequest::LegacyAccount { .. } | SignRawAuthorityRequest::IdentityAccount { .. } => "legacy",
    }))]
    pub async fn remote_sign_raw(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
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
                    return self.sso
                        .call(
                            cx,
                            session,
                            SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(request),
                        )
                        .await
                        .map_err(remote_authority_error)?
                        .map_err(remote_authority_error);
                }
                let signature = self.sso
                    .call(cx, session, request)
                    .await
                    .map_err(remote_authority_error)?
                    .map_err(remote_authority_error)?;
                return Ok(latest::HostSignPayloadResponse {
                    signature,
                    signed_transaction: None,
                });
            }
        };
        self.sso.call(
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
        .map_err(remote_authority_error)
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
                self.sso.call(
                    cx,
                    session,
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
                self.sso.call(
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
            }
            CreateTransactionAuthorityRequest::IdentityAccount(payload) => {
                self.sso.call(
                    cx,
                    session,
                    CreateTransactionWithLegacyAccountRequest {
                        payload: CreateTransactionLegacyPayload::V1(payload),
                    },
                )
                .await
            }
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
        self.sso
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
        self.sso
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
        self.sso
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
        self.sso
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
        self.sso
            .call(cx, session, request)
            .await
            .map_err(ring_vrf_transport_error)?
    }

    /// Return allocation outcomes from the paired signing host without retaining keys.
    pub async fn remote_allocate_resources(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: String,
        request: latest::HostRequestResourceAllocationRequest,
    ) -> Result<Vec<SsoAllocationOutcome>, AuthorityError> {
        self.sso.call(
            cx,
            session,
            ResourceAllocationRequest {
                calling_product_id: product_id,
                resources: request.resources,
                on_existing: OnExistingAllowancePolicy::Increase,
            },
        )
        .await
        .map_err(remote_authority_error)?
        .map_err(remote_authority_error)
    }

    /// Allocate exactly one allowance for the product and return its material.
    async fn remote_allowance_slot(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: &str,
        resource: latest::AllocatableResource,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<SsoAllocatedResource, AuthorityError> {
        let name = allowance_name(&resource);
        let outcomes = self.sso
            .call(
                cx,
                session,
                ResourceAllocationRequest {
                    calling_product_id: product_id.to_string(),
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
            Some(SsoAllocationOutcome::NotAvailable) => Err(AuthorityError::Unavailable {
                reason: format!("{name} is not available"),
            }),
            None => Err(AuthorityError::Unknown {
                reason: format!("Empty {name} response"),
            }),
        }
    }

    /// Statement-store allowance key for the product, served from the cache
    /// or allocated by the paired signing host.
    pub async fn remote_statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        if let Some(cached) = self
            .grants
            .cached_statement_store_allowance_key(
                &self.sso.session_state(),
                session,
                lifecycle_epoch,
                &product_id,
            )
            .await?
        {
            return Ok(cached);
        }
        match self
            .remote_allowance_slot(
                cx,
                session,
                &product_id,
                latest::AllocatableResource::StatementStoreAllowance,
                OnExistingAllowancePolicy::Ignore,
            )
            .await?
        {
            SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                self.grants.cache_statement_store_allowance_key(&self.sso.session_state(),
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            other => Err(unexpected_resource("statement-store allowance", &other)),
        }
    }

    /// Bulletin allowance key for the product, served from the cache or
    /// allocated by the paired signing host.
    pub async fn remote_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        if let Some(cached) = self
            .grants
            .cached_bulletin_allowance_key(
                &self.sso.session_state(),
                session,
                lifecycle_epoch,
                &product_id,
            )
            .await?
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
        lifecycle_epoch: u64,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        self.grants
            .evict_bulletin_allowance_key(
                &self.sso.session_state(),
                session,
                lifecycle_epoch,
                &product_id,
            )
            .await?;
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
        product_id: String,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        match self
            .remote_allowance_slot(
                cx,
                session,
                &product_id,
                latest::AllocatableResource::BulletinAllowance,
                on_existing,
            )
            .await?
        {
            SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                self.grants.cache_bulletin_allowance_key(&self.sso.session_state(),
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
            }
            other => Err(unexpected_resource("bulletin allowance", &other)),
        }
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

        assert_eq!(
            reason,
            "Unexpected statement-store allowance response resource: auto-signing"
        );
        assert!(!reason.contains("165, 165"));
    }
}
