//! Pairing-host role for inter-host account authority.
//!
//! Account policy and retained grants use the paired [`SsoRequestService`].

mod sso_channel;

use std::sync::Arc;
use truapi::latest::{
    HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
    HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

use super::authority::{
    AccountCaller, AccountHolder, AccountInvocation, AuthorityError, AuthoritySession,
    AutoSigningKey, BulletinAllowanceKey, CreateTransactionAuthorityRequest, ProductAuthority,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
    require_current_session,
};
use super::host_grants::HostGrantStore;
use super::product_consent::ProductConsent;
use super::services::RuntimeServices;
use super::sso_remote::SsoSessionKey;
use super::sso_request_service::SsoRequestService;
use crate::chain_runtime::ChainRuntime;
use crate::host_internal::extrinsic::{
    Sr25519Signer, build_signed_transaction, local_transaction_metadata,
};
use crate::host_internal::sso_messages::{
    ProductRequest, RingVrfError, SsoAllocatedResource, SsoAllocationOutcome,
};
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair_from_subtree_secret,
    derive_ring_vrf_entropy_from_domain,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::runtime::vrf;

use crate::platform::{
    Platform, ProductContext, SignVrfReview, UserConfirmationReview, normalize_product_identifier,
};
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, latest};
use zeroize::Zeroizing;

use super::ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing};
use super::signing_host::ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};

/// Product account policy using retained grants and a paired account holder.
pub struct PairingHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    chain: ChainRuntime,
    sso: Arc<SsoRequestService>,
    grants: Arc<HostGrantStore>,
    holder: Arc<super::SsoAccountHolderClient>,
    consent: Arc<ProductConsent>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    #[cfg(feature = "test-host")]
    submit_preimages_locally: core::sync::atomic::AtomicBool,
}

impl PairingHost {
    /// Keep preimage submissions in the core instead of the Bulletin chain.
    #[cfg(feature = "test-host")]
    pub fn set_submit_preimages_locally(&self, local: bool) {
        self.submit_preimages_locally
            .store(local, core::sync::atomic::Ordering::Relaxed);
    }

    /// Compose one paired host and its session service with shared grant ownership.
    pub fn new(
        services: Arc<RuntimeServices>,
        config: crate::platform::PairingHostConfig,
    ) -> (Arc<Self>, Arc<SsoRequestService>) {
        let grants = Arc::new(HostGrantStore::new(services.platform.clone()));
        let sso = SsoRequestService::new(services.clone(), config, grants.clone());
        if services.asset_hub_chain_genesis_hash().is_none() {
            tracing::warn!(
                "no Asset Hub configured on the pairing role: no product manifest \
                 will resolve, so every cross-product grant is refused"
            );
        }
        let host = Arc::new(Self {
            platform: services.platform.clone(),
            consent: Arc::new(ProductConsent::new(services.platform.clone())),
            chain: services.chain.clone(),
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            ring_vrf_registry: RingVrfRegistryStore::new(services.platform.clone()),
            services,
            holder: Arc::new(super::SsoAccountHolderClient::new(sso.clone())),
            sso: sso.clone(),
            grants,
            #[cfg(feature = "test-host")]
            submit_preimages_locally: core::sync::atomic::AtomicBool::new(false),
        });
        (host, sso)
    }

    /// Real session service exercised by lifecycle and transport tests.
    #[cfg(test)]
    pub fn sso_for_tests(&self) -> &Arc<SsoRequestService> {
        &self.sso
    }

    /// Providers registered for this ring under the active account.
    pub async fn ring_vrf_providers(
        &self,
        ring: &latest::RingLocation,
    ) -> Result<Vec<latest::ProductAccountId>, RingVrfError> {
        let session = self
            .sso
            .session_state()
            .current()
            .ok_or(RingVrfError::Unknown {
                reason: "no active session".to_string(),
            })?;
        self.ring_vrf_registry
            .providers(session.public_key, ring)
            .await
    }

