//! Product account policy and retained grants, independent of account-holder transport.

use super::allowances::{AllowanceResource, GrantScope};
use super::authority::{
    AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
    AuthorityError, AuthoritySession, AutoSigningKey, BulletinAllowanceKey,
    CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey,
};
use super::host_grants::{GrantBarrier, HostGrantGuard, HostGrantStore};
use super::product_consent::ProductConsent;
use super::ring_vrf_registry::{
    RingVrfRegistryStore, apply_ring_vrf_disclosure, validate_owner_listing,
};
use super::services::RuntimeServices;
use super::signing_host::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};
use super::sso_request_service::PairedSessionOwner;
use super::vrf;
use crate::host_internal::extrinsic::{
    Sr25519Signer, build_signed_transaction, local_transaction_metadata,
};
use crate::host_internal::sso_messages::{OnExistingAllowancePolicy, RingVrfError};
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair_from_subtree_secret,
    derive_product_public_key, derive_ring_vrf_entropy_from_domain,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::platform::{
    PermissionAuthorizationStatus, ProductContext, normalize_product_identifier,
};
use futures::StreamExt;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use truapi::{CallContext, CallError, latest as api};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use zeroize::Zeroizing;

/// Receives, retains and uses product grants from one selected account holder.
pub struct HostAccounts<H: AccountHolder> {
    services: Arc<RuntimeServices>,
    holder: Arc<H>,
    session_state: Arc<SessionState>,
    grants: Arc<HostGrantStore>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    consent: Arc<ProductConsent>,
    #[cfg(feature = "test-host")]
    resource_controls: Arc<super::test_resource_controls::TestResourceControls>,
    #[cfg(feature = "test-host")]
    submit_preimages_locally: core::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl<H: AccountHolder> PairedSessionOwner for HostAccounts<H> {
    async fn write_barrier(&self) -> GrantBarrier {
        self.grants.barrier().await
    }

    fn session_ended(&self, previous: Option<&SessionInfo>, revoked: bool) {
        self.grants.session_ended(previous, revoked);
        self.consent.forget_allowed_once();
    }

    fn begin_cleanup(&self, barrier: &GrantBarrier) -> bool {
        self.grants.persistence_under(barrier).begin_cleanup()
    }

    async fn finish_cleanup(&self, barrier: &GrantBarrier) -> Result<(), String> {
        self.grants.persistence_under(barrier).drain_cleanup().await
    }

    async fn prepare_session(
        &self,
        barrier: &GrantBarrier,
        previous: Option<&SessionInfo>,
        next: &SessionInfo,
    ) {
        self.grants
            .persistence_under(barrier)
            .prepare_session(previous, next)
            .await;
    }
}

impl<H: AccountHolder> HostAccounts<H> {
    /// Compose one host's policy with its canonical session and grant owners.
    pub fn new(
        services: Arc<RuntimeServices>,
        holder: Arc<H>,
        session_state: Arc<SessionState>,
        grants: Arc<HostGrantStore>,
        ring_vrf_registry: Arc<RingVrfRegistryStore>,
        consent: Arc<ProductConsent>,
        #[cfg(feature = "test-host")] resource_controls: Arc<
            super::test_resource_controls::TestResourceControls,
        >,
    ) -> Self {
        Self {
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            services,
            holder,
            session_state,
            grants,
            ring_vrf_registry,
            consent,
            #[cfg(feature = "test-host")]
            resource_controls,
            #[cfg(feature = "test-host")]
            submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Clear one product's grants and Allow once answers, keeping the session and other products.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
        let product_id =
            normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
        self.consent.forget_allowed_once_for(&product_id);
        let session = {
            let mut lifecycle = self.grants.lifecycle();
            lifecycle.revoke_product(&product_id);
            self.session_state.current()
        };
        self.grants
            .persistence()
            .await
            .clear_product(session.as_ref(), &product_id)
            .await
    }

    /// Forget this activation's grants and Allow once answers, then make
    /// `change` before any grant can be used again.
    pub fn change_activation<T>(&self, change: impl FnOnce() -> T) -> T {
        let mut lifecycle = self.grants.lifecycle();
        lifecycle.clear_memory();
        self.consent.forget_allowed_once();
        change()
    }

    /// Drop the kept AutoSigning keys, as a paired host's logout requires.
    pub async fn forget_auto_signing_keys(&self) -> Result<(), String> {
        self.grants
            .persistence()
            .await
            .clear_auto_signing_keys()
            .await
    }

    /// Keep test submissions in memory when no on-chain allowance exists.
    #[cfg(feature = "test-host")]
    pub fn set_submit_preimages_locally(&self, local: bool) {
        self.submit_preimages_locally
            .store(local, core::sync::atomic::Ordering::Relaxed);
    }

    /// Unchecked allowances cannot authorize a real Bulletin submission.
    #[cfg(feature = "test-host")]
    pub fn submits_preimages_locally(&self) -> bool {
        self.resource_controls.grants_allowances_unchecked()
            || self
                .submit_preimages_locally
                .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Selected account holder identity.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        self.holder.current_session()
    }

    fn hold_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<HostGrantGuard<'_>, AuthorityError> {
        let lifecycle = self.grants.lifecycle();
        self.holder.require_current_session(session)?;
        Ok(lifecycle)
    }

