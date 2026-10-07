//! Product account policy and retained grants, independent of account-holder transport.

use super::allowances::{AllowanceResource, GrantScope};
use super::authority::{
    AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
    AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
    CreateTransactionAuthorityRequest, HostOperation, SignPayloadAuthorityRequest,
    SignRawAuthorityRequest, StatementStoreAllowanceKey,
};
use super::host_grants::{HostGrantGuard, HostGrantStore};
use super::ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing};
use super::services::RuntimeServices;
use super::signing_host::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};
use super::vrf;
use crate::host_internal::extrinsic::{
    Sr25519Signer, build_signed_transaction, local_transaction_metadata,
};
use crate::host_internal::sso_messages::{OnExistingAllowancePolicy, RingVrfError};
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair_from_subtree_secret,
    derive_ring_vrf_entropy_from_domain,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::platform::{
    ProductContext, SignVrfReview, UserConfirmationReview, normalize_product_identifier,
};
use futures::StreamExt;
use std::sync::Arc;
use truapi::{CallContext, latest as api};
use zeroize::Zeroizing;

/// Receives, retains and uses product grants from one selected account holder.
pub struct HostAccounts<H: AccountHolder> {
    services: Arc<RuntimeServices>,
    holder: Arc<H>,
    session_state: Arc<SessionState>,
    grants: Arc<HostGrantStore>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    #[cfg(feature = "test-host")]
    resource_controls: Arc<super::test_resource_controls::TestResourceControls>,
}

impl<H: AccountHolder> HostAccounts<H> {
    /// Compose one host's policy with its canonical session and grant owners.
    pub fn new(
        services: Arc<RuntimeServices>,
        holder: Arc<H>,
        session_state: Arc<SessionState>,
        grants: Arc<HostGrantStore>,
        ring_vrf_registry: Arc<RingVrfRegistryStore>,
        #[cfg(feature = "test-host")] resource_controls: Arc<
            super::test_resource_controls::TestResourceControls,
        >,
    ) -> Arc<Self> {
        Arc::new(Self {
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            services,
            holder,
            session_state,
            grants,
            ring_vrf_registry,
            #[cfg(feature = "test-host")]
            resource_controls,
        })
    }