    /// Provider selected for this ring under the active account.
    pub async fn selected_ring_vrf_provider(
        &self,
        ring: &latest::RingLocation,
    ) -> Result<Option<latest::ProductAccountId>, RingVrfError> {
        let session = self
            .sso
            .session_state()
            .current()
            .ok_or(RingVrfError::Unknown {
                reason: "no active session".to_string(),
            })?;
        self.ring_vrf_registry
            .selected_provider(session.public_key, ring)
            .await
    }

    /// Persist the provider choice under the active account.
    pub async fn select_ring_vrf_provider(
        &self,
        ring: latest::RingLocation,
        handle: latest::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let session = self
            .sso
            .session_state()
            .current()
            .ok_or(RingVrfError::Unknown {
                reason: "no active session".to_string(),
            })?;
        self.ring_vrf_registry
            .select_provider(session.public_key, ring, handle)
            .await
    }

    /// Clear all capability material owned by one product while preserving the
    /// active session and unrelated products.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), String> {
        let product_id =
            normalize_product_identifier(product_id).map_err(|error| error.to_string())?;
        self.consent.forget_allowed_once_for(&product_id);
        let session = {
            let mut lifecycle = self.grants.lifecycle();
            lifecycle.advance();
            self.sso.session_state().current()
        };
        self.grants
            .persistence()
            .await
            .clear_product(session.as_ref(), &product_id)
            .await
    }

    /// Retained grants exercised by runtime lifecycle tests.
    #[cfg(test)]
    pub fn grants_for_tests(&self) -> &HostGrantStore {
        &self.grants
    }

    #[cfg(test)]
    pub async fn register_ring_vrf_key_for_tests(
        &self,
        session: &SessionInfo,
        handle: latest::ProductAccountId,
        ring: latest::RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        self.ring_vrf_registry
            .register(session.public_key, handle, ring, public_key)
            .await
    }

    /// Whether resolving `product_id`'s subtree would reach the Account Holder,
    /// i.e. neither the memory cache nor the persisted slot already holds it.
    ///
    /// A stale or missing session resolves as `false` so a broken state falls
    /// through to the resolution's own error rather than a spurious prompt.
    async fn subtree_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        let Ok(session) = self.current_private_session(session) else {
            return false;
        };
        let Some(sso) = session.sso.as_ref() else {
            return false;
        };
        let cache_key = (SsoSessionKey::from_session(sso), product_id.to_string());
        self.grants
            .known_product_subtree(&self.sso.session_state(), &session, cache_key)
            .await
            .is_none()
    }

    fn current_private_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        require_current_session(&self.sso.session_state(), session)
    }

    fn grant_session(
        &self,
        authority_session: &AuthoritySession,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let lifecycle = self.grants.lifecycle();
        let session = self.current_private_session(authority_session)?;
        Ok((session, lifecycle.revision()))
    }

    /// Keep access checks and key derivation bound to the same canonical owner.
    async fn require_ring_vrf_key_access(
        &self,
        caller: AccountCaller<'_>,
        handle: &latest::ProductAccountId,
    ) -> Result<
        (
            latest::ProductAccountId,
            crate::runtime::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
            &self.services,
            self.platform.as_ref(),
            caller.product_id().ok_or(RingVrfError::NotAllowlisted)?,
            handle,
        )
        .await?;
        Ok((
            latest::ProductAccountId {
                dot_ns_identifier: access.owner.clone(),
                derivation_index: handle.derivation_index.clone(),
            },
            access,
        ))
    }

    async fn local_ring_vrf_entropy(
        &self,
        session: &SessionInfo,
        handle: &latest::ProductAccountId,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(auto_signing) = self
            .grants
            .auto_signing_key(session, &handle.dot_ns_identifier)
            .await
            .map_err(RingVrfError::from)?
        else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
            auto_signing.ring_vrf_domain_entropy(),
            &handle.derivation_index,
        ));
        let vrf = vrf::load().await?;
        if entry.public_key != Some(vrf.member(&entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the AutoSigning capability"
                    .to_string(),
            });
        }
        Ok(Some(entropy))
    }

    async fn local_ring_vrf_entropy_for_ring(
        &self,
        session: &SessionInfo,
        handle: &latest::ProductAccountId,
        ring: &latest::RingLocation,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(entropy) = self.local_ring_vrf_entropy(session, handle).await? else {
            return Ok(None);
        };
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if !entry.rings.contains(ring) {
            return Err(RingVrfError::KeyNotInRing);
        }
        Ok(Some(entropy))
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let session = self.current_private_session(session)?;
        sso_channel::remote_product_subtree_public_key(self, cx, &session, product_id).await
    }

    /// The keypair that can serve `account` locally under an AutoSigning
    /// capability, when this host holds one.
    ///
    /// The single source of truth for local serviceability: the grant
    /// predicate and every local fast path call it, so a request can never be
    /// told "no prompt" and then relayed to a signing host that will prompt
    /// for it.
    ///
    /// A caller may name an account belonging to another product —
    /// `authorized_product_account` admits that for a `localhost` caller, and
    /// for one the owner granted `context` — and the grant covers the owning
    /// product's subtree, not the caller's. Serving one locally would sign with a key the caller was
    /// never granted and skip the confirmation the signing host raises for a
    /// relayed request, so the binding is checked here rather than only at the
    /// gate.
    async fn local_product_signing_key(
        &self,
        session: &SessionInfo,
        calling_product_id: Option<&str>,
        account: &latest::ProductAccountId,
    ) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
        if calling_product_id != Some(account.dot_ns_identifier.as_str()) {
            return Ok(None);
        }
        let Some(key) = self
            .grants
            .auto_signing_key(session, &account.dot_ns_identifier)
            .await?
        else {
            return Ok(None);
        };
        derive_product_keypair_from_subtree_secret(
            *key.as_secret_bytes(),
            derivation_index_bytes(&account.derivation_index),
        )
        .map(Some)
        .map_err(|err| AuthorityError::Unknown {
            reason: err.to_string(),
        })
    }

    async fn sign_vrf(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountSignVrfRequest,
    ) -> Result<latest::VrfSignature, AuthorityError> {
        let session = invocation.session;
        let cx = invocation.call;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(AuthorityError::Rejected)?;
        let session = self.current_private_session(session)?;
        if calling_product_id == request.account.dot_ns_identifier
            && let Some(auto_signing_key) = self
                .grants
                .auto_signing_key(&session, &request.account.dot_ns_identifier)
                .await?
        {
            let keypair = derive_product_keypair_from_subtree_secret(
                *auto_signing_key.as_secret_bytes(),
                derivation_index_bytes(&request.account.derivation_index),
            )
            .map_err(|err| AuthorityError::Unknown {
                reason: err.to_string(),
            })?;
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            return Ok(latest::VrfSignature { pre_output, proof });
        }
        if !super::authority::is_blessed_owner(
            calling_product_id,
            &request.account.dot_ns_identifier,
        ) {
            self.consent
                .review(
                    cx,
                    invocation.caller,
                    UserConfirmationReview::SignVrf(SignVrfReview {
                        calling_product_id: calling_product_id.to_string(),
                        request: request.clone(),
                    }),
                )
                .await
                .map_err(|error| match error {
                    AuthorityError::ConfirmationFailed(err) => AuthorityError::Unknown {
                        reason: format!("VRF signing confirmation failed: {err:?}"),
                    },
                    error => error,
                })?;
        }
        self.holder.sign_vrf(invocation, request).await
    }

    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<latest::ContextualAlias, RingVrfError> {
        let session = invocation.session;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let private_session = self.current_private_session(session)?;
        if calling_product_id == request.key_handle.dot_ns_identifier
            && let Some(entropy) = self
                .local_ring_vrf_entropy_for_ring(
                    &private_session,
                    &request.key_handle,
                    &request.ring_location,
                )
                .await?
        {
            self.ring_resolver.validate(&request.ring_location).await?;
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            let context = development_context_bytes(&request.context);
            let alias = vrf.alias(&entropy, &context)?;
            return Ok(latest::ContextualAlias {
                context,
                alias: alias.to_vec(),
            });
        }
        self.holder.account_alias(invocation, request).await
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
        let session = invocation.session;
        let (key_handle, access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        // A publisher's grant cannot expose its alias in an unrelated product's context.
        crate::runtime::product_manifest::require_own_context(&access, &request.context)?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy_for_ring(&private_session, &key_handle, &request.ring_location)
            .await?
        {
            let vrf = vrf::load().await?;
            let member = vrf.member(&entropy)?;
            let resolved = self
                .ring_resolver
                .resolve(&request.ring_location, &[MemberCandidate { member }])
                .await?;
            self.current_private_session(session)?;
            let context = development_context_bytes(&request.context);
            let (proof, alias) =
                create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
            return Ok(latest::HostAccountCreateProofResponse {
                proof,
                contextual_alias: latest::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            });
        }
        self.holder.create_proof(invocation, request).await
    }

    async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<[u8; 32], RingVrfError> {
        let private_session = self.current_private_session(session)?;
        let handle = latest::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&request.calling_product_id).map_err(
                |error| RingVrfError::Unknown {
                    reason: error.to_string(),
                },
            )?,
            derivation_index: request.payload.index.clone(),
        };
        if let Some(auto_signing) = self
            .grants
            .auto_signing_key(&private_session, &request.calling_product_id)
            .await
            .map_err(RingVrfError::from)?
        {
            self.ring_resolver.validate(&request.payload.ring).await?;
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                auto_signing.ring_vrf_domain_entropy(),
                &request.payload.index,
            ));
            let public_key = vrf.member(&entropy)?;
            self.ring_vrf_registry
                .register(
                    private_session.public_key,
                    handle,
                    request.payload.ring.clone(),
                    public_key,
                )
                .await?;
            self.current_private_session(session)?;
            sso_channel::mirror_ring_vrf_registration(self, private_session, request);
            return Ok(public_key);
        }
        let public_key = self
            .holder
            .register_ring_vrf_key(
                AccountInvocation {
                    call: cx,
                    session,
                    caller: AccountCaller::Remote {
                        product_id: Some(&request.calling_product_id),
                    },
                },
                request.payload.clone(),
            )
            .await?;
        self.ring_vrf_registry
            .register(
                private_session.public_key,
                handle,
                request.payload.ring,
                public_key,
            )
            .await?;
        self.current_private_session(session)?;
        Ok(public_key)
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
        let session = invocation.session;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(RingVrfError::NotAllowlisted)?;
        let private_session = self.current_private_session(session)?;
        let owner = normalize_product_identifier(&request.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if calling_product_id == owner
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(private_session.public_key, &owner)
                .await?
        {
            self.current_private_session(session)?;
            apply_ring_vrf_disclosure(&mut entries, request.disclosure);
            return Ok(entries);
        }
        let requested_disclosure = request.disclosure;
        let mut remote_request = ProductRequest {
            calling_product_id: calling_product_id.to_string(),
            payload: request,
        };
        if remote_request.calling_product_id == owner {
            remote_request.payload.disclosure = latest::RingVrfKeyDisclosure::PublicKey;
        }
        let mut entries = self
            .holder
            .list_ring_vrf_keys(invocation, remote_request.payload)
            .await?;
        validate_owner_listing(&owner, &entries)?;
        if entries.iter().all(|entry| entry.public_key.is_some()) {
            entries = self
                .ring_vrf_registry
                .reconcile_owner(private_session.public_key, &owner, entries)
                .await?;
        }
        self.current_private_session(session)?;
        apply_ring_vrf_disclosure(&mut entries, requested_disclosure);
        Ok(entries)
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        let session = invocation.session;
        let (key_handle, _access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy(&private_session, &key_handle)
            .await?
        {
            let vrf = vrf::load().await?;
            self.current_private_session(session)?;
            return vrf.sign(&entropy, &request.message);
        }
        self.holder.ring_vrf_sign(invocation, request).await
    }

    async fn cache_allowance_outcomes(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: &str,
        outcomes: &[SsoAllocationOutcome],
    ) -> Result<(), AuthorityError> {
        for outcome in outcomes {
            if let SsoAllocationOutcome::Allocated(resource) = outcome {
                match resource {
                    SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
                        self.grants
                            .cache_statement_store_allowance_key(
                                &self.sso.session_state(),
                                session,
                                lifecycle_epoch,
                                product_id,
                                slot_account_key.clone(),
                            )
                            .await?;
                    }
                    SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                        self.grants
                            .cache_bulletin_allowance_key(
                                &self.sso.session_state(),
                                session,
                                lifecycle_epoch,
                                product_id,
                                slot_account_key.clone(),
                            )
                            .await?;
                    }
                    SsoAllocatedResource::SmartContractAllowance => {}
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key,
                        ring_vrf_domain_entropy,
                    } => {
                        let expected_product_subtree_public_key =
                            sso_channel::remote_product_subtree_public_key(
                                self,
                                cx,
                                session,
                                product_id.to_string(),
                            )
                            .await?;
                        self.grants
                            .remember_auto_signing_key(
                                &self.sso.session_state(),
                                session,
                                lifecycle_epoch,
                                product_id,
                                expected_product_subtree_public_key,
                                AutoSigningKey::from_parts(
                                    *product_root_private_key,
                                    *ring_vrf_domain_entropy,
                                ),
                            )
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        account: latest::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        let (session, revision) = self.grant_session(invocation.session)?;
        self.consent
            .review(invocation.call, invocation.caller, UserConfirmationReview::StatementStoreProductSign(
                        crate::platform::StatementStoreProductSignReview {
                            calling_product_id: invocation.caller.product_id().map(str::to_string),
                            account: account.clone(),
                            payload: payload.clone(),
                        },
                    ))
            .await?;
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            let keypair = match invocation.caller {
                AccountCaller::Local { product, .. } => self.local_product_signing_key(&session, Some(&product.product_id), &account).await?,
                AccountCaller::Remote { .. } => None,
            };
            let Some(keypair) = keypair else {
                return Err(AuthorityError::Unavailable { reason: "pairing host: exact statement proof signing needs an AutoSigning capability; the current SSO raw-signing protocol cannot carry it".to_string() });
            };
            let lifecycle = self.grants.lifecycle();
            lifecycle.require_revision(revision)?;
            self.current_private_session(invocation.session)?;
            Ok(keypair.secret.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public).to_bytes())
        }).await
    }
}