    fn hold_grant(
        &self,
        session: &AuthoritySession,
        revision: u64,
    ) -> Result<HostGrantGuard<'_>, AuthorityError> {
        let lifecycle = self.hold_session(session)?;
        lifecycle.require_revision(revision)?;
        Ok(lifecycle)
    }

    fn grant_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let lifecycle = self.hold_session(session)?;
        let session = self
            .session_state
            .current()
            .ok_or(AuthorityError::Disconnected)?;
        Ok((session, lifecycle.revision()))
    }

    /// Canonical account owner authorized by the calling product's manifest grants.
    pub async fn authorized_product_account(
        &self,
        product: &ProductContext,
        dot_ns_identifier: &str,
        cx: &CallContext,
    ) -> Option<String> {
        let owner = normalize_product_identifier(dot_ns_identifier).ok()?;
        if owner == product.product_id
            || crate::platform::is_localhost_product_identifier(&product.product_id)
        {
            return Some(owner);
        }
        let cx = super::remote_authority_context(cx);
        super::until_cancelled(
            &cx,
            super::product_manifest::grants_scope(
                &self.services,
                self.services.platform.as_ref(),
                &product.product_id,
                &owner,
                crate::host_internal::product_manifest::Granted::Context,
            ),
        )
        .await
        .ok()?
        .then_some(owner)
    }

    /// Disclose an account only after the calling product may access it.
    pub async fn get_account(
        &self,
        cx: &CallContext,
        product: &ProductContext,
        mut account: api::ProductAccountId,
    ) -> Result<[u8; 32], CallError<api::HostAccountGetError>> {
        account.dot_ns_identifier = normalize_product_identifier(&account.dot_ns_identifier)
            .map_err(|_| CallError::Domain(api::HostAccountGetError::DomainNotValid))?;
        let authority_session = self
            .current_session()
            .ok_or(CallError::Domain(api::HostAccountGetError::NotConnected))?;
        if account.dot_ns_identifier != product.product_id {
            match self
                .consent
                .account_access(&product.product_id, &account.dot_ns_identifier)
                .await
            {
                Ok(PermissionAuthorizationStatus::Authorized) => {}
                Ok(
                    PermissionAuthorizationStatus::Denied
                    | PermissionAuthorizationStatus::NotDetermined,
                ) => {
                    return Err(CallError::Domain(api::HostAccountGetError::Rejected));
                }
                Err(error) => {
                    return Err(CallError::HostFailure {
                        reason: error.to_string(),
                    });
                }
            }
        }
        self.product_account_public_key(cx, &authority_session, product, &account)
            .await
        .map_err(super::account_get_authority_error)
    }

    /// Derive an account beneath a subtree selected by an already authorized operation.
    pub async fn product_account_public_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product: &ProductContext,
        product_account_id: &api::ProductAccountId,
    ) -> Result<[u8; 32], AuthorityError> {
        let subtree = self
            .product_subtree_public_key(
                authority_session,
                cx,
                product,
                product_account_id.dot_ns_identifier.clone(),
            )
            .await?;
        derive_product_public_key(
            subtree,
            derivation_index_bytes(&product_account_id.derivation_index),
        )
        .map_err(|err| AuthorityError::Unknown {
            reason: err.to_string(),
        })
    }

    /// Resolve a retained subtree before asking the holder to review and resolve it.
    pub async fn product_subtree_public_key(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let (session, revision) = self.grant_session(authority_session)?;
        let cache_key = (GrantScope::from_session(&session), product_id.clone());
        if let Some(key) = self
            .grants
            .known_product_subtree(&self.session_state, &session, cache_key.clone())
            .await
        {
            self.holder.require_current_session(authority_session)?;
            return Ok(key);
        }
        let resolution = self
            .holder
            .product_subtree_public_key(invocation, product_id)
            .await?;
        let cx = super::remote_authority_context(invocation.call);
        super::remote_authority_call(&cx, async {
            let key = resolution.await?;
            self.holder.require_current_session(authority_session)?;
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
        })
        .await
    }

    /// Review one allocation, then retain each issued capability for the selected wallet.
    pub async fn allocate_resources(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product: &ProductContext,
        request: api::HostRequestResourceAllocationRequest,
    ) -> Result<api::HostRequestResourceAllocationResponse, AuthorityError> {
        let revision = self.hold_session(authority_session)?.revision();
        #[cfg(feature = "test-host")]
        let resources = request.resources.clone();
        let count = request.resources.len();
        let mut grants = match self
            .holder
            .allocate_grants(
                AccountInvocation {
                    call: cx,
                    session: authority_session,
                    caller: AccountCaller::Local {
                        product,
                    },
                },
                request,
                OnExistingAllowancePolicy::Increase,
            )
            .await
        {
            Ok(grants) => grants,
            Err(AuthorityError::Rejected) => {
                return Ok(api::HostRequestResourceAllocationResponse {
                    outcomes: vec![api::AllocationOutcome::Rejected; count],
                });
            }
            Err(error) => return Err(error),
        };
        let cx = super::remote_authority_context_with_default(
            cx,
            super::RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
        );
        super::remote_authority_call(&cx, async {
            self.hold_grant(authority_session, revision)?;
            #[cfg(feature = "test-host")]
            if self.resource_controls.grants_allowances_unchecked() {
                drop(grants);
                return Ok(api::HostRequestResourceAllocationResponse { outcomes: resources.iter().map(|resource| if self.resource_controls.withholds(resource) { api::AllocationOutcome::Rejected } else { api::AllocationOutcome::Allocated }).collect() });
            }
            let mut outcomes = Vec::new();
            loop {
                self.hold_grant(authority_session, revision)?;
                let Some(outcome) = grants.next().await else { break; };
                self.hold_grant(authority_session, revision)?;
                outcomes.push(match outcome? {
                    AccountGrantOutcome::Allocated(grant) => {
                        self.retain_grant(&cx, authority_session, revision, product, grant).await?;
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
        }).await
    }

    async fn retain_grant(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        revision: u64,
        product: &ProductContext,
        grant: AccountGrant,
    ) -> Result<(), AuthorityError> {
        let (session, _) = self.grant_session(authority_session)?;
        self.hold_grant(authority_session, revision)?;
        match grant {
            grant @ (AccountGrant::StatementStore { .. } | AccountGrant::Bulletin(_)) => {
                self.grants
                    .retain_allowance(
                        &self.session_state,
                        &session,
                        revision,
                        &product.product_id,
                        &grant,
                    )
                    .await?;
            }
            AccountGrant::AutoSigning(key) => {
                let expected = self
                    .product_subtree_public_key(
                        authority_session,
                        cx,
                        product,
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
        self.holder.require_current_session(authority_session)
    }

    /// Reuse a retained sponsorship key or request its initial issuance.
    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let (session, revision) = self.grant_session(authority_session)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::StatementStoreAllowance)?;
        if let Some(AccountGrant::StatementStore { key, .. }) = self
            .grants
            .cached_allowance(
                &self.session_state,
                &session,
                revision,
                &product_id,
                AllowanceResource::StatementStore,
            )
            .await?
        {
            self.holder.require_current_session(authority_session)?;
            return Ok(key);
        }
        let product = ProductContext::new(product_id.clone()).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let grant = self
            .holder
            .ensure_allowance(
                AccountInvocation {
                    call: cx,
                    session: authority_session,
                    caller: AccountCaller::Local {
                        product: &product,
                    },
                },
                AllowanceResource::StatementStore,
                OnExistingAllowancePolicy::Ignore,
            )
            .await?;
        match grant {
            AccountGrant::StatementStore { ref key, .. } => {
                self.holder.require_current_session(authority_session)?;
                self.grants
                    .retain_allowance(&self.session_state, &session, revision, &product_id, &grant)
                    .await?;
                Ok(key.clone())
            }
            _ => Err(AuthorityError::Unknown {
                reason: "Unexpected statement-store allowance response resource".to_string(),
            }),
        }
    }

    /// Renew only statements signed by this product's retained sponsorship key.
    pub async fn renew_statement_sponsorship(
        &self,
        cx: &CallContext,
        product: &ProductContext,
        signer: [u8; 32],
    ) -> Result<(), AuthorityError> {
        let Some(authority_session) = self.current_session() else {
            return Ok(());
        };
        let (session, revision) = self.grant_session(&authority_session)?;
        let grant = match self
            .grants
            .cached_allowance(
                &self.session_state,
                &session,
                revision,
                &product.product_id,
                AllowanceResource::StatementStore,
            )
            .await
        {
            Ok(grant) => grant,
            Err(error) => {
                tracing::warn!(%error, product_id = %product.product_id, "could not identify statement sponsorship");
                return Ok(());
            }
        };
        if !matches!(grant, Some(AccountGrant::StatementStore { key, .. }) if key.public_key == signer)
        {
            return Ok(());
        }
        self.hold_grant(&authority_session, revision)?;
        self.holder
            .renew_statement_sponsorship(
                AccountInvocation {
                    call: cx,
                    session: &authority_session,
                    caller: AccountCaller::Local {
                        product,
                    },
                },
                signer,
            )
            .await?;
        self.hold_grant(&authority_session, revision)?;
        Ok(())
    }

    /// Reuse a retained Bulletin key without contacting its issuer.
    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, revision) = self.grant_session(authority_session)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::BulletinAllowance)?;
        if let Some(AccountGrant::Bulletin(key)) = self
            .grants
            .cached_allowance(
                &self.session_state,
                &session,
                revision,
                &product_id,
                AllowanceResource::Bulletin,
            )
            .await?
        {
            self.holder.require_current_session(authority_session)?;
            return Ok(key);
        }
        self.allocate_bulletin_allowance_key(
            cx,
            authority_session,
            product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
    }

    async fn allocate_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
        policy: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, revision) = self.grant_session(authority_session)?;
        let product = ProductContext::new(product_id.clone()).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let grant = self
            .holder
            .ensure_allowance(
                AccountInvocation {
                    call: cx,
                    session: authority_session,
                    caller: AccountCaller::Local {
                        product: &product,
                    },
                },
                AllowanceResource::Bulletin,
                policy,
            )
            .await?;
        match grant {
            AccountGrant::Bulletin(ref key) => {
                self.holder.require_current_session(authority_session)?;
                self.grants
                    .retain_allowance(&self.session_state, &session, revision, &product_id, &grant)
                    .await?;
                Ok(key.clone())
            }
            _ => Err(AuthorityError::Unknown {
                reason: "Unexpected bulletin allowance response resource".to_string(),
            }),
        }
    }

    /// Derive entropy while the original host selection remains locked.
    pub fn derive_entropy(
        &self,
        authority_session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        self.holder
            .derive_entropy(authority_session, product_id, context)
    }

    /// Contact handles under the selected account holder.
    pub fn contact_handles(
        &self,
        authority_session: &AuthoritySession,
    ) -> Result<super::contacts::ContactHandles, AuthorityError> {
        self.holder.contact_handles(authority_session)
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
        authority_session: &AuthoritySession,
        product: &ProductContext,
        account: &api::ProductAccountId,
    ) -> Result<Option<AutoSigningKey>, AuthorityError> {
        if product.product_id != account.dot_ns_identifier {
            return Ok(None);
        }
        let (session, _) = self.grant_session(authority_session)?;
        let grant = self
            .grants
            .auto_signing_key(&session, &account.dot_ns_identifier)
            .await?;
        self.holder.require_current_session(authority_session)?;
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
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: SignPayloadAuthorityRequest,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        if let SignPayloadAuthorityRequest::Product(payload) = &request
            && let Some(grant) = self
                .product_signing_grant(authority_session, product, &payload.account)
                .await?
        {
            let cx = super::remote_authority_context(invocation.call);
            return super::remote_authority_call(&cx, async {
                let _lifecycle = self.hold_grant(authority_session, revision)?;
                let keypair = Self::grant_keypair(&grant, &payload.account)?;
                Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?)
            })
            .await;
        }
        self.holder.sign_payload(invocation, request).await
    }

    /// Preserve legacy review even when a retained product key can sign locally.
    pub async fn sign_raw(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let account = match &request {
            SignRawAuthorityRequest::Product(payload) => Some(&payload.account),
            SignRawAuthorityRequest::LegacyAccount {
                product_account, ..
            } => Some(product_account),
            SignRawAuthorityRequest::IdentityAccount { .. } => None,
        };
        let grant = if watermarked && let Some(account) = account {
            self.product_signing_grant(authority_session, product, account)
                .await
        } else {
            Ok(None)
        };
        if !matches!(request, SignRawAuthorityRequest::Product(_)) && !matches!(&grant, Ok(None)) {
            self.holder.require_current_session(authority_session)?;
            self.consent
                .review(
                    invocation.call,
                    invocation.caller,
                    request.review(invocation.caller, watermarked),
                )
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
                let _lifecycle = self.hold_grant(authority_session, revision)?;
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
        self.holder.sign_raw(invocation, request, watermarked).await
    }

    /// Prepare chain metadata before using a retained key with the selected grant.
    pub async fn create_transaction(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        if let CreateTransactionAuthorityRequest::Product(payload) = &request
            && let Some(grant) = self
                .product_signing_grant(authority_session, product, &payload.signer)
                .await?
        {
            if !payload.contacts.is_empty() {
                self.consent
                    .review(
                        invocation.call,
                        invocation.caller,
                        request.review(invocation.caller),
                    )
                    .await?;
            }
            let cx = super::remote_authority_context(invocation.call);
            return super::remote_authority_call(&cx, async {
                let metadata =
                    local_transaction_metadata(&self.services.chain, payload.genesis_hash).await?;
                let _lifecycle = self.hold_grant(authority_session, revision)?;
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
            })
            .await;
        }
        self.holder.create_transaction(invocation, request).await
    }

    /// Keep retained VRF signing restricted to this host's bound product caller.
    pub async fn sign_vrf(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountSignVrfRequest,
    ) -> Result<api::VrfSignature, AuthorityError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        if let Some(grant) = self
            .product_signing_grant(authority_session, product, &request.account)
            .await?
        {
            let _lifecycle = self.hold_grant(authority_session, revision)?;
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
        self.holder.sign_vrf(invocation, request).await
    }

    /// Sign exact proof bytes, without using a raw-signing wire message as a substitute.
    pub async fn sign_statement_store_product_payload(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        account: api::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        if let Some(grant) = self
            .product_signing_grant(authority_session, product, &account)
            .await?
        {
            let _lifecycle = self.hold_grant(authority_session, revision)?;
            let keypair = Self::grant_keypair(&grant, &account)?;
            return Ok(keypair
                .secret
                .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
                .to_bytes());
        }
        self.holder
            .sign_statement_store_product_payload(invocation, account, payload)
            .await
    }
}

