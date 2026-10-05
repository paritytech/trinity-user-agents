//! Shared product-account capabilities and delegated execution.

mod lifecycle;
use lifecycle::HostLifecycle;

use super::RuntimeServices;
use super::authority::*;
use super::ring_vrf_registry::{RingVrfRegistryStore, validate_owner_listing};
use super::signing_host::ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};
use super::vrf;
use crate::host_internal::extrinsic::build_local_transaction;
use crate::host_internal::sso_messages::OnExistingAllowancePolicy;
use crate::host_internal::sso_messages::{ProductRequest, RingVrfError};
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::product_account::derive_ring_vrf_entropy_from_domain;
use crate::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair_from_subtree_secret,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::SessionInfo;
use crate::host_logic::session::SessionState;
use crate::platform::ProductContext;
use crate::platform::SecretCoreStorageKey;
use crate::platform::normalize_product_identifier;
use parity_scale_codec::{Decode, DecodeAll, Encode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use truapi::latest::*;
use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, latest as v01};
use zeroize::{Zeroize, Zeroizing};

/// Product host state shared by local and paired account-holder compositions.
pub struct HostAccounts<H: AccountHolder> {
    holder: Arc<H>,
    lifecycle: HostLifecycle,
    services: Arc<RuntimeServices>,
    ring_resolver: Arc<dyn RingResolver>,
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    storage_guard: futures::lock::Mutex<()>,
    generation: AtomicU64,
    wallet_authorizations: Mutex<HashMap<String, WalletAuthorization>>,
    product_subtrees: Mutex<HashMap<(Vec<u8>, String), [u8; 32]>>,
}