fn apply_ring_vrf_disclosure(
    entries: &mut [latest::RegisteredRingVrfKey],
    disclosure: latest::RingVrfKeyDisclosure,
) {
    if disclosure == latest::RingVrfKeyDisclosure::Anonymized {
        for entry in entries {
            entry.public_key = None;
        }
    }
}

#[async_trait::async_trait]
impl ProductAuthority for PairingHost {
    fn account_holder(&self) -> &dyn AccountHolder {
        self
    }

    fn consent(&self) -> &ProductConsent {
        &self.consent
    }

    async fn allocate_resources(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product: &ProductContext,
        request: latest::HostRequestResourceAllocationRequest,
    ) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
        let (session, lifecycle_epoch) = self.grant_session(authority_session)?;
        match self
            .consent
            .review(
                cx,
                AccountCaller::Local {
                    product,
                    authorization: None,
                },
                UserConfirmationReview::ResourceAllocation(
                    crate::platform::ResourceAllocationReview {
                        calling_product_id: product.product_id.clone(),
                        resources: request.resources.clone(),
                    },
                ),
            )
            .await
        {
            Ok(()) => {}
            Err(AuthorityError::Rejected) => {
                return Ok(latest::HostRequestResourceAllocationResponse {
                    outcomes: vec![latest::AllocationOutcome::Rejected; request.resources.len()],
                });
            }
            Err(error) => return Err(error),
        }
        let cx = super::remote_authority_context_with_default(
            cx,
            super::RESOURCE_ALLOCATION_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
        );
        super::remote_authority_call(&cx, async {
            self.current_private_session(authority_session)?;
            self.grants.lifecycle().require_revision(lifecycle_epoch)?;
            let outcomes = sso_channel::remote_allocate_resources(
                self,
                &cx,
                &session,
                product.product_id.clone(),
                request,
            )
            .await?;
            self.cache_allowance_outcomes(
                &cx,
                &session,
                lifecycle_epoch,
                &product.product_id,
                &outcomes,
            )
            .await?;
            self.current_private_session(authority_session)?;
            self.grants.lifecycle().require_revision(lifecycle_epoch)?;
            Ok(latest::HostRequestResourceAllocationResponse {
                outcomes: outcomes.into_iter().map(Into::into).collect(),
            })
        })
        .await
    }

    #[cfg(feature = "test-host")]
    fn submits_preimages_locally(&self) -> bool {
        self.submit_preimages_locally
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    fn session_state(&self) -> Arc<SessionState> {
        self.sso.session_state()
    }

    #[cfg(test)]
    fn cache_product_subtree_for_test(
        &self,
        session: &SessionInfo,
        product_id: &str,
        public_key: [u8; 32],
    ) {
        self.grants
            .cache_product_subtree_for_test(session, product_id, public_key);
    }

    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        self.sso.request_login(product).await
    }

    async fn disconnect(&self) {
        self.sso.disconnect().await;
    }

    async fn refresh_session_identity(&self) -> Option<AuthoritySession> {
        self.sso.refresh_current_session_identity().await
    }

    async fn subtree_resolution_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        PairingHost::subtree_reaches_account_holder(self, session, product_id).await
    }

    fn wallet_authorization(
        &self,
        authority_session: &AuthoritySession,
        _product: &ProductContext,
    ) -> Result<Option<super::WalletAuthorization>, AuthorityError> {
        self.current_private_session(authority_session)?;
        Ok(None)
    }

    async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.grant_session(authority_session)?;
        sso_channel::remote_statement_store_allowance_key(
            self,
            cx,
            &session,
            lifecycle_epoch,
            product_id,
        )
        .await
    }

    async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.grant_session(authority_session)?;
        sso_channel::remote_bulletin_allowance_key(self, cx, &session, lifecycle_epoch, product_id)
            .await
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        authority_session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let (session, lifecycle_epoch) = self.grant_session(authority_session)?;
        sso_channel::remote_refresh_bulletin_allowance_key(
            self,
            cx,
            &session,
            lifecycle_epoch,
            product_id,
        )
        .await
    }
}