impl<H: AccountHolder + 'static> HostAccounts<H> {
    /// Providers in the selected wallet's registry.
    pub async fn ring_vrf_providers(
        &self,
        ring: &api::RingLocation,
    ) -> Result<Vec<api::ProductAccountId>, RingVrfError> {
        let authority_session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        let entries = self
            .ring_vrf_registry
            .providers(authority_session.public_key, ring)
            .await?;
        self.holder.require_current_session(&authority_session)?;
        Ok(entries)
    }

    /// Selected provider in the same canonical registry used by the wallet.
    pub async fn selected_ring_vrf_provider(
        &self,
        ring: &api::RingLocation,
    ) -> Result<Option<api::ProductAccountId>, RingVrfError> {
        let authority_session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        let provider = self
            .ring_vrf_registry
            .selected_provider(authority_session.public_key, ring)
            .await?;
        self.holder.require_current_session(&authority_session)?;
        Ok(provider)
    }

    /// Prepare persistence before checking the originally selected account.
    pub async fn select_ring_vrf_provider(
        &self,
        ring: api::RingLocation,
        handle: api::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let authority_session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        let mut update = self
            .ring_vrf_registry
            .prepare_update(authority_session.public_key)
            .await?;
        {
            let _lifecycle = self.hold_session(&authority_session)?;
            update.select_provider(ring, handle)?;
        }
        update.persist().await
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
        cx: &CallContext,
        product: &ProductContext,
        handle: &api::ProductAccountId,
    ) -> Result<
        (
            api::ProductAccountId,
            super::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = super::until_cancelled(
            cx,
            super::product_manifest::ring_vrf_key_access_granted(
                &self.services,
                self.services.platform.as_ref(),
                &product.product_id,
                handle,
            ),
        )
        .await
        .map_err(|_| RingVrfError::NotAllowlisted)??;
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
        authority_session: &AuthoritySession,
        handle: &api::ProductAccountId,
        ring: Option<&api::RingLocation>,
    ) -> Result<Option<(Zeroizing<[u8; 32]>, vrf::Vrf)>, RingVrfError> {
        let (session, revision) = self.grant_session(authority_session)?;
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
        let _lifecycle = self.hold_grant(authority_session, revision)?;
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
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountGetAliasRequest,
    ) -> Result<api::ContextualAlias, RingVrfError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        if product.product_id == request.key_handle.dot_ns_identifier
            && let Some((entropy, vrf)) = self
                .local_ring_vrf_entropy(
                    authority_session,
                    &request.key_handle,
                    Some(&request.ring_location),
                )
                .await?
        {
            self.ring_resolver.validate(&request.ring_location).await?;
            let _lifecycle = self.hold_grant(authority_session, revision)?;
            let context = development_context_bytes(&request.context);
            return Ok(api::ContextualAlias {
                alias: vrf.alias(&entropy, &context)?.to_vec(),
                context,
            });
        }
        self.holder.account_alias(invocation, request).await
    }

    /// Resolve the ring before the final guarded proof computation.
    pub async fn create_proof(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountCreateProofRequest,
    ) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let (handle, access) = self
            .require_ring_vrf_key_access(cx, product, &request.key_handle)
            .await?;
        super::remote_authority_call(cx, async {
            super::product_manifest::require_own_context(&access, &request.context)?;
            if let Some((entropy, vrf)) = self
                .local_ring_vrf_entropy(
                    authority_session,
                    &handle,
                    Some(&request.ring_location),
                )
                .await?
            {
                let member = {
                    let _lifecycle = self.hold_grant(authority_session, revision)?;
                    vrf.member(&entropy)?
                };
                let resolved = self
                    .ring_resolver
                    .resolve(&request.ring_location, &[MemberCandidate { member }])
                    .await?;
                let _lifecycle = self.hold_grant(authority_session, revision)?;
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
            self.holder.create_proof(invocation, request).await
        })
        .await
    }

    /// Register with a kept AutoSigning key when one exists; the account holder records it either way.
    pub async fn register_ring_vrf_key(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<[u8; 32], RingVrfError> {
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let (session, revision) = self.grant_session(authority_session)?;
        let Some(grant) = self
            .grants
            .auto_signing_key(&session, &product.product_id)
            .await?
        else {
            return self.holder.register_ring_vrf_key(invocation, request).await;
        };
        self.ring_resolver.validate(&request.ring).await?;
        let vrf = vrf::load().await?;
        let public_key = {
            let _lifecycle = self.hold_grant(authority_session, revision)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                grant.ring_vrf_domain_entropy(),
                &request.index,
            ));
            vrf.member(&entropy)?
        };
        self.holder
            .record_ring_vrf_key(invocation, request, public_key)
            .await?;
        Ok(public_key)
    }

    /// Answer an own complete listing locally; otherwise the account holder lists and keeps the registry current.
    pub async fn list_ring_vrf_keys(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let owner = normalize_product_identifier(&request.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if product.product_id == owner
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(authority_session.public_key, &owner)
                .await?
        {
            self.holder.require_current_session(authority_session)?;
            apply_ring_vrf_disclosure(&mut entries, request.disclosure);
            return Ok(entries);
        }
        let entries = self.holder.list_ring_vrf_keys(invocation, request).await?;
        validate_owner_listing(&owner, &entries)?;
        self.holder.require_current_session(authority_session)?;
        Ok(entries)
    }

    /// Sign registered-key bytes only while the retained capability remains selected.
    pub async fn ring_vrf_sign(
        &self,
        authority_session: &AuthoritySession,
        cx: &CallContext,
        product: &ProductContext,
        request: api::HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        let revision = self.grants.lifecycle().revision();
        self.holder.require_current_session(authority_session)?;
        let invocation = AccountInvocation {
            call: cx,
            session: authority_session,
            caller: AccountCaller::Local { product },
        };
        let (handle, _) = self
            .require_ring_vrf_key_access(cx, product, &request.key_handle)
            .await?;
        super::remote_authority_call(cx, async {
            if let Some((entropy, vrf)) = self
                .local_ring_vrf_entropy(authority_session, &handle, None)
                .await?
            {
                let _lifecycle = self.hold_grant(authority_session, revision)?;
                return vrf.sign(&entropy, &request.message);
            }
            self.holder.ring_vrf_sign(invocation, request).await
        })
        .await
    }
}