impl<H: AccountHolder + 'static> HostAccounts<H> {
    /// Build isolated host state over the selected account holder.
    fn new(holder: Arc<H>, services: Arc<RuntimeServices>, lifecycle: HostLifecycle) -> Arc<Self> {
        Arc::new(Self {
            ring_resolver: ChainRingResolver::new(services.chain.clone()),
            ring_vrf_registry: RingVrfRegistryStore::host(services.platform.clone()),
            holder,
            lifecycle,
            services,
            storage_guard: futures::lock::Mutex::new(()),
            generation: AtomicU64::new(0),
            wallet_authorizations: Mutex::new(HashMap::new()),
            product_subtrees: Mutex::new(HashMap::new()),
        })
    }
    /// A paired host approves before sending a request to its wallet.
    pub fn requires_host_confirmation(&self) -> bool {
        self.lifecycle
            .session_state()
            .current()
            .is_some_and(|session| session.sso.is_some())
    }

    /// Current session.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        self.holder.current_session()
    }

    /// Session state.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.lifecycle.session_state()
    }

    #[cfg(test)]
    /// Cache product subtree for test.
    pub fn cache_product_subtree_for_test(
        &self,
        _session: &SessionInfo,
        _product_id: &str,
        _public_key: [u8; 32],
    ) {
        if let Some(session) = self.current_session() {
            assert_eq!(session.public_key, _session.public_key);
            self.product_subtrees
                .lock()
                .expect("subtree cache mutex poisoned")
                .insert(
                    (session.validation_id, _product_id.to_string()),
                    _public_key,
                );
        }
    }

    /// Request login.
    pub async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        self.lifecycle.request_login(product).await
    }

    /// Disconnect.
    pub async fn disconnect(&self) -> Result<(), AuthorityError> {
        let paired_key = self
            .current_session()
            .and_then(|session| self.storage_key(&session).ok())
            .filter(|key| {
                matches!(
                    key,
                    SecretCoreStorageKey::HostAccountGrants {
                        session_id: Some(_),
                        ..
                    }
                )
            });
        self.generation.fetch_add(1, Ordering::SeqCst);
        for authorization in self
            .wallet_authorizations
            .lock()
            .expect("wallet authorizations mutex poisoned")
            .drain()
            .map(|(_, authorization)| authorization)
        {
            authorization.revoke();
        }
        self.product_subtrees
            .lock()
            .expect("product subtrees mutex poisoned")
            .clear();
        let disconnected = self.lifecycle.disconnect().await;
        let _guard = self.storage_guard.lock().await;
        if let Some(key) = paired_key {
            self.services
                .platform
                .clear_secret_core_storage(key)
                .await
                .map_err(storage_error)?;
        }
        disconnected
    }

    /// Refresh session identity.
    pub async fn refresh_session_identity(&self) -> Result<Option<AuthoritySession>, AuthorityError> {
        self.lifecycle.refresh_session_identity().await
    }

    /// Product subtree public key.
    pub async fn product_subtree_public_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let product_id = canonical_product(&product_id)?;
        let key = (session.validation_id.clone(), product_id.clone());
        if let Some(public) = self
            .product_subtrees
            .lock()
            .expect("subtree cache mutex poisoned")
            .get(&key)
        {
            return Ok(*public);
        }
        let private = self.current_private_session(session)?;
        if private.sso.is_some() {
            let _guard = self.storage_guard.lock().await;
            let stored = super::product_subtree::read_product_subtree(
                self.services.platform.as_ref(),
                &private,
                &product_id,
            )
            .await?;
            self.require_session(session, generation)?;
            if let Some(public) = stored {
                self.product_subtrees
                    .lock()
                    .expect("subtree cache mutex poisoned")
                    .insert(key, public);
                return Ok(public);
            }
        }
        let public = self
            .holder
            .product_subtree_public_key(cx, session, product_id.clone())
            .await?;
        if private.sso.is_some() {
            let _guard = self.storage_guard.lock().await;
            self.require_session(session, generation)?;
            super::product_subtree::write_product_subtree(
                self.services.platform.as_ref(),
                &private,
                &product_id,
                public,
            )
            .await?;
            if self.require_session(session, generation).is_err() {
                super::product_subtree::remove_product_subtree(
                    self.services.platform.as_ref(),
                    &private,
                    &product_id,
                )
                .await?;
                return Err(AuthorityError::Disconnected);
            }
        }
        self.require_session(session, generation)?;
        self.product_subtrees
            .lock()
            .expect("subtree cache mutex poisoned")
            .insert(key, public);
        Ok(public)
    }

    /// Subtree resolution reaches account holder.
    pub async fn subtree_resolution_reaches_account_holder(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        if self
            .product_subtrees
            .lock()
            .expect("subtree cache mutex poisoned")
            .contains_key(&(session.validation_id.clone(), product_id.to_string()))
        {
            return false;
        }
        if let Ok(private) = self.current_private_session(session)
            && private.sso.is_some()
            && matches!(
                super::product_subtree::read_product_subtree(
                    self.services.platform.as_ref(),
                    &private,
                    product_id
                )
                .await,
                Ok(Some(_))
            )
        {
            return false;
        }
        self.requires_host_confirmation()
    }

    /// Auto signing status.
    pub async fn auto_signing_status(
        &self,
        session: &AuthoritySession,
        calling_product_id: &str,
        account: &ProductAccountId,
    ) -> Result<AutoSigningGrant, AuthorityError> {
        self.require_session(session, self.generation.load(Ordering::SeqCst))?;
        if canonical_product(calling_product_id)? != canonical_product(&account.dot_ns_identifier)?
        {
            return Ok(AutoSigningGrant::Absent);
        }
        if self
            .signing_key(session, Some(calling_product_id), account)
            .await?
            .is_some()
            || self
                .wallet_session(
                    session,
                    Some(calling_product_id),
                    &account.dot_ns_identifier,
                )
                .authorizes(Some(calling_product_id), &account.dot_ns_identifier)
        {
            return Ok(AutoSigningGrant::Active);
        }
        Ok(if !self.requires_host_confirmation() && super::authority::is_blessed_owner(calling_product_id, &account.dot_ns_identifier) { AutoSigningGrant::Active } else { AutoSigningGrant::Absent })
    }

    /// Sign vrf.
    pub async fn sign_vrf(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: String,
        request: HostAccountSignVrfRequest,
    ) -> Result<VrfSignature, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        if let Some(keypair) = self
            .signing_key(session, Some(&caller), &request.account)
            .await?
        {
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            self.require_session(session, generation)?;
            return Ok(VrfSignature { pre_output, proof });
        }
        let wallet_session =
            self.wallet_session(session, Some(&caller), &request.account.dot_ns_identifier);
        self.require_session(session, generation)?;
        self.confirm_remote_route(
            cx,
            session,
            Some(&caller),
            crate::platform::UserConfirmationReview::SignVrf(crate::platform::SignVrfReview {
                calling_product_id: caller.clone(),
                request: request.clone(),
            }),
        )
        .await?;
        let result = self.holder
            .sign_vrf(cx, &wallet_session, caller, request)
            .await;
        self.require_session(session, generation)?;
        result
    }

    /// Sign payload.
    pub async fn sign_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: Option<&str>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<HostSignPayloadResponse, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let mut wallet_session = session.clone();
        if let SignPayloadAuthorityRequest::Product(payload) = &request {
            if let Some(keypair) = self.signing_key(session, caller, &payload.account).await? {
                self.require_session(session, generation)?;
                return Ok(sign_extrinsic_payload(&keypair, payload.payload.clone())?);
            }
            wallet_session =
                self.wallet_session(session, caller, &payload.account.dot_ns_identifier);
        }
        let review = match &request {
            SignPayloadAuthorityRequest::Product(request) => {
                crate::platform::SignPayloadReview::Product {
                    calling_product_id: caller.map(str::to_string),
                    request: request.clone(),
                }
            }
            SignPayloadAuthorityRequest::LegacyAccount { request, .. } => {
                crate::platform::SignPayloadReview::LegacyAccount(request.clone())
            }
        };
        self.require_session(session, generation)?;
        self.confirm_remote_route(
            cx,
            session,
            caller,
            crate::platform::UserConfirmationReview::SignPayload(review),
        )
        .await?;
        let result = self.holder
            .sign_payload(cx, &wallet_session, caller, request)
            .await;
        self.require_session(session, generation)?;
        result
    }

    /// Sign raw.
    pub async fn sign_raw(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: Option<&str>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<HostSignPayloadResponse, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let mut wallet_session = session.clone();
        if watermarked && let SignRawAuthorityRequest::Product(payload) = &request {
            if let Some(keypair) = self.signing_key(session, caller, &payload.account).await? {
                self.require_session(session, generation)?;
                let message = raw_payload_bytes(payload.payload.clone(), true)?;
                return Ok(HostSignPayloadResponse {
                    signature: keypair
                        .sign_simple(SR25519_SIGNING_CONTEXT, &message)
                        .to_bytes()
                        .to_vec(),
                    signed_transaction: None,
                });
            }
            wallet_session =
                self.wallet_session(session, caller, &payload.account.dot_ns_identifier);
        }
        let review = match &request {
            SignRawAuthorityRequest::Product(request) => crate::platform::SignRawReview::Product {
                calling_product_id: caller.map(str::to_string),
                request: request.clone(),
                watermarked,
            },
            SignRawAuthorityRequest::LegacyAccount { request, .. } => {
                crate::platform::SignRawReview::LegacyAccount {
                    request: request.clone(),
                    watermarked,
                }
            }
        };
        self.require_session(session, generation)?;
        self.confirm_remote_route(
            cx,
            session,
            caller,
            crate::platform::UserConfirmationReview::SignRaw(review),
        )
        .await?;
        let result = self.holder
            .sign_raw(cx, &wallet_session, caller, request, watermarked)
            .await;
        self.require_session(session, generation)?;
        result
    }

    /// Create transaction.
    pub async fn create_transaction(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: Option<&str>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<HostCreateTransactionResponse, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let mut wallet_session = session.clone();
        if let CreateTransactionAuthorityRequest::Product(payload) = &request {
            if let Some(keypair) = self.signing_key(session, caller, &payload.signer).await? {
                if !payload.contacts.is_empty()
                    && !caller.is_some_and(|caller| {
                        super::authority::is_blessed_owner(
                            caller,
                            &payload.signer.dot_ns_identifier,
                        )
                    })
                {
                    let review = crate::platform::UserConfirmationReview::CreateTransaction(
                        crate::platform::CreateTransactionReview::Product {
                            calling_product_id: caller.map(str::to_string),
                            payload: payload.clone(),
                        },
                    );
                    if !super::until_cancelled(
                        cx,
                        self.services.platform.confirm_user_action(review),
                    )
                    .await?
                    .map_err(storage_error)?
                    {
                        return Err(AuthorityError::Rejected);
                    }
                    self.require_session(session, generation)?;
                }
                let result = build_local_transaction(
                    &self.services.chain,
                    &keypair,
                    payload.genesis_hash,
                    &payload.call_data,
                    &payload.extensions,
                    payload.tx_ext_version,
                )
                .await?;
                self.require_session(session, generation)?;
                return Ok(result);
            }
            wallet_session =
                self.wallet_session(session, caller, &payload.signer.dot_ns_identifier);
        }
        let review = match &request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                crate::platform::CreateTransactionReview::Product {
                    calling_product_id: caller.map(str::to_string),
                    payload: payload.clone(),
                }
            }
            CreateTransactionAuthorityRequest::LegacyAccount { request, .. }
            | CreateTransactionAuthorityRequest::IdentityAccount(request) => {
                crate::platform::CreateTransactionReview::LegacyAccount(request.clone())
            }
        };
        self.require_session(session, generation)?;
        self.confirm_remote_route(
            cx,
            session,
            caller,
            crate::platform::UserConfirmationReview::CreateTransaction(review),
        )
        .await?;
        let result = self.holder
            .create_transaction(cx, &wallet_session, caller, request)
            .await;
        self.require_session(session, generation)?;
        result
    }

    /// Registered providers of the selected ring.
    pub async fn ring_vrf_providers(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        self.require_session(&session, generation)?;
        let result = self.ring_vrf_registry
            .providers(session.public_key, ring)
            .await;
        self.require_session(&session, generation)?;
        result
    }

    /// Selected product account for this ring.
    pub async fn selected_ring_vrf_provider(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        self.require_session(&session, generation)?;
        let result = self.ring_vrf_registry
            .selected_provider(session.public_key, ring)
            .await;
        self.require_session(&session, generation)?;
        result
    }

    /// Persist the chosen provider for this owner and ring.
    pub async fn select_ring_vrf_provider(
        &self,
        ring: v01::RingLocation,
        handle: v01::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        self.require_session(&session, generation)?;
        let _guard = self.storage_guard.lock().await;
        self.require_session(&session, generation)?;
        let result = self.ring_vrf_registry
            .select_provider(session.public_key, ring, handle)
            .await;
        self.require_session(&session, generation)?;
        result
    }

    /// Account alias.
    pub async fn account_alias(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountGetAliasRequest>,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let private_session = self.current_private_session(session)?;
        if request.calling_product_id == request.payload.key_handle.dot_ns_identifier
            && let Some(entropy) = self
                .local_ring_vrf_entropy_for_ring(
                    &private_session,
                    &request.payload.key_handle,
                    &request.payload.ring_location,
                )
                .await?
        {
            self.ring_resolver
                .validate(&request.payload.ring_location)
                .await?;
            let vrf = vrf::load().await?;
            self.require_session(session, generation)?;
            let context = development_context_bytes(&request.payload.context);
            let alias = vrf.alias(&entropy, &context)?;
            return Ok(v01::ContextualAlias {
                context,
                alias: alias.to_vec(),
            });
        }
        self.require_session(session, generation)?;
        let result = self.holder.account_alias(cx, session, request).await;
        self.require_session(session, generation)?;
        result
    }

    /// Create proof.
    pub async fn create_proof(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountCreateProofRequest>,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let (key_handle, access) = self
            .require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
            .await?;
        // A grant lets the caller act with the owner's key in the caller's own
        // context. It does not let it choose whose pseudonym to mint: the
        // contextual alias is a function of (owner key, context), so an
        // unconstrained context would let a grantee produce the alias the owner
        // presents to a third product that granted nothing. That third party
        // cannot consent here and is not a party to the grant.
        //
        // The owner's own calls are unaffected; a cross-product caller is held to
        // its own context or the granting product's.
        crate::runtime::product_manifest::require_own_context(&access, &request.payload.context)?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy_for_ring(
                &private_session,
                &key_handle,
                &request.payload.ring_location,
            )
            .await?
        {
            let vrf = vrf::load().await?;
            let member = vrf.member(&entropy)?;
            let resolved = self
                .ring_resolver
                .resolve(
                    &request.payload.ring_location,
                    &[MemberCandidate { member }],
                )
                .await?;
            self.require_session(session, generation)?;
            let context = development_context_bytes(&request.payload.context);
            let (proof, alias) = create_proof(
                &vrf,
                &entropy,
                &resolved,
                &context,
                &request.payload.message,
            )?;
            return Ok(v01::HostAccountCreateProofResponse {
                proof,
                contextual_alias: v01::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            });
        }
        self.require_session(session, generation)?;
        let result = self.holder.create_proof(cx, session, request).await;
        self.require_session(session, generation)?;
        result
    }

    /// Register ring vrf key.
    pub async fn register_ring_vrf_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<[u8; 32], RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let private_session = self.current_private_session(session)?;
        let handle = v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&request.calling_product_id).map_err(
                |error| RingVrfError::Unknown {
                    reason: error.to_string(),
                },
            )?,
            derivation_index: request.payload.index.clone(),
        };
        if let Some(auto_signing) = self
            .auto_signing_key(&private_session, &request.calling_product_id)
            .await
            .map_err(RingVrfError::from)?
        {
            self.ring_resolver.validate(&request.payload.ring).await?;
            let vrf = vrf::load().await?;
            self.require_session(session, generation)?;
            let entropy = Zeroizing::new(derive_ring_vrf_entropy_from_domain(
                auto_signing.ring_vrf_domain_entropy(),
                &request.payload.index,
            ));
            let public_key = vrf.member(&entropy)?;
            let _guard = self.storage_guard.lock().await;
            self.require_session(session, generation)?;
            self.ring_vrf_registry
                .register(
                    private_session.public_key,
                    handle,
                    request.payload.ring.clone(),
                    public_key,
                )
                .await?;
            self.require_session(session, generation)?;
            self.mirror_ring_vrf_registration(session.clone(), request);
            return Ok(public_key);
        }
        self.require_session(session, generation)?;
        let public_key = self
            .holder
            .register_ring_vrf_key(cx, session, request.clone())
            .await?;
        let _guard = self.storage_guard.lock().await;
        self.require_session(session, generation)?;
        self.ring_vrf_registry
            .register(
                private_session.public_key,
                handle,
                request.payload.ring,
                public_key,
            )
            .await?;
        self.require_session(session, generation)?;
        Ok(public_key)
    }

    /// List ring vrf keys.
    pub async fn list_ring_vrf_keys(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountListRingVrfKeysRequest>,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let private_session = self.current_private_session(session)?;
        let owner = normalize_product_identifier(&request.payload.owner).map_err(|error| {
            RingVrfError::Unknown {
                reason: error.to_string(),
            }
        })?;
        if private_session.sso.is_some() && request.calling_product_id == owner
            && let Some(mut entries) = self
                .ring_vrf_registry
                .complete_owner_entries(private_session.public_key, &owner)
                .await?
        {
            self.require_session(session, generation)?;
            apply_ring_vrf_disclosure(&mut entries, request.payload.disclosure);
            return Ok(entries);
        }
        let requested_disclosure = request.payload.disclosure;
        let mut remote_request = request;
        if remote_request.calling_product_id == owner {
            remote_request.payload.disclosure = v01::RingVrfKeyDisclosure::PublicKey;
        }
        self.require_session(session, generation)?;
        let mut entries = self
            .holder
            .list_ring_vrf_keys(cx, session, remote_request)
            .await?;
        validate_owner_listing(&owner, &entries)?;
        if entries.iter().all(|entry| entry.public_key.is_some()) {
            let _guard = self.storage_guard.lock().await;
            self.require_session(session, generation)?;
            entries = self
                .ring_vrf_registry
                .reconcile_owner(private_session.public_key, &owner, entries)
                .await?;
        }
        self.require_session(session, generation)?;
        apply_ring_vrf_disclosure(&mut entries, requested_disclosure);
        Ok(entries)
    }

    /// Ring vrf sign.
    pub async fn ring_vrf_sign(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRingVrfSignRequest>,
    ) -> Result<Vec<u8>, RingVrfError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let (key_handle, _access) = self
            .require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
            .await?;
        let private_session = self.current_private_session(session)?;
        if let Some(entropy) = self
            .local_ring_vrf_entropy(&private_session, &key_handle)
            .await?
        {
            let vrf = vrf::load().await?;
            self.require_session(session, generation)?;
            return vrf.sign(&entropy, &request.payload.message);
        }
        self.require_session(session, generation)?;
        let result = self.holder.ring_vrf_sign(cx, session, request).await;
        self.require_session(session, generation)?;
        result
    }

    /// Allocate resources.
    pub async fn allocate_resources(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        let product_id = canonical_product(&product_id)?;
        let resources = request.resources.clone();
        let outcomes = self
            .holder
            .allocate_grants(
                cx,
                session,
                product_id.clone(),
                request,
                OnExistingAllowancePolicy::Increase,
            )
            .await?;
        validate_allocations(&resources, &outcomes)?;
        let public_outcomes = outcomes
            .iter()
            .map(AccountAllocationOutcome::outcome)
            .collect();
        for outcome in outcomes {
            if let AccountAllocationOutcome::Allocated(grant) = outcome {
                self.retain_grant(cx, session, generation, &product_id, grant)
                    .await?;
            }
        }
        Ok(HostRequestResourceAllocationResponse {
            outcomes: public_outcomes,
        })
    }

    /// Statement store allowance key.
    pub async fn statement_store_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        match self
            .allowance(
                cx,
                session,
                &product_id,
                AllocatableResource::StatementStoreAllowance,
                false,
            )
            .await?
        {
            StoredMaterial::StatementStore(secret) => {
                StatementStoreAllowanceKey::from_secret_bytes(secret.0.clone())
            }
            _ => Err(invalid_grant()),
        }
    }

    /// Forget statement store allowance key.
    pub async fn forget_statement_store_allowance_key(
        &self,
        product_id: &str,
        public_key: [u8; 32],
    ) -> Result<(), AuthorityError> {
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        let _guard = self.storage_guard.lock().await;
        let key = self.storage_key(&session)?;
        let mut entries = self.read_records(key.clone()).await?;
        entries.retain(|entry| !(entry.product == product_id && matches!(&entry.material, StoredMaterial::StatementStore(secret) if StatementStoreAllowanceKey::from_secret_bytes(secret.0.clone()).is_ok_and(|key| key.public_key == public_key))));
        self.services
            .platform
            .write_secret_core_storage(key, entries.encode())
            .await
            .map_err(storage_error)
    }

    /// Bulletin allowance key.
    pub async fn bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        match self
            .allowance(
                cx,
                session,
                &product_id,
                AllocatableResource::BulletinAllowance,
                false,
            )
            .await?
        {
            StoredMaterial::Bulletin(secret) => {
                BulletinAllowanceKey::from_secret_bytes(secret.0.clone())
            }
            _ => Err(invalid_grant()),
        }
    }

    /// Refresh bulletin allowance key.
    pub async fn refresh_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        match self
            .allowance(
                cx,
                session,
                &product_id,
                AllocatableResource::BulletinAllowance,
                true,
            )
            .await?
        {
            StoredMaterial::Bulletin(secret) => {
                BulletinAllowanceKey::from_secret_bytes(secret.0.clone())
            }
            _ => Err(invalid_grant()),
        }
    }

    /// Sign statement store product payload.
    pub async fn sign_statement_store_product_payload(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: Option<&str>,
        account: ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        if let Some(keypair) = self.signing_key(session, caller, &account).await? {
            return Ok(keypair
                .sign_simple(SR25519_SIGNING_CONTEXT, &payload)
                .to_bytes());
        }
        let wallet_session = self.wallet_session(session, caller, &account.dot_ns_identifier);
        self.holder
            .sign_statement_store_product_payload(cx, &wallet_session, caller, account, payload)
            .await
    }

    /// Derive entropy.
    pub fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        self.holder.derive_entropy(session, product_id, context)
    }

    /// Contacts handle key.
    pub fn contacts_handle_key(
        &self,
        session: &AuthoritySession,
    ) -> Result<[u8; 32], AuthorityError> {
        self.holder.contacts_handle_key(session)
    }

    async fn require_ring_vrf_key_access(
        &self,
        calling_product_id: &str,
        handle: &v01::ProductAccountId,
    ) -> Result<
        (
            v01::ProductAccountId,
            crate::runtime::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
            &self.services,
            self.services.platform.as_ref(),
            calling_product_id,
            handle,
        )
        .await?;
        Ok((
            v01::ProductAccountId {
                dot_ns_identifier: access.owner.clone(),
                derivation_index: handle.derivation_index.clone(),
            },
            access,
        ))
    }

    async fn local_ring_vrf_entropy(
        &self,
        session: &SessionInfo,
        handle: &v01::ProductAccountId,
    ) -> Result<Option<Zeroizing<[u8; 32]>>, RingVrfError> {
        let Some(auto_signing) = self
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
        handle: &v01::ProductAccountId,
        ring: &v01::RingLocation,
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

    fn current_private_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        self.require_session(session, self.generation.load(Ordering::SeqCst))?;
        self.lifecycle
            .session_state()
            .current()
            .ok_or(AuthorityError::Disconnected)
    }

    async fn auto_signing_key(
        &self,
        private: &SessionInfo,
        product: &str,
    ) -> Result<Option<AutoSigningKey>, AuthorityError> {
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        if self.current_private_session(&session)? != *private {
            return Err(AuthorityError::Disconnected);
        }
        let Some(StoredMaterial::DelegatedSigning {
            secret,
            ring_entropy,
            public_key,
        }) = self
            .material(&session, product, &AllocatableResource::AutoSigning)
            .await?
        else {
            return Ok(None);
        };
        let secret: [u8; 64] = secret
            .0
            .as_slice()
            .try_into()
            .map_err(|_| invalid_grant())?;
        if schnorrkel::SecretKey::from_bytes(&secret)
            .map_err(|_| invalid_grant())?
            .to_public()
            .to_bytes()
            != public_key
        {
            return Err(invalid_grant());
        }
        Ok(Some(AutoSigningKey::from_parts(
            secret,
            ring_entropy
                .0
                .as_slice()
                .try_into()
                .map_err(|_| invalid_grant())?,
        )))
    }

    fn mirror_ring_vrf_registration(
        &self,
        session: AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) {
        let holder = self.holder.clone();
        (self.services.spawner)(Box::pin(async move {
            let cx = CallContext::with_request_id(format!(
                "ring-vrf-registration-mirror:{}",
                super::sso_remote::sso_message_id()
            ));
            if let Err(error) = holder.register_ring_vrf_key(&cx, &session, request).await {
                tracing::warn!(?error, "ring-VRF registration mirror failed");
            }
        }));
    }

    async fn confirm_remote_route(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        caller: Option<&str>,
        review: crate::platform::UserConfirmationReview,
    ) -> Result<(), AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.require_session(session, generation)?;
        if !self.requires_host_confirmation() {
            return Ok(());
        }
        let own_trusted_request = match &review {
            crate::platform::UserConfirmationReview::SignPayload(
                crate::platform::SignPayloadReview::Product { request, .. },
            ) => caller.is_some_and(|caller| {
                super::authority::is_blessed_owner(caller, &request.account.dot_ns_identifier)
            }),
            crate::platform::UserConfirmationReview::SignRaw(
                crate::platform::SignRawReview::Product {
                    request,
                    watermarked: true,
                    ..
                },
            ) => caller.is_some_and(|caller| {
                super::authority::is_blessed_owner(caller, &request.account.dot_ns_identifier)
            }),
            crate::platform::UserConfirmationReview::CreateTransaction(
                crate::platform::CreateTransactionReview::Product { payload, .. },
            ) => caller.is_some_and(|caller| {
                super::authority::is_blessed_owner(caller, &payload.signer.dot_ns_identifier)
            }),
            crate::platform::UserConfirmationReview::SignVrf(review) => {
                super::authority::is_blessed_owner(
                    &review.calling_product_id,
                    &review.request.account.dot_ns_identifier,
                )
            }
            _ => false,
        };
        if own_trusted_request {
            return Ok(());
        }
        let accepted =
            super::until_cancelled(cx, self.services.platform.confirm_user_action(review))
                .await?
                .map_err(|error| AuthorityError::HostFailure { reason: format!("signing confirmation failed: {}", error.reason) })?;
        self.require_session(session, generation)?;
        if accepted {
            Ok(())
        } else {
            Err(AuthorityError::Rejected)
        }
    }

    fn require_session(
        &self,
        session: &AuthoritySession,
        generation: u64,
    ) -> Result<(), AuthorityError> {
        if self.generation.load(Ordering::SeqCst) != generation
            || !self.holder.current_session().is_some_and(|current| {
                current.validation_id == session.validation_id
                    && current.public_key == session.public_key
            })
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    fn storage_key(
        &self,
        session: &AuthoritySession,
    ) -> Result<SecretCoreStorageKey, AuthorityError> {
        self.require_session(session, self.generation.load(Ordering::SeqCst))?;
        let current = self
            .lifecycle
            .session_state()
            .current()
            .ok_or(AuthorityError::Disconnected)?;
        Ok(SecretCoreStorageKey::HostAccountGrants {
            root_public_key: session.public_key,
            session_id: current
                .sso
                .as_ref()
                .map(super::allowances::session_storage_id),
        })
    }

    async fn read_records(
        &self,
        key: SecretCoreStorageKey,
    ) -> Result<Vec<StoredGrant>, AuthorityError> {
        let Some(bytes) = self
            .services
            .platform
            .read_secret_core_storage(key)
            .await
            .map_err(storage_error)?
        else {
            return Ok(Vec::new());
        };
        let bytes = Zeroizing::new(bytes);
        Vec::<StoredGrant>::decode_all(&mut bytes.as_slice()).map_err(|_| invalid_grant())
    }

    async fn material(
        &self,
        session: &AuthoritySession,
        product: &str,
        resource: &AllocatableResource,
    ) -> Result<Option<StoredMaterial>, AuthorityError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let _guard = self.storage_guard.lock().await;
        self.require_session(session, generation)?;
        let records = self.read_records(self.storage_key(session)?).await?;
        self.require_session(session, generation)?;
        Ok(records
            .into_iter()
            .find(|entry| entry.product == product && entry.material.matches(resource))
            .map(|entry| entry.material))
    }

    async fn retain_grant(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        generation: u64,
        product: &str,
        grant: AccountGrant,
    ) -> Result<(), AuthorityError> {
        self.require_session(session, generation)?;
        let material = match grant {
            AccountGrant::StatementStore(key) => {
                StoredMaterial::StatementStore(ProtectedBytes(key.secret.to_vec()))
            }
            AccountGrant::Bulletin(key) => {
                StoredMaterial::Bulletin(ProtectedBytes(key.as_secret_bytes().to_vec()))
            }
            AccountGrant::DelegatedSigning(key) => {
                let expected = self
                    .product_subtree_public_key(cx, session, product.to_string())
                    .await?;
                let secret = schnorrkel::SecretKey::from_bytes(key.as_secret_bytes())
                    .map_err(|_| invalid_grant())?;
                if secret.to_public().to_bytes() != expected {
                    return Err(invalid_grant());
                }
                StoredMaterial::DelegatedSigning {
                    secret: ProtectedBytes(key.as_secret_bytes().to_vec()),
                    ring_entropy: ProtectedBytes(key.ring_vrf_domain_entropy().to_vec()),
                    public_key: expected,
                }
            }
            AccountGrant::WalletAuthorization(authorization) => {
                self.require_session(session, generation)?;
                let mut authorizations = self.wallet_authorizations.lock().expect("wallet authorizations mutex poisoned");
                self.require_session(session, generation)?;
                authorizations.insert(product.to_string(), authorization);
                return Ok(());
            }
            AccountGrant::SmartContract => return Ok(()),
        };
        let _guard = self.storage_guard.lock().await;
        self.require_session(session, generation)?;
        let key = self.storage_key(session)?;
        let previous = self
            .services
            .platform
            .read_secret_core_storage(key.clone())
            .await
            .map_err(storage_error)?.map(Zeroizing::new);
        let mut records = match previous.as_ref() {
            Some(bytes) => Vec::<StoredGrant>::decode_all(&mut bytes.as_slice())
                .map_err(|_| invalid_grant())?,
            None => Vec::new(),
        };
        records
            .retain(|entry| entry.product != product || entry.material.kind() != material.kind());
        records.push(StoredGrant {
            product: product.to_string(),
            material,
        });
        self.require_session(session, generation)?;
        self.services
            .platform
            .write_secret_core_storage(key.clone(), records.encode())
            .await
            .map_err(storage_error)?;
        if self.require_session(session, generation).is_err() {
            match previous {
                Some(bytes) => self
                    .services
                    .platform
                    .write_secret_core_storage(key, bytes.to_vec())
                    .await
                    .map_err(storage_error)?,
                None => self
                    .services
                    .platform
                    .clear_secret_core_storage(key)
                    .await
                    .map_err(storage_error)?,
            }
            return Err(AuthorityError::Disconnected);
        }
        Ok(())
    }

    async fn allowance(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        product: &str,
        resource: AllocatableResource,
        refresh: bool,
    ) -> Result<StoredMaterial, AuthorityError> {
        let product = canonical_product(product)?;
        let generation = self.generation.load(Ordering::SeqCst);
        if !refresh && let Some(material) = self.material(session, &product, &resource).await? {
            self.holder
                .ensure_allowance_ready(session, &product, &resource)
                .await?;
            self.require_session(session, generation)?;
            return Ok(material);
        }
        let request = HostRequestResourceAllocationRequest {
            resources: vec![resource.clone()],
        };
        let policy = if refresh {
            OnExistingAllowancePolicy::Increase
        } else {
            OnExistingAllowancePolicy::Ignore
        };
        let outcomes = self
            .holder
            .allocate_grants(cx, session, product.clone(), request, policy)
            .await?;
        validate_allocations(&[resource.clone()], &outcomes)?;
        match outcomes.into_iter().next().ok_or_else(invalid_grant)? {
            AccountAllocationOutcome::Allocated(grant) => {
                self.retain_grant(cx, session, generation, &product, grant)
                    .await?
            }
            AccountAllocationOutcome::Rejected => return Err(AuthorityError::Rejected),
            AccountAllocationOutcome::NotAvailable { reason } => {
                return Err(AuthorityError::Unavailable {
                    reason: reason.unwrap_or_else(|| "wallet allowance is unavailable".to_string()),
                });
            }
        }
        self.material(session, &product, &resource)
            .await?
            .ok_or_else(invalid_grant)
    }

    fn wallet_session(
        &self,
        session: &AuthoritySession,
        caller: Option<&str>,
        owner: &str,
    ) -> AuthoritySession {
        if caller == Some(owner)
            && let Some(authorization) = self
                .wallet_authorizations
                .lock()
                .expect("wallet authorizations mutex poisoned")
                .get(owner)
        {
            return session.clone().with_authorization(authorization.clone());
        }
        session.clone()
    }

    async fn signing_key(
        &self,
        session: &AuthoritySession,
        caller: Option<&str>,
        account: &ProductAccountId,
    ) -> Result<Option<schnorrkel::Keypair>, AuthorityError> {
        if caller != Some(account.dot_ns_identifier.as_str()) {
            return Ok(None);
        }
        let Some(StoredMaterial::DelegatedSigning {
            secret, public_key, ..
        }) = self
            .material(
                session,
                &account.dot_ns_identifier,
                &AllocatableResource::AutoSigning,
            )
            .await?
        else {
            return Ok(None);
        };
        let secret: [u8; 64] = secret
            .0
            .as_slice()
            .try_into()
            .map_err(|_| invalid_grant())?;
        if schnorrkel::SecretKey::from_bytes(&secret)
            .map_err(|_| invalid_grant())?
            .to_public()
            .to_bytes()
            != public_key
        {
            return Err(invalid_grant());
        }
        derive_product_keypair_from_subtree_secret(
            secret,
            derivation_index_bytes(&account.derivation_index),
        )
        .map(Some)
        .map_err(|_| invalid_grant())
    }

    /// Remove this product's grants, fencing allocation results already in flight.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), AuthorityError> {
        let product = canonical_product(product_id)?;
        let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(authorization) = self
            .wallet_authorizations
            .lock()
            .expect("wallet authorizations mutex poisoned")
            .remove(&product)
        {
            authorization.revoke();
        }
        let _guard = self.storage_guard.lock().await;
        let key = self.storage_key(&session)?;
        let mut records = self.read_records(key.clone()).await?;
        records.retain(|entry| entry.product != product);
        self.product_subtrees
            .lock()
            .expect("subtree cache mutex poisoned")
            .retain(|(_, key), _| key != &product);
        let private = self.current_private_session(&session)?;
        if private.sso.is_some() {
            super::product_subtree::remove_product_subtree(
                self.services.platform.as_ref(),
                &private,
                &product,
            )
            .await?;
        }
        self.services
            .platform
            .write_secret_core_storage(key, records.encode())
            .await
            .map_err(storage_error)
    }

    /// Revoke account execution before deleting its protected native grant records.
    pub async fn reset_native_owner(&self, owner: [u8; 32]) -> Result<(), AuthorityError> {
        self.disconnect().await?;
        let _guard = self.storage_guard.lock().await;
        self.services
            .platform
            .clear_secret_core_storage(SecretCoreStorageKey::HostAccountGrants {
                root_public_key: owner,
                session_id: None,
            })
            .await
            .map_err(storage_error)
    }

    /// Remove every retained grant for the active owner and session.
    pub async fn reset_grants(&self) -> Result<(), AuthorityError> {
        let Some(session) = self.current_session() else {
            return Ok(());
        };
        self.generation.fetch_add(1, Ordering::SeqCst);
        for authorization in self
            .wallet_authorizations
            .lock()
            .expect("wallet authorizations mutex poisoned")
            .drain()
            .map(|(_, authorization)| authorization)
        {
            authorization.revoke();
        }
        let _guard = self.storage_guard.lock().await;
        self.services
            .platform
            .clear_secret_core_storage(self.storage_key(&session)?)
            .await
            .map_err(storage_error)
    }
}