    /// Selected account holder identity.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        self.holder.current_session()
    }

    /// Capture both account identity and host grants before approval.
    pub fn current_operation(&self) -> Option<HostOperation> {
        let lifecycle = self.grants.lifecycle();
        self.current_session()
            .map(|session| lifecycle.capture(session))
    }

    fn hold_operation(
        &self,
        operation: &HostOperation,
    ) -> Result<HostGrantGuard<'_>, AuthorityError> {
        let lifecycle = self.grants.lifecycle();
        lifecycle.require(operation)?;
        self.holder.require_current_session(&operation.session)?;
        Ok(lifecycle)
    }

    fn operation_session(
        &self,
        operation: &HostOperation,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let lifecycle = self.grants.lifecycle();
        lifecycle.require(operation)?;
        let session = self.holder.require_current_session(&operation.session)?;
        Ok((session, lifecycle.revision()))
    }

    /// Reject work invalidated by account selection or a host grant reset.
    pub fn require_current_operation(
        &self,
        operation: &HostOperation,
    ) -> Result<(), AuthorityError> {
        self.hold_operation(operation).map(|_| ())
    }

    /// Wallet permission retained for this product and activation.
    pub fn wallet_authorization(
        &self,
        operation: &HostOperation,
        product: &ProductContext,
    ) -> Result<Option<super::WalletAuthorization>, AuthorityError> {
        self.hold_operation(operation)?
            .wallet_authorization(operation, &product.product_id)
    }

    /// Resolve a retained subtree before asking the holder to review and resolve it.
    pub async fn product_subtree_public_key(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let (session, revision) = self.operation_session(operation)?;
        let cache_key = (GrantScope::from_session(&session), product_id.clone());
        if let Some(key) = self
            .grants
            .known_product_subtree(&self.session_state, &session, cache_key.clone())
            .await
        {
            self.require_current_operation(operation)?;
            return Ok(key);
        }
        let resolution = self
            .holder
            .product_subtree_public_key(invocation, product_id)
            .await?;
        let cx = super::remote_authority_context(invocation.call);
        super::remote_authority_call(
            &cx,
            operation.run(self, async {
                let key = resolution.await?;
                self.require_current_operation(operation)?;
                if !self
                    .grants
                    .persist_product_subtree_if_current(
                        &self.session_state,
                        &session,
                        revision,
                        cache_key,
                        key,
                    )
                    .await
                {
                    return Err(AuthorityError::Disconnected);
                }
                Ok(key)
            }),
        )
        .await
    }

    /// Review one allocation, then retain each issued capability under its original operation.
    pub async fn allocate_resources(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product: &ProductContext,
        request: api::HostRequestResourceAllocationRequest,
    ) -> Result<api::HostRequestResourceAllocationResponse, AuthorityError> {
        self.require_current_operation(operation)?;
        let review =
            UserConfirmationReview::ResourceAllocation(crate::platform::ResourceAllocationReview {
                calling_product_id: product.product_id.clone(),
                resources: request.resources.clone(),
            });
        #[cfg(feature = "test-host")]
        let resources = request.resources.clone();
        let mut grants = self
            .holder
            .allocate_grants(
                AccountInvocation {
                    call: cx,
                    session: &operation.session,
                    caller: AccountCaller::Local {
                        product,
                        authorization: None,
                        outbound_review: Some(&review),
                    },
                },
                request,
                OnExistingAllowancePolicy::Increase,
            )
            .await
            .map_err(|error| match error {
                AuthorityError::Rejected => AuthorityError::Unknown {
                    reason: "User rejected resource allocation".to_string(),
                },
                other => other,
            })?;
        let cx = super::remote_authority_context_with_default(
            cx,
            super::RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
        );
        super::remote_authority_call(&cx, operation.run(self, async {
            #[cfg(feature = "test-host")]
            if self.resource_controls.grants_allowances_unchecked() {
                drop(grants);
                return Ok(api::HostRequestResourceAllocationResponse { outcomes: resources.iter().map(|resource| if self.resource_controls.withholds(resource) { api::AllocationOutcome::Rejected } else { api::AllocationOutcome::Allocated }).collect() });
            }
            let mut outcomes = Vec::new();
            loop {
                self.require_current_operation(operation)?;
                let Some(outcome) = grants.next().await else { break; };
                self.require_current_operation(operation)?;
                outcomes.push(match outcome? {
                    AccountGrantOutcome::Allocated(grant) => {
                        self.retain_grant(&cx, operation, product, grant).await?;
                        api::AllocationOutcome::Allocated
                    },
                    AccountGrantOutcome::Rejected => api::AllocationOutcome::Rejected,
                    AccountGrantOutcome::NotAvailable { reason } => {
                        if let Some(reason) = reason { tracing::warn!(product_id = %product.product_id, %reason, "resource allocation item failed"); }
                        api::AllocationOutcome::NotAvailable
                    },
                });
            }
            Ok(api::HostRequestResourceAllocationResponse { outcomes })
        })).await
    }

    async fn retain_grant(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product: &ProductContext,
        grant: AccountGrant,
    ) -> Result<(), AuthorityError> {
        let (session, revision) = self.operation_session(operation)?;
        match grant {
            AccountGrant::StatementStore { key, period } => {
                self.grants
                    .cache_statement_store_allowance_key(
                        &self.session_state,
                        &session,
                        revision,
                        &product.product_id,
                        key,
                        period,
                    )
                    .await?;
            }
            AccountGrant::Bulletin(key) => {
                self.grants
                    .cache_bulletin_allowance_key(
                        &self.session_state,
                        &session,
                        revision,
                        &product.product_id,
                        key,
                    )
                    .await?;
            }
            AccountGrant::WalletAuthorization(authorization) => self
                .hold_operation(operation)?
                .retain_wallet_authorization(
                    &self.session_state,
                    operation,
                    &product.product_id,
                    authorization,
                )?,
            AccountGrant::AutoSigning(key) => {
                let expected = self
                    .product_subtree_public_key(
                        operation,
                        cx,
                        AccountCaller::Local {
                            product,
                            authorization: None,
                            outbound_review: None,
                        },
                        product.product_id.clone(),
                    )
                    .await?;
                self.grants
                    .remember_auto_signing_key(
                        &self.session_state,
                        &session,
                        revision,
                        &product.product_id,
                        expected,
                        key,
                    )
                    .await?;
            }
            AccountGrant::SmartContract => {}
        }
        self.require_current_operation(operation)
    }

    /// Acquire a statement key, renewing only grants whose actual allocation period is known.
    pub async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let (session, revision) = self.operation_session(operation)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::StatementStoreAllowance)?;
        if let Some((period, key)) = self
            .grants
            .cached_statement_store_allowance_key(
                &self.session_state,
                &session,
                revision,
                &product_id,
            )
            .await?
        {
            let current = period
                .map(|_| {
                    super::allowances::current_unix_secs()
                        .map(super::statement_allowance::slot::current_period)
                })
                .transpose()?;
            if period.is_none() || period == current {
                self.require_current_operation(operation)?;
                return Ok(key);
            }
        }
        let product = ProductContext::new(product_id.clone()).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let grant = operation
            .run(
                self,
                self.holder.ensure_allowance(
                    AccountInvocation {
                        call: cx,
                        session: &operation.session,
                        caller: AccountCaller::Local {
                            product: &product,
                            authorization: None,
                            outbound_review: None,
                        },
                    },
                    AllowanceResource::StatementStore,
                    OnExistingAllowancePolicy::Ignore,
                ),
            )
            .await?;
        match grant {
            AccountGrant::StatementStore { key, period } => {
                self.require_current_operation(operation)?;
                self.grants
                    .cache_statement_store_allowance_key(
                        &self.session_state,
                        &session,
                        revision,
                        &product_id,
                        key,
                        period,
                    )
                    .await
            }
            _ => Err(AuthorityError::Unknown {
                reason: "Unexpected statement-store allowance response resource".to_string(),
            }),
        }
    }

    /// Forget only the dated statement grant whose key was rejected.
    pub fn forget_statement_store_allowance_key(&self, product_id: &str, public_key: [u8; 32]) {
        self.grants
            .lifecycle()
            .forget_statement_store_allowance(product_id, public_key);
    }

    /// Reuse a retained Bulletin key without contacting its issuer.
    pub async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, revision) = self.operation_session(operation)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::BulletinAllowance)?;
        if let Some(key) = self
            .grants
            .cached_bulletin_allowance_key(&self.session_state, &session, revision, &product_id)
            .await?
        {
            self.require_current_operation(operation)?;
            return Ok(key);
        }
        self.allocate_bulletin_allowance_key(
            cx,
            operation,
            product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
    }

    /// Refresh after the chain rejects a missing or exhausted Bulletin allowance.
    pub async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, revision) = self.operation_session(operation)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::BulletinAllowance)?;
        self.grants
            .evict_bulletin_allowance_key(&self.session_state, &session, revision, &product_id)
            .await?;
        self.allocate_bulletin_allowance_key(
            cx,
            operation,
            product_id,
            OnExistingAllowancePolicy::Increase,
        )
        .await
    }

    async fn allocate_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
        policy: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, revision) = self.operation_session(operation)?;
        let product = ProductContext::new(product_id.clone()).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let grant = operation
            .run(
                self,
                self.holder.ensure_allowance(
                    AccountInvocation {
                        call: cx,
                        session: &operation.session,
                        caller: AccountCaller::Local {
                            product: &product,
                            authorization: None,
                            outbound_review: None,
                        },
                    },
                    AllowanceResource::Bulletin,
                    policy,
                ),
            )
            .await?;
        match grant {
            AccountGrant::Bulletin(key) => {
                self.require_current_operation(operation)?;
                self.grants
                    .cache_bulletin_allowance_key(
                        &self.session_state,
                        &session,
                        revision,
                        &product_id,
                        key,
                    )
                    .await
            }
            _ => Err(AuthorityError::Unknown {
                reason: "Unexpected bulletin allowance response resource".to_string(),
            }),
        }
    }

    /// Derive entropy while the original host selection remains locked.
    pub fn derive_entropy(
        &self,
        operation: &HostOperation,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        let _lifecycle = self.hold_operation(operation)?;
        self.holder
            .derive_entropy(&operation.session, product_id, context)
    }

    /// Mint contact handles under the selected account holder.
    pub fn contacts_handle_key(
        &self,
        operation: &HostOperation,
    ) -> Result<[u8; 32], AuthorityError> {
        let _lifecycle = self.hold_operation(operation)?;
        self.holder.contacts_handle_key(&operation.session)
    }

    /// Retained grants exercised by lifecycle tests.
    #[cfg(test)]
    pub fn grants_for_tests(&self) -> &HostGrantStore {
        &self.grants
    }

    /// Seed a subtree without an account-holder exchange.
    #[cfg(test)]
    pub fn cache_product_subtree_for_test(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.grants
            .cache_product_subtree_for_test(session, product_id, public_key);
    }
}