impl<H: AccountHolder> HostAccounts<H> {
    /// Check the retained allowance before preparing and signing a Bulletin submission.
    pub async fn submit_preimage(
        &self,
        cx: &CallContext,
        deadline: Instant,
        authority_session: &AuthoritySession,
        product_id: String,
        value: &[u8],
    ) -> Result<Vec<u8>, super::bulletin_rpc::BulletinSubmitError> {
        use super::bulletin_rpc::BulletinSubmitError;
        use super::statement_allowance::{fetch_bulletin_allowance, wait_bulletin_authorization};
        use crate::host_internal::bulletin::allowance_signer;
        use subxt::tx::Signer;

        let revision = self.grants.lifecycle().revision();
        let mut allowance = self
            .bulletin_allowance_key(cx, authority_session, product_id.clone())
            .await?;
        #[cfg(feature = "test-host")]
        if self.submits_preimages_locally() {
            self.hold_grant(authority_session, revision)?;
            return Ok(crate::host_internal::bulletin::preimage_key(value).to_vec());
        }
        let rpc = self
            .services
            .bulletin
            .client("Bulletin authorization")
            .await
            .map_err(BulletinSubmitError::Host)?;
        let rpc = super::statement_allowance::rpc::RpcClient::new(rpc);
        let target = allowance_signer(&allowance)
            .map_err(BulletinSubmitError::InvalidAllowanceKey)?
            .account_id()
            .0;
        let current = fetch_bulletin_allowance(&rpc, &target)
            .await
            .map_err(|error| AuthorityError::Unavailable {
                reason: error.to_string(),
            })?;
        if !current.is_some_and(|info| info.can_store(value.len() as u64)) {
            self.hold_grant(authority_session, revision)?;
            allowance = self
                .allocate_bulletin_allowance_key(
                    cx,
                    authority_session,
                    product_id,
                    OnExistingAllowancePolicy::Increase,
                )
                .await?;
            let replacement = allowance_signer(&allowance)
                .map_err(BulletinSubmitError::InvalidAllowanceKey)?
                .account_id()
                .0;
            let baseline = (replacement == target).then_some(current).flatten();
            let timeout = deadline.saturating_duration_since(Instant::now());
            wait_bulletin_authorization(&rpc, &replacement, baseline, value.len() as u64, timeout)
                .await
                .map_err(|error| AuthorityError::Unavailable {
                    reason: error.to_string(),
                })?;
        }
        let signer =
            allowance_signer(&allowance).map_err(BulletinSubmitError::InvalidAllowanceKey)?;
        self.services
            .bulletin
            .submit_preimage(
                cx,
                deadline,
                &signer.account_id(),
                &|bytes| {
                    let _grant = self.hold_grant(authority_session, revision)?;
                    Ok(signer.sign(bytes))
                },
                value,
            )
            .await
    }

    /// Acquire and sign a statement while its retained grant remains selected.
    pub async fn create_authorized_statement_proof(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
        statement: api::Statement,
    ) -> Result<api::StatementProof, super::statement_store::StatementProofFailure> {
        let revision = self.grants.lifecycle().revision();
        let allowance = self
            .statement_store_allowance_key(cx, authority_session, product_id)
            .await
            .map_err(super::statement_store::statement_authority_failure)?;
        let _lifecycle = self
            .hold_grant(authority_session, revision)
            .map_err(super::statement_store::statement_authority_failure)?;
        super::statement_store::create_statement_proof_with_key(statement, &allowance)
    }
}