#[derive(Clone, Encode, Decode)]
struct StoredGrant {
    product: String,
    material: StoredMaterial,
}

#[derive(Clone, Encode, Decode)]
enum StoredMaterial {
    StatementStore(ProtectedBytes),
    Bulletin(ProtectedBytes),
    DelegatedSigning {
        secret: ProtectedBytes,
        ring_entropy: ProtectedBytes,
        public_key: [u8; 32],
    },
}

impl StoredMaterial {
    fn kind(&self) -> u8 {
        match self {
            Self::StatementStore(_) => 0,
            Self::Bulletin(_) => 1,
            Self::DelegatedSigning { .. } => 2,
        }
    }
    fn matches(&self, resource: &AllocatableResource) -> bool {
        matches!(
            (self, resource),
            (
                Self::StatementStore(_),
                AllocatableResource::StatementStoreAllowance
            ) | (Self::Bulletin(_), AllocatableResource::BulletinAllowance)
                | (
                    Self::DelegatedSigning { .. },
                    AllocatableResource::AutoSigning
                )
        )
    }
}

fn canonical_product(product: &str) -> Result<String, AuthorityError> {
    crate::platform::normalize_product_identifier(product).map_err(|error| {
        AuthorityError::Unavailable {
            reason: error.to_string(),
        }
    })
}