impl<H: AccountHolder> HostAccounts<H> {
    async fn product_signing_grant(
        &self,
        operation: &HostOperation,
        caller: AccountCaller<'_>,
        account: &api::ProductAccountId,
    ) -> Result<Option<AutoSigningKey>, AuthorityError> {
        if !matches!(caller, AccountCaller::Local { product, .. } if product.product_id == account.dot_ns_identifier)
        {
            return Ok(None);
        }
        let (session, _) = self.operation_session(operation)?;
        let grant = self
            .grants
            .auto_signing_key(&session, &account.dot_ns_identifier)
            .await?;
        self.require_current_operation(operation)?;
        Ok(grant)
    }

    fn grant_keypair(
        grant: &AutoSigningKey,
        account: &api::ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        derive_product_keypair_from_subtree_secret(
            *grant.as_secret_bytes(),
            derivation_index_bytes(&account.derivation_index),
        )
        .map_err(|error| AuthorityError::Unknown {
            reason: error.to_string(),
        })
    }

    /// Sign with a retained product key or obtain the holder's independent consent.
    pub async fn sign_payload(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        if let SignPayloadAuthorityRequest::Product(payload) = &request
            && let Some(grant) = self
                .product_signing_grant(operation, invocation.caller, &payload.account)
                .await?
        {
            let cx = super::remote_authority_context(invocation.call);
            return super::remote_authority_call(&cx, async {
                let _lifecycle = self.hold_operation(operation)?;
                let keypair = Self::grant_keypair(&grant, &payload.account)?;
                Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?)
            })
            .await;
        }
        let review = request.review(invocation.caller);
        operation
            .run(
                self,
                self.holder
                    .sign_payload(invocation.with_outbound_review(&review), request),
            )
            .await
    }

    /// Preserve legacy review even when a retained product key can sign locally.
    pub async fn sign_raw(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let account = match &request {
            SignRawAuthorityRequest::Product(payload) => Some(&payload.account),
            SignRawAuthorityRequest::LegacyAccount {
                product_account, ..
            } => Some(product_account),
            SignRawAuthorityRequest::IdentityAccount { .. } => None,
        };
        let grant = if watermarked && let Some(account) = account {
            self.product_signing_grant(operation, invocation.caller, account)
                .await
        } else {
            Ok(None)
        };
        let review = request.review(invocation.caller, watermarked);
        if !matches!(request, SignRawAuthorityRequest::Product(_)) && !matches!(&grant, Ok(None)) {
            self.require_current_operation(operation)?;
            invocation
                .confirm(self.services.platform.as_ref(), review.clone())
                .await?;
        }
        if let (Some(grant), Some(account)) = (grant?, account) {
            let payload = match &request {
                SignRawAuthorityRequest::Product(request) => &request.payload,
                SignRawAuthorityRequest::LegacyAccount { request, .. }
                | SignRawAuthorityRequest::IdentityAccount { request, .. } => &request.payload,
            };
            let message = raw_payload_bytes(payload.clone(), watermarked)?;
            let cx = super::remote_authority_context(invocation.call);
            return super::remote_authority_call(&cx, async {
                let _lifecycle = self.hold_operation(operation)?;
                let keypair = Self::grant_keypair(&grant, account)?;
                Ok(api::HostSignPayloadResponse {
                    signature: keypair
                        .secret
                        .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
                        .to_bytes()
                        .to_vec(),
                    signed_transaction: None,
                })
            })
            .await;
        }
        operation
            .run(
                self,
                self.holder.sign_raw(
                    invocation.with_outbound_review(&review),
                    request,
                    watermarked,
                ),
            )
            .await
    }

    /// Prepare chain metadata before using a retained key under its operation guard.
    pub async fn create_transaction(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let review = request.review(invocation.caller);
        if let CreateTransactionAuthorityRequest::Product(payload) = &request
            && let Some(grant) = self
                .product_signing_grant(operation, invocation.caller, &payload.signer)
                .await?
        {
            if !payload.contacts.is_empty() {
                invocation
                    .confirm(self.services.platform.as_ref(), review)
                    .await?;
            }
            let cx = super::remote_authority_context(invocation.call);
            return super::remote_authority_call(
                &cx,
                operation.run(self, async {
                    let metadata =
                        local_transaction_metadata(&self.services.chain, payload.genesis_hash)
                            .await?;
                    let _lifecycle = self.hold_operation(operation)?;
                    let keypair = Self::grant_keypair(&grant, &payload.signer)?;
                    Ok(api::HostCreateTransactionResponse {
                        transaction: build_signed_transaction(
                            &Sr25519Signer::from_keypair(&keypair),
                            payload.genesis_hash,
                            &payload.call_data,
                            &payload.extensions,
                            payload.tx_ext_version,
                            metadata,
                        )?,
                    })
                }),
            )
            .await;
        }
        operation
            .run(
                self,
                self.holder
                    .create_transaction(invocation.with_outbound_review(&review), request),
            )
            .await
    }

    /// Keep retained VRF signing restricted to this host's bound product caller.
    pub async fn sign_vrf(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: api::HostAccountSignVrfRequest,
    ) -> Result<api::VrfSignature, AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        if let Some(grant) = self
            .product_signing_grant(operation, invocation.caller, &request.account)
            .await?
        {
            let _lifecycle = self.hold_operation(operation)?;
            let keypair = Self::grant_keypair(&grant, &request.account)?;
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            return Ok(api::VrfSignature { pre_output, proof });
        }
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(AuthorityError::Rejected)?;
        let review = UserConfirmationReview::SignVrf(SignVrfReview {
            calling_product_id: calling_product_id.to_string(),
            request: request.clone(),
        });
        let invocation = if super::authority::is_blessed_owner(
            calling_product_id,
            &request.account.dot_ns_identifier,
        ) {
            invocation
        } else {
            invocation.with_outbound_review(&review)
        };
        operation
            .run(self, self.holder.sign_vrf(invocation, request))
            .await
    }

    /// Sign exact proof bytes, without using a raw-signing wire message as a substitute.
    pub async fn sign_statement_store_product_payload(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        account: api::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        if let Some(grant) = self
            .product_signing_grant(operation, invocation.caller, &account)
            .await?
        {
            let _lifecycle = self.hold_operation(operation)?;
            let keypair = Self::grant_keypair(&grant, &account)?;
            return Ok(keypair
                .secret
                .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
                .to_bytes());
        }
        let review = UserConfirmationReview::StatementStoreProductSign(
            crate::platform::StatementStoreProductSignReview {
                calling_product_id: invocation.caller.product_id().map(str::to_string),
                account: account.clone(),
                payload: payload.clone(),
            },
        );
        let invocation = if matches!(invocation.caller, AccountCaller::Local { product, .. } if product.product_id == account.dot_ns_identifier)
        {
            invocation
        } else {
            invocation.with_outbound_review(&review)
        };
        operation
            .run(
                self,
                self.holder
                    .sign_statement_store_product_payload(invocation, account, payload),
            )
            .await
    }
}