#[async_trait::async_trait]
impl AccountHolder for PairingHost {
    fn current_session(&self) -> Option<AuthoritySession> {
        self.sso.current_session()
    }

    async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        PairingHost::product_subtree_public_key(self, cx, session, product_id).await
    }

    async fn sign_vrf(
        &self,
        invocation: AccountInvocation<'_>,
        request: latest::HostAccountSignVrfRequest,
    ) -> Result<latest::VrfSignature, AuthorityError> {
        PairingHost::sign_vrf(self, invocation, request).await
    }

    async fn sign_payload(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let (session, revision) = self.grant_session(invocation.session)?;
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && let SignPayloadAuthorityRequest::Product(payload) = &request
        {
            self.local_product_signing_key(&session, Some(&product.product_id), &payload.account)
                .await?
        } else {
            None
        };
        if keypair.is_none() && matches!(invocation.caller, AccountCaller::Local { .. }) {
            self.consent
                .review(invocation.call, invocation.caller, request.review(invocation.caller))
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.current_private_session(invocation.session)?;
            if let Some(keypair) = keypair
                && let SignPayloadAuthorityRequest::Product(payload) = request
            {
                let lifecycle = self.grants.lifecycle();
                lifecycle.require_revision(revision)?;
                self.current_private_session(invocation.session)?;
                return Ok(sign_extrinsic_payload(&keypair, payload.payload)?);
            }
            self.holder
                .sign_payload(
                    AccountInvocation {
                        call: &cx,
                        ..invocation
                    },
                    request,
                )
                .await
        })
        .await
    }

    async fn sign_raw(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<latest::HostSignPayloadResponse, AuthorityError> {
        let (session, revision) = self.grant_session(invocation.session)?;
        if !matches!(request, SignRawAuthorityRequest::Product(_))
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            self.consent
                .review(invocation.call, invocation.caller, request.review(invocation.caller, watermarked))
                .await?;
        }
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && watermarked
        {
            let account = match &request {
                SignRawAuthorityRequest::Product(payload) => Some(&payload.account),
                SignRawAuthorityRequest::LegacyAccount {
                    product_account, ..
                } => Some(product_account),
                SignRawAuthorityRequest::IdentityAccount { .. } => None,
            };
            if let Some(account) = account {
                self.local_product_signing_key(&session, Some(&product.product_id), account)
                    .await?
            } else {
                None
            }
        } else {
            None
        };
        if keypair.is_none()
            && matches!(request, SignRawAuthorityRequest::Product(_))
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            self.consent
                .review(invocation.call, invocation.caller, request.review(invocation.caller, watermarked))
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.current_private_session(invocation.session)?;
            if let Some(keypair) = keypair {
                let payload = match request {
                    SignRawAuthorityRequest::Product(request) => request.payload,
                    SignRawAuthorityRequest::LegacyAccount { request, .. }
                    | SignRawAuthorityRequest::IdentityAccount { request, .. } => request.payload,
                };
                let message = raw_payload_bytes(payload, watermarked)?;
                let lifecycle = self.grants.lifecycle();
                lifecycle.require_revision(revision)?;
                self.current_private_session(invocation.session)?;
                let signature = keypair
                    .secret
                    .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
                    .to_bytes();
                return Ok(latest::HostSignPayloadResponse {
                    signature: signature.to_vec(),
                    signed_transaction: None,
                });
            }
            self.holder
                .sign_raw(
                    AccountInvocation {
                        call: &cx,
                        ..invocation
                    },
                    request,
                    watermarked,
                )
                .await
        })
        .await
    }

    async fn create_transaction(
        &self,
        invocation: AccountInvocation<'_>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<latest::HostCreateTransactionResponse, AuthorityError> {
        let (session, revision) = self.grant_session(invocation.session)?;
        let keypair = if let AccountCaller::Local { product, .. } = invocation.caller
            && let CreateTransactionAuthorityRequest::Product(payload) = &request
        {
            self.local_product_signing_key(&session, Some(&product.product_id), &payload.signer)
                .await?
        } else {
            None
        };
        let names_contacts = matches!(&request, CreateTransactionAuthorityRequest::Product(payload) if !payload.contacts.is_empty());
        if (keypair.is_none() || names_contacts)
            && matches!(invocation.caller, AccountCaller::Local { .. })
        {
            self.consent
                .review(invocation.call, invocation.caller, request.review(invocation.caller))
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => super::remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        super::remote_authority_call(&cx, async {
            self.current_private_session(invocation.session)?;
            if let Some(keypair) = keypair
                && let CreateTransactionAuthorityRequest::Product(payload) = request
            {
                let metadata =
                    local_transaction_metadata(&self.chain, payload.genesis_hash).await?;
                let lifecycle = self.grants.lifecycle();
                lifecycle.require_revision(revision)?;
                self.current_private_session(invocation.session)?;
                return Ok(latest::HostCreateTransactionResponse {
                    transaction: build_signed_transaction(
                        &Sr25519Signer::from_keypair(&keypair),
                        payload.genesis_hash,
                        &payload.call_data,
                        &payload.extensions,
                        payload.tx_ext_version,
                        metadata,
                    )?,
                });
            }
            self.holder
                .create_transaction(
                    AccountInvocation {
                        call: &cx,
                        ..invocation
                    },
                    request,
                )
                .await
        })
        .await
    }

    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<latest::ContextualAlias, RingVrfError> {
        PairingHost::account_alias(self, invocation, request).await
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<latest::HostAccountCreateProofResponse, RingVrfError> {
        PairingHost::create_proof(self, invocation, request).await
    }

    async fn register_ring_vrf_key(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<[u8; 32], RingVrfError> {
        PairingHost::register_ring_vrf_key(
            self,
            invocation.call,
            invocation.session,
            ProductRequest {
                calling_product_id: invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?
                    .to_string(),
                payload: request,
            },
        )
        .await
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<latest::RegisteredRingVrfKey>, RingVrfError> {
        PairingHost::list_ring_vrf_keys(self, invocation, request).await
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        PairingHost::ring_vrf_sign(self, invocation, request).await
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        account: latest::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        PairingHost::sign_statement_store_product_payload(self, invocation, account, payload).await
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        self.holder.derive_entropy(session, product_id, context)
    }

    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        self.holder.contacts_handle_key(session)
    }
}