fn invalid_grant() -> AuthorityError {
    AuthorityError::Unavailable {
        reason: "invalid protected host grant".to_string(),
    }
}
fn storage_error(error: GenericError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: error.reason,
    }
}

fn apply_ring_vrf_disclosure(
    entries: &mut [RegisteredRingVrfKey],
    disclosure: RingVrfKeyDisclosure,
) {
    if disclosure == RingVrfKeyDisclosure::Anonymized {
        for entry in entries {
            entry.public_key = None;
        }
    }
}

fn validate_allocations(
    resources: &[AllocatableResource],
    outcomes: &[AccountAllocationOutcome],
) -> Result<(), AuthorityError> {
    if resources.len() != outcomes.len() {
        return Err(invalid_grant());
    }
    for (resource, outcome) in resources.iter().zip(outcomes) {
        if let AccountAllocationOutcome::Allocated(grant) = outcome {
            let valid = matches!(
                (resource, grant),
                (
                    AllocatableResource::StatementStoreAllowance,
                    AccountGrant::StatementStore(_)
                ) | (
                    AllocatableResource::BulletinAllowance,
                    AccountGrant::Bulletin(_)
                ) | (
                    AllocatableResource::AutoSigning,
                    AccountGrant::DelegatedSigning(_) | AccountGrant::WalletAuthorization(_)
                ) | (
                    AllocatableResource::SmartContractAllowance(_),
                    AccountGrant::SmartContract
                )
            );
            if !valid {
                return Err(invalid_grant());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Encode, Decode)]
struct ProtectedBytes(Vec<u8>);

impl Drop for ProtectedBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl HostAccounts<super::WalletAccountHolder> {
    /// Compose native product accounts with the wallet activation lifecycle.
    pub fn native(holder: Arc<super::WalletAccountHolder>, services: Arc<RuntimeServices>) -> Arc<Self> {
        Self::new(holder.clone(), services, HostLifecycle::Native(holder))
    }
}

impl HostAccounts<super::SsoAccountHolderClient> {
    /// Compose the transport client with awaited capability invalidation.
    pub fn paired(holder: Arc<super::SsoAccountHolderClient>, services: Arc<RuntimeServices>) -> Arc<Self> {
        use futures::FutureExt;
        let accounts = Self::new(holder.clone(), services, HostLifecycle::Paired(holder.clone()));
        let weak = Arc::downgrade(&accounts);
        holder.set_session_cleanup(Arc::new(move |previous| {
            let weak = weak.clone();
            async move {
                let Some(accounts) = weak.upgrade() else { return Ok(()); };
                accounts.generation.fetch_add(1, Ordering::SeqCst);
                accounts.product_subtrees.lock().expect("subtree cache mutex poisoned").clear();
                let _guard = accounts.storage_guard.lock().await;
                if let Some(session) = previous.sso.as_ref() {
                    accounts.services.platform.clear_secret_core_storage(SecretCoreStorageKey::HostAccountGrants { root_public_key: previous.public_key, session_id: Some(super::allowances::session_storage_id(session)) }).await.map_err(storage_error)?;
                }
                Ok(())
            }.boxed()
        }));
        accounts
    }
}

#[cfg(test)]
impl HostAccounts<super::SsoAccountHolderClient> {
    /// Read a stored statement allowance without requesting a replacement.
    pub async fn cached_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
        product: &str,
    ) -> Result<Option<StatementStoreAllowanceKey>, AuthorityError> {
        let session = self.session_for_test(session, epoch)?;
        self.material(
            &session,
            product,
            &AllocatableResource::StatementStoreAllowance,
        )
        .await?
        .map(|material| match material {
            StoredMaterial::StatementStore(secret) => {
                StatementStoreAllowanceKey::from_secret_bytes(secret.0.clone())
            }
            _ => Err(invalid_grant()),
        })
        .transpose()
    }

    /// Read a stored Bulletin allowance without requesting a replacement.
    pub async fn cached_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
        product: &str,
    ) -> Result<Option<BulletinAllowanceKey>, AuthorityError> {
        let session = self.session_for_test(session, epoch)?;
        self.material(&session, product, &AllocatableResource::BulletinAllowance)
            .await?
            .map(|material| match material {
                StoredMaterial::Bulletin(secret) => {
                    BulletinAllowanceKey::from_secret_bytes(secret.0.clone())
                }
                _ => Err(invalid_grant()),
            })
            .transpose()
    }

    /// Pairing and host epochs captured by pending grants.
    pub fn current_session_lifecycle_epoch(&self) -> (u64, u64) {
        (
            self.holder.current_session_lifecycle_epoch(),
            self.generation.load(Ordering::SeqCst),
        )
    }

    fn session_for_test(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
    ) -> Result<AuthoritySession, AuthorityError> {
        if self.current_session_lifecycle_epoch() != epoch
            || self.lifecycle.session_state().current().as_ref() != Some(session)
        {
            return Err(AuthorityError::Disconnected);
        }
        self.current_session().ok_or(AuthorityError::Disconnected)
    }

    /// Seed an authenticated delegated grant through production validation.
    pub async fn remember_auto_signing_key_for_tests(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
        product: &str,
        expected_public: [u8; 32],
        secret: [u8; 64],
        domain: [u8; 32],
    ) -> Result<(), AuthorityError> {
        let current = self.session_for_test(session, epoch)?;
        self.cache_product_subtree_for_test(session, product, expected_public);
        self.retain_grant(
            &CallContext::default(),
            &current,
            self.generation.load(Ordering::SeqCst),
            product,
            AccountGrant::DelegatedSigning(AutoSigningKey::from_parts(secret, domain)),
        )
        .await
    }

    /// Inspect whether protected delegated material remains usable.
    pub async fn has_auto_signing_key_for_tests(
        &self,
        session: &SessionInfo,
        product: &str,
    ) -> Result<bool, AuthorityError> {
        self.auto_signing_key(session, product)
            .await
            .map(|key| key.is_some())
    }

    /// Seed a statement grant while preserving production lifecycle checks.
    pub async fn cache_statement_store_allowance_key(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
        product: &str,
        secret: Vec<u8>,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        let current = self.session_for_test(session, epoch)?;
        let key = StatementStoreAllowanceKey::from_secret_bytes(secret)?;
        self.retain_grant(
            &CallContext::default(),
            &current,
            self.generation.load(Ordering::SeqCst),
            product,
            AccountGrant::StatementStore(key.clone()),
        )
        .await?;
        Ok(key)
    }

    /// Seed a Bulletin grant while preserving production lifecycle checks.
    pub async fn cache_bulletin_allowance_key(
        &self,
        session: &SessionInfo,
        epoch: (u64, u64),
        product: &str,
        secret: Vec<u8>,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        let current = self.session_for_test(session, epoch)?;
        let key = BulletinAllowanceKey::from_secret_bytes(secret)?;
        self.retain_grant(
            &CallContext::default(),
            &current,
            self.generation.load(Ordering::SeqCst),
            product,
            AccountGrant::Bulletin(key.clone()),
        )
        .await?;
        Ok(key)
    }

    /// Seed a public registration for a delegated signing operation.
    pub async fn register_ring_vrf_key_for_tests(
        &self,
        session: &SessionInfo,
        handle: ProductAccountId,
        ring: RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        self.ring_vrf_registry
            .register(session.public_key, handle, ring, public_key)
            .await
    }
}

#[cfg(test)]
mod tests;