impl<H: AccountHolder + 'static> HostAccounts<H> {
    /// Providers in the selected wallet's registry.
    pub async fn ring_vrf_providers(
        &self,
        ring: &api::RingLocation,
    ) -> Result<Vec<api::ProductAccountId>, RingVrfError> {
        let operation = self
            .current_operation()
            .ok_or(AuthorityError::Disconnected)?;
        let entries = self
            .ring_vrf_registry
            .providers(operation.session.public_key, ring)
            .await?;
        self.require_current_operation(&operation)?;
        Ok(entries)
    }

    /// Selected provider in the same canonical registry used by the wallet.
    pub async fn selected_ring_vrf_provider(
        &self,
        ring: &api::RingLocation,
    ) -> Result<Option<api::ProductAccountId>, RingVrfError> {
        let operation = self
            .current_operation()
            .ok_or(AuthorityError::Disconnected)?;
        let provider = self
            .ring_vrf_registry
            .selected_provider(operation.session.public_key, ring)
            .await?;
        self.require_current_operation(&operation)?;
        Ok(provider)
    }

    /// Prepare persistence before checking the originally selected account.
    pub async fn select_ring_vrf_provider(
        &self,
        ring: api::RingLocation,
        handle: api::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let operation = self
            .current_operation()
            .ok_or(AuthorityError::Disconnected)?;
        operation
            .run(self, async {
                let mut update = self
                    .ring_vrf_registry
                    .prepare_update(operation.session.public_key)
                    .await?;
                {
                    let _lifecycle = self.hold_operation(&operation)?;
                    update.select_provider(ring, handle)?;
                }
                update.persist().await
            })
            .await
    }

    /// Register fixture material in the actual durable registry.
    #[cfg(test)]
    pub async fn register_ring_vrf_key_for_tests(
        &self,
        session: &SessionInfo,
        handle: api::ProductAccountId,
        ring: api::RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        self.ring_vrf_registry
            .register(session.public_key, handle, ring, public_key)
            .await
    }

    async fn require_ring_vrf_key_access(
        &self,
        caller: AccountCaller<'_>,
        handle: &api::ProductAccountId,
    ) -> Result<
        (
            api::ProductAccountId,
            super::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = super::product_manifest::ring_vrf_key_access_granted(
            &self.services,
            self.services.platform.as_ref(),
            caller,
            handle,
        )
        .await?;
        Ok((
            api::ProductAccountId {
                dot_ns_identifier: access.owner.clone(),
                derivation_index: handle.derivation_index.clone(),
            },
            access,
        ))
    }

    async fn local_ring_vrf_entropy(
        &self,
        operation: &HostOperation,
        caller: AccountCaller<'_>,
        handle: &api::ProductAccountId,
        ring: Option<&api::RingLocation>,
    ) -> Result<Option<(Zeroizing<[u8; 32]>, vrf::Vrf)>, RingVrfError> {
        if !matches!(caller, AccountCaller::Local { .. }) {
            return Ok(None);
        }
        let (session, _) = self.operation_session(operation)?;
        let Some(grant) = self
            .grants
            .auto_signing_key(&session, &handle.dot_ns_identifier)
            .await?
        else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if ring.is_some_and(|ring| !entry.rings.contains(ring)) {
            return Err(RingVrfError::KeyNotInRing);
        }
        let vrf = vrf::load().await?;
        let _lifecycle = self.hold_operation(operation)?;
        let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
            grant.ring_vrf_domain_entropy(),
            &handle.derivation_index,
        ));
        if entry.public_key != Some(vrf.member(&entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the AutoSigning capability"
                    .to_string(),
            });
        }
        Ok(Some((entropy, vrf)))
    }

    /// Own aliases can use retained material after validating the registered ring.
    pub async fn account_alias(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: api::HostAccountGetAliasRequest,
    ) -> Result<api::ContextualAlias, RingVrfError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        if invocation.caller.product_id() == Some(&request.key_handle.dot_ns_identifier)
            && let Some((entropy, vrf)) = self
                .local_ring_vrf_entropy(
                    operation,
                    invocation.caller,
                    &request.key_handle,
                    Some(&request.ring_location),
                )
                .await?
        {
            self.ring_resolver.validate(&request.ring_location).await?;
            let _lifecycle = self.hold_operation(operation)?;
            let context = development_context_bytes(&request.context);
            return Ok(api::ContextualAlias {
                alias: vrf.alias(&entropy, &context)?.to_vec(),
                context,
            });
        }
        operation
            .run(self, self.holder.account_alias(invocation, request))
            .await
    }

    /// Resolve the ring before the final guarded proof computation.
    pub async fn create_proof(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: api::HostAccountCreateProofRequest,
    ) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let (handle, access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        super::product_manifest::require_own_context(&access, &request.context)?;
        if let Some((entropy, vrf)) = self
            .local_ring_vrf_entropy(
                operation,
                invocation.caller,
                &handle,
                Some(&request.ring_location),
            )
            .await?
        {
            let member = {
                let _lifecycle = self.hold_operation(operation)?;
                vrf.member(&entropy)?
            };
            let resolved = self
                .ring_resolver
                .resolve(&request.ring_location, &[MemberCandidate { member }])
                .await?;
            let _lifecycle = self.hold_operation(operation)?;
            let context = development_context_bytes(&request.context);
            let (proof, alias) =
                create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
            return Ok(api::HostAccountCreateProofResponse {
                proof,
                contextual_alias: api::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            });
        }
        operation
            .run(self, self.holder.create_proof(invocation, request))
            .await
    }

    /// Retain and mirror registrations under the account selected before any awaits.
    pub async fn register_ring_vrf_key(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: api::HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<[u8; 32], RingVrfError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let caller = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let handle = api::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(caller).map_err(|error| {
                RingVrfError::Unknown {
                    reason: error.to_string(),
                }
            })?,
            derivation_index: request.index.clone(),
        };
        let (session, _) = self.operation_session(operation)?;
        let grant = if matches!(invocation.caller, AccountCaller::Local { .. }) {
            self.grants
                .auto_signing_key(&session, &handle.dot_ns_identifier)
                .await?
        } else {
            None
        };
        let public_key = if let Some(grant) = &grant {
            self.ring_resolver.validate(&request.ring).await?;
            let vrf = vrf::load().await?;
            let _lifecycle = self.hold_operation(operation)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                grant.ring_vrf_domain_entropy(),
                &request.index,
            ));
            vrf.member(&entropy)?
        } else {
            operation
                .run(
                    self,
                    self.holder
                        .register_ring_vrf_key(invocation, request.clone()),
                )
                .await?
        };
        operation
            .run(self, async {
                let mut update = self
                    .ring_vrf_registry
                    .prepare_update(session.public_key)
                    .await?;
                {
                    let _lifecycle = self.hold_operation(operation)?;
                    update.register(handle, request.ring.clone(), public_key)?;
                }
                update.persist().await
            })
            .await?;
        if grant.is_some()
            && let AccountCaller::Local { product, .. } = invocation.caller
        {
            let holder = Arc::downgrade(&self.holder);
            let grants = Arc::downgrade(&self.grants);
            let operation = operation.clone();
            let product = product.clone();
            (self.services.spawner)(Box::pin(async move {
                let (Some(holder), Some(grants)) = (holder.upgrade(), grants.upgrade()) else {
                    return;
                };
                let result = async {
                    {
                        let lifecycle = grants.lifecycle();
                        lifecycle.require(&operation)?;
                        holder.require_current_session(&operation.session)?;
                    }
                    let cx = CallContext::with_request_id(format!(
                        "ring-vrf-registration-mirror:{}",
                        super::sso_remote::sso_message_id()
                    ));
                    holder
                        .register_ring_vrf_key(
                            AccountInvocation {
                                call: &cx,
                                session: &operation.session,
                                caller: AccountCaller::Local {
                                    product: &product,
                                    authorization: None,
                                    outbound_review: None,
                                },
                            },
                            request,
                        )
                        .await
                }
                .await;
                if let Err(error) = result {
                    tracing::warn!(?error, "ring-VRF registration mirror failed");
                }
            }));
        }
        Ok(public_key)
    }

    /// Merge complete owner listings without discarding accepted local registrations.
    pub async fn list_ring_vrf_keys(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        mut request: api::HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let owner = normalize_product_identifier(&request.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        let own = matches!(invocation.caller, AccountCaller::Local { product, .. } if product.product_id == owner);
        if own
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(operation.session.public_key, &owner)
                .await?
        {
            self.require_current_operation(operation)?;
            apply_ring_vrf_disclosure(&mut entries, request.disclosure);
            return Ok(entries);
        }
        let disclosure = request.disclosure;
        if own {
            request.disclosure = api::RingVrfKeyDisclosure::PublicKey;
        }
        let mut entries = operation
            .run(self, self.holder.list_ring_vrf_keys(invocation, request))
            .await?;
        validate_owner_listing(&owner, &entries)?;
        if entries.iter().all(|entry| entry.public_key.is_some()) {
            entries = operation
                .run(self, async {
                    let mut update = self
                        .ring_vrf_registry
                        .prepare_update(operation.session.public_key)
                        .await?;
                    let entries = {
                        let _lifecycle = self.hold_operation(operation)?;
                        update.reconcile_owner(&owner, entries)?
                    };
                    update.persist().await?;
                    Ok::<_, RingVrfError>(entries)
                })
                .await?;
        }
        self.require_current_operation(operation)?;
        apply_ring_vrf_disclosure(&mut entries, disclosure);
        Ok(entries)
    }

    /// Sign registered-key bytes only while the retained capability remains selected.
    pub async fn ring_vrf_sign(
        &self,
        operation: &HostOperation,
        cx: &CallContext,
        caller: AccountCaller<'_>,
        request: api::HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        self.require_current_operation(operation)?;
        let invocation = AccountInvocation {
            call: cx,
            session: &operation.session,
            caller,
        };
        let (handle, _) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        if let Some((entropy, vrf)) = self
            .local_ring_vrf_entropy(operation, invocation.caller, &handle, None)
            .await?
        {
            let _lifecycle = self.hold_operation(operation)?;
            return vrf.sign(&entropy, &request.message);
        }
        operation
            .run(self, self.holder.ring_vrf_sign(invocation, request))
            .await
    }
}

fn apply_ring_vrf_disclosure(
    entries: &mut [api::RegisteredRingVrfKey],
    disclosure: api::RingVrfKeyDisclosure,
) {
    if disclosure == api::RingVrfKeyDisclosure::Anonymized {
        for entry in entries {
            entry.public_key = None;
        }
    }
}

impl<H: AccountHolder> HostAccounts<H> {
    /// Prepare public transaction data before using the retained secret under its original guard.
    pub async fn build_bulletin_transaction<
        C: subxt::client::OnlineClientAtBlockT<subxt::config::substrate::SubstrateConfig>,
    >(
        &self,
        operation: &HostOperation,
        allowance: &BulletinAllowanceKey,
        client: &subxt::client::ClientAtBlock<subxt::config::substrate::SubstrateConfig, C>,
        data: &[u8],
    ) -> Result<
        subxt::tx::SubmittableTransaction<subxt::config::substrate::SubstrateConfig, C>,
        super::bulletin_rpc::BulletinSubmitError,
    > {
        use super::bulletin_rpc::BulletinSubmitError;
        use crate::host_internal::bulletin::{
            MORTAL_PERIOD_BLOCKS, allowance_signer, store_transaction_payload,
        };
        use subxt::tx::Signer;
        let account_id = {
            let _lifecycle = self.hold_operation(operation)?;
            allowance_signer(allowance)
                .map_err(BulletinSubmitError::InvalidAllowanceKey)?
                .account_id()
        };
        let payload = store_transaction_payload(data);
        let params = subxt::config::DefaultExtrinsicParamsBuilder::<
            subxt::config::substrate::SubstrateConfig,
        >::new()
        .mortal(MORTAL_PERIOD_BLOCKS)
        .build();
        let mut prepared = client
            .tx()
            .create_signable(&payload, &account_id, params)
            .await
            .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
        let _lifecycle = self.hold_operation(operation)?;
        let signer =
            allowance_signer(allowance).map_err(BulletinSubmitError::InvalidAllowanceKey)?;
        let bytes = prepared
            .signer_payload()
            .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))?;
        prepared
            .sign_with_account_and_signature(&account_id, &signer.sign(&bytes))
            .map_err(|error| BulletinSubmitError::Subxt(Box::new(error.into())))
    }

    /// Acquire and sign a statement while its retained grant remains selected.
    pub async fn create_authorized_statement_proof(
        &self,
        cx: &CallContext,
        operation: &HostOperation,
        product_id: String,
        statement: api::Statement,
    ) -> Result<api::StatementProof, super::statement_store::StatementProofFailure> {
        let allowance = operation
            .run(
                self,
                self.statement_store_allowance_key(cx, operation, product_id),
            )
            .await
            .map_err(super::statement_store::statement_authority_failure)?;
        let _lifecycle = self
            .hold_operation(operation)
            .map_err(super::statement_store::statement_authority_failure)?;
        super::statement_store::create_statement_proof_with_key(statement, &allowance)
    }
}
