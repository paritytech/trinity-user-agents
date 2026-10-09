//! Wallet account execution and consent.

use super::{AllowanceAllocationError, WalletAccountHolder, product_authority_error};
use crate::host_internal::extrinsic::Sr25519Signer;
use crate::host_internal::extrinsic::{build_signed_transaction, local_transaction_metadata};
use crate::host_internal::sso_messages::OnExistingAllowancePolicy;
use crate::host_internal::sso_messages::RingVrfError;
use crate::host_internal::transaction::sign_extrinsic_payload;
use crate::host_logic::features::genesis_for;
use crate::host_logic::product_account::{SR25519_SIGNING_CONTEXT, personhood_product_id};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::platform::{
    PermissionAuthorizationStatus, SignVrfReview, StatementStoreProductSignReview,
    UserConfirmationReview, normalize_product_identifier,
};
use crate::platform::ResourceAllocationReview;
use crate::runtime::authority::{
    AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
    AuthorityError, AuthoritySession, AutoSigningGrant, AutoSigningKey, BulletinAllowanceKey,
    CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    StatementStoreAllowanceKey,
};
use crate::runtime::signing_host::ring_vrf::{
    MemberCandidate, create_proof, development_context_bytes,
};
use crate::runtime::statement_allowance::collection::PersonhoodCollection;
use crate::runtime::vrf::{self, Vrf};
use crate::runtime::{WalletAuthorization, allowances::AllowanceResource};
use crate::runtime::{
    remote_authority_call, remote_authority_context, until_cancelled, validate_vrf_transcript,
};
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use std::sync::Arc;
use truapi::latest as api;
use truapi::latest::{
    ChainIdentifier, DerivationIndex, HostAccountCreateProofRequest, HostAccountGetAliasRequest,
    HostAccountListRingVrfKeysRequest, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRingVrfSignRequest, ProductAccountId, RingLocation, RingLocationJunction,
};

impl WalletAccountHolder {
    /// Derive the product's hard-subtree public key from the active session root.
    /// Returns `None` when no session is active.
    pub fn derive_subtree_public_key(
        &self,
        product_id: &str,
    ) -> Result<Option<[u8; 32]>, AuthorityError> {
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        let Some(session) = self.current_session() else {
            return Ok(None);
        };
        self.with_keys(&session, |keys| {
            keys.product_subtree_public_key(&product_id)
        })
        .map(Some)
    }
}

impl WalletAccountHolder {
    fn transaction_keypair(
        keys: &super::WalletKeys,
        request: &CreateTransactionAuthorityRequest,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        match request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                keys.product_keypair(&payload.signer)
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => {
                let keypair = keys.product_keypair(product_account)?;
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unknown { reason: "signing host: legacy signer does not match the product slot-zero account".to_string() });
                }
                Ok(keypair)
            }
            CreateTransactionAuthorityRequest::IdentityAccount(request) => {
                let keypair = keys.identity_keypair()?;
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unavailable { reason: "signing host: the requested identity account is not available in this CLI wallet".to_string() });
                }
                Ok(keypair)
            }
        }
    }

    fn invocation_auto_signing(
        &self,
        invocation: &AccountInvocation<'_>,
        account: &ProductAccountId,
    ) -> Result<bool, AuthorityError> {
        match invocation.caller {
            AccountCaller::Local {
                product,
                authorization,
                ..
            } => Ok(self.auto_signing_status(
                invocation.session,
                &product.product_id,
                account,
                authorization,
            )? == AutoSigningGrant::Active),
            AccountCaller::Remote { .. } => Ok(false),
        }
    }

    async fn register_builtin_personhood_keys_if_needed(
        &self,
        session: &AuthoritySession,
        owner: &str,
    ) -> Result<(), RingVrfError> {
        if owner != personhood_product_id(&self.network_suffix) {
            return Ok(());
        }
        let chains = self
            .services
            .platform
            .supported_chains()
            .await
            .map_err(|error| RingVrfError::Unknown {
                reason: error.reason,
            })?;
        let chain_id =
            genesis_for(&chains, ChainIdentifier::People).ok_or(RingVrfError::RingNotFound)?;
        let entries = self
            .ring_vrf_registry
            .owner_entries(session.public_key, owner)
            .await?;
        let missing = [
            (PersonhoodCollection::People, 0),
            (PersonhoodCollection::LitePeople, 1),
        ]
        .into_iter()
        .filter(|(collection, index)| {
            !entries.iter().any(|entry| {
                entry.handle.derivation_index == DerivationIndex::Index(*index)
                    && entry.rings.iter().any(|ring| {
                        ring.chain_id == chain_id
                            && matches!(
                                ring.junctions.as_slice(),
                                [RingLocationJunction::PalletInstance(_), RingLocationJunction::CollectionId(identifier)]
                                    if identifier.as_slice() == collection.identifier()
                            )
                    })
            })
        })
        .collect::<Vec<_>>();
        if missing.is_empty() {
            self.require_current_session(session)?;
            return Ok(());
        }
        let pallet_index = self.ring_resolver.members_pallet_index(&chain_id).await?;
        let vrf = vrf::load().await?;
        for (collection, index) in missing {
            let handle = ProductAccountId {
                dot_ns_identifier: owner.to_string(),
                derivation_index: DerivationIndex::Index(index),
            };
            let public_key = self.with_keys(session, |keys| {
                vrf.member(&*keys.ring_vrf_entropy(&handle)?)
            })?;
            let ring = RingLocation {
                chain_id,
                junctions: vec![
                    RingLocationJunction::PalletInstance(pallet_index),
                    RingLocationJunction::CollectionId(collection.identifier().to_vec()),
                ],
            };
            let mut update = self
                .ring_vrf_registry
                .prepare_update(session.public_key)
                .await?;
            self.require_current_session(session)?;
            update.register(handle, ring, public_key)?;
            update.persist().await?;
            self.require_current_session(session)?;
        }
        Ok(())
    }

    async fn registered_ring_vrf_entry(
        &self,
        session: &AuthoritySession,
        handle: &api::ProductAccountId,
    ) -> Result<Option<api::RegisteredRingVrfKey>, RingVrfError> {
        self.require_current_session(session)?;
        let entry = self
            .ring_vrf_registry
            .entry(session.public_key, handle)
            .await?;
        self.require_current_session(session)?;
        Ok(entry)
    }

    async fn registered_ring_vrf_entry_for_ring(
        &self,
        session: &AuthoritySession,
        handle: &api::ProductAccountId,
        ring: &api::RingLocation,
    ) -> Result<api::RegisteredRingVrfKey, RingVrfError> {
        let entry = self
            .registered_ring_vrf_entry(session, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if !entry.rings.contains(ring) {
            return Err(RingVrfError::KeyNotInRing);
        }
        Ok(entry)
    }

    fn require_matching_registered_public_key(
        vrf: &Vrf,
        entry: &api::RegisteredRingVrfKey,
        entropy: &[u8; 32],
    ) -> Result<(), RingVrfError> {
        if entry.public_key != Some(vrf.member(entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the active wallet"
                    .to_string(),
            });
        }
        Ok(())
    }

    /// Keep access checks and key derivation bound to the same canonical owner.
    async fn require_ring_vrf_key_access(
        &self,
        caller: AccountCaller<'_>,
        handle: &api::ProductAccountId,
    ) -> Result<
        (
            api::ProductAccountId,
            crate::runtime::product_manifest::AuthorizedAccess,
        ),
        RingVrfError,
    > {
        let access = crate::runtime::product_manifest::ring_vrf_key_access_granted(
            &self.services,
            self.services.platform.as_ref(),
            caller.product_id().ok_or(RingVrfError::Rejected)?,
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

    async fn require_account_access(
        &self,
        invocation: &AccountInvocation<'_>,
        requester: &str,
        owner: &str,
    ) -> Result<(), RingVrfError> {
        let status = until_cancelled(
            invocation.call,
            self.consent.account_access(requester, owner),
        )
        .await?
        .map_err(|error| RingVrfError::Unknown {
            reason: error.to_string(),
        })?;
        if status == PermissionAuthorizationStatus::Authorized {
            Ok(())
        } else {
            Err(RingVrfError::Rejected)
        }
    }
}

#[async_trait::async_trait]
impl AccountHolder for WalletAccountHolder {
    fn current_session(&self) -> Option<AuthoritySession> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        let session = self.session_state.current()?;
        state.keys.as_ref()?;
        Some(state.session(&session))
    }

    fn require_current_session(&self, session: &AuthoritySession) -> Result<(), AuthorityError> {
        self.lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned")
            .require_session(self.session_state.current(), session)
            .map(|_| ())
    }

    async fn allocate_grants<'a>(
        &'a self,
        invocation: AccountInvocation<'a>,
        request: api::HostRequestResourceAllocationRequest,
        policy: OnExistingAllowancePolicy,
    ) -> Result<BoxStream<'a, Result<AccountGrantOutcome, AuthorityError>>, AuthorityError> {
        let caller = invocation
            .caller
            .product_id()
            .ok_or(AuthorityError::Rejected)?;
        self.consent
            .review(
                invocation.call,
                invocation.caller,
                UserConfirmationReview::ResourceAllocation(ResourceAllocationReview {
                    calling_product_id: caller.to_string(),
                    resources: request.resources.clone(),
                }),
            )
            .await?;
        self.require_current_session(invocation.session)?;
        let product_id = caller.to_string();
        Ok(stream::unfold(
            (request.resources.into_iter(), product_id, invocation),
            move |(mut resources, product_id, invocation)| async move {
                let resource = resources.next()?;
                let outcome = async {
                    self.require_current_session(invocation.session)?;
                    if let Some(reason) = invocation.call.cancel().reason() {
                        return Err(crate::runtime::authority_cancellation_error(
                            invocation.call,
                            reason,
                        )
                        .into());
                    }
                    #[cfg(feature = "test-host")]
                    if matches!(invocation.caller, AccountCaller::Local { .. }) {
                        self.resource_controls.refuse_withheld(&resource)?;
                    }
                    let product_id = product_id.as_str();
                    let grant = match resource {
                        api::AllocatableResource::StatementStoreAllowance => {
                            let allocation = self
                                .allocate_statement_store_allowance(
                                    invocation.session,
                                    product_id,
                                    policy,
                                )
                                .await?;
                            AccountGrant::StatementStore {
                                key: StatementStoreAllowanceKey::from_secret_bytes(
                                    allocation.secret,
                                )?,
                                period: Some(allocation.period),
                            }
                        }
                        api::AllocatableResource::BulletinAllowance => {
                            AccountGrant::Bulletin(BulletinAllowanceKey::from_secret_bytes(
                                self.allocate_bulletin_allowance(
                                    invocation.session,
                                    product_id,
                                    policy,
                                )
                                .await?,
                            )?)
                        }
                        api::AllocatableResource::SmartContractAllowance(index) => {
                            self.allocate_smart_contract_allowance(
                                invocation.session,
                                product_id,
                                index,
                                policy,
                            )
                            .await?;
                            AccountGrant::SmartContract
                        }
                        api::AllocatableResource::AutoSigning => self
                            .with_keys::<_, AuthorityError>(invocation.session, |keys| {
                                Ok(match invocation.caller {
                                    AccountCaller::Local { .. } => {
                                        let product_id = normalize_product_identifier(product_id)
                                            .map_err(|error| {
                                            AuthorityError::Unavailable {
                                                reason: error.to_string(),
                                            }
                                        })?;
                                        keys.product_subtree_public_key(&product_id)?;
                                        AccountGrant::WalletAuthorization(WalletAuthorization {
                                            issuer: Arc::downgrade(&self.session_state),
                                            validation_id: invocation.session.validation_id.clone(),
                                            product_id,
                                        })
                                    }
                                    AccountCaller::Remote { .. } => {
                                        AccountGrant::AutoSigning(AutoSigningKey::from_parts(
                                            keys.product_subtree_secret(product_id)?,
                                            keys.ring_vrf_domain_entropy(product_id)
                                                .map_err(product_authority_error)?,
                                        ))
                                    }
                                })
                            })?,
                    };
                    Ok(grant)
                }
                .await;
                let outcome = self
                    .require_current_session(invocation.session)
                    .map_err(AllowanceAllocationError::from)
                    .and(outcome);
                let outcome = match outcome {
                    Ok(grant) => Ok(AccountGrantOutcome::Allocated(grant)),
                    Err(AllowanceAllocationError::Authority(
                        error @ (AuthorityError::Disconnected | AuthorityError::Cancelled(_)),
                    )) => Err(error),
                    Err(AllowanceAllocationError::Authority(AuthorityError::Rejected)) => {
                        Ok(AccountGrantOutcome::Rejected)
                    }
                    Err(error) => Ok(AccountGrantOutcome::NotAvailable {
                        reason: Some(error.to_string()),
                    }),
                };
                Some((outcome, (resources, product_id, invocation)))
            },
        )
        .boxed())
    }

    async fn renew_statement_sponsorship(
        &self,
        invocation: AccountInvocation<'_>,
        account_id: [u8; 32],
    ) -> Result<(), AuthorityError> {
        let AccountCaller::Local { product, .. } = invocation.caller else {
            return Err(AuthorityError::Rejected);
        };
        self.require_current_session(invocation.session)?;
        #[cfg(feature = "test-host")]
        self.resource_controls
            .refuse_withheld(&api::AllocatableResource::StatementStoreAllowance)?;
        until_cancelled(
            invocation.call,
            self.ensure_statement_slot(
                invocation.session,
                &product.product_id,
                account_id,
                OnExistingAllowancePolicy::Ignore,
            ),
        )
        .await?
        .map_err(AllowanceAllocationError::into_authority_error)?;
        self.require_current_session(invocation.session)
    }

    async fn ensure_allowance(
        &self,
        invocation: AccountInvocation<'_>,
        resource: AllowanceResource,
        policy: OnExistingAllowancePolicy,
    ) -> Result<AccountGrant, AuthorityError> {
        let resource = match resource {
            AllowanceResource::StatementStore => api::AllocatableResource::StatementStoreAllowance,
            AllowanceResource::Bulletin => api::AllocatableResource::BulletinAllowance,
        };
        let mut grants = self
            .allocate_grants(
                invocation,
                api::HostRequestResourceAllocationRequest {
                    resources: vec![resource],
                },
                policy,
            )
            .await?;
        match grants.next().await.transpose()? {
            Some(AccountGrantOutcome::Allocated(grant)) => Ok(grant),
            Some(AccountGrantOutcome::NotAvailable { reason }) => {
                Err(AuthorityError::Unavailable {
                    reason: reason.unwrap_or_else(|| "Allowance unavailable".to_string()),
                })
            }
            Some(AccountGrantOutcome::Rejected) | None => Err(AuthorityError::Rejected),
        }
    }

    async fn product_subtree_public_key<'a>(
        &'a self,
        invocation: AccountInvocation<'a>,
        product_id: String,
    ) -> Result<futures::future::BoxFuture<'a, Result<[u8; 32], AuthorityError>>, AuthorityError>
    {
        self.require_current_session(invocation.session)?;
        let product_id = normalize_product_identifier(&product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        Ok(Box::pin(async move {
            self.with_keys(invocation.session, |keys| {
                keys.product_subtree_public_key(&product_id)
            })
        }))
    }

    async fn sign_vrf(
        &self,
        invocation: AccountInvocation<'_>,
        request: api::HostAccountSignVrfRequest,
    ) -> Result<api::VrfSignature, AuthorityError> {
        let session = invocation.session;
        let calling_product_id = invocation
            .caller
            .product_id()
            .ok_or(AuthorityError::Rejected)?;
        self.require_current_session(session)?;
        validate_vrf_transcript(&request).map_err(|reason| AuthorityError::Unknown { reason })?;
        self.with_keys(session, |keys| {
            keys.product_keypair(&request.account).map(|_| ())
        })?;
        if !self.invocation_auto_signing(&invocation, &request.account)? {
            self.consent
                .review(
                    invocation.call,
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
        self.with_keys(session, |keys| {
            let keypair = keys.product_keypair(&request.account)?;
            let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
                &keypair,
                &request.transcript_label,
                request
                    .items
                    .iter()
                    .map(|item| (item.label.as_slice(), item.value.as_slice())),
            );
            Ok(api::VrfSignature { pre_output, proof })
        })
    }

    async fn sign_payload(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        self.require_current_session(invocation.session)?;
        let granted = match &request {
            SignPayloadAuthorityRequest::Product(request) => {
                self.invocation_auto_signing(&invocation, &request.account)?
            }
            _ => false,
        };
        if !granted {
            self.consent
                .review(
                    invocation.call,
                    invocation.caller,
                    request.review(invocation.caller),
                )
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        remote_authority_call(&cx, async {
            self.with_keys(invocation.session, |keys| {
                let (account, payload) = match request {
                    SignPayloadAuthorityRequest::Product(request) => {
                        (request.account, request.payload)
                    }
                    SignPayloadAuthorityRequest::LegacyAccount {
                        product_account,
                        request,
                    } => (product_account, request.payload),
                };
                Ok(sign_extrinsic_payload(
                    &keys.product_keypair(&account)?,
                    payload,
                )?)
            })
        })
        .await
    }

    async fn sign_raw(
        &self,
        invocation: AccountInvocation<'_>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<api::HostSignPayloadResponse, AuthorityError> {
        self.require_current_session(invocation.session)?;
        let granted = match &request {
            SignRawAuthorityRequest::Product(request) if watermarked => {
                self.invocation_auto_signing(&invocation, &request.account)?
            }
            _ => false,
        };
        if !granted {
            self.consent
                .review(
                    invocation.call,
                    invocation.caller,
                    request.review(invocation.caller, watermarked),
                )
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        remote_authority_call(&cx, async {
            self.with_keys(invocation.session, |keys| {
                let (keypair, payload) = match request {
                    SignRawAuthorityRequest::Product(request) => (keys.product_keypair(&request.account)?, request.payload),
                    SignRawAuthorityRequest::LegacyAccount { product_account, request } => (keys.product_keypair(&product_account)?, request.payload),
                    SignRawAuthorityRequest::IdentityAccount { account, request } => {
                        let keypair = keys.identity_keypair()?;
                        if keypair.public.to_bytes() != account {
                            return Err(AuthorityError::Unavailable { reason: "signing host: the requested legacy account is not available in this CLI wallet".to_string() });
                        }
                        (keypair, request.payload)
                    }
                };
                let message = raw_payload_bytes(payload, watermarked)?;
                let signature = keypair.secret.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public).to_bytes();
                Ok(api::HostSignPayloadResponse { signature: signature.to_vec(), signed_transaction: None })
            })
        }).await
    }

    async fn create_transaction(
        &self,
        invocation: AccountInvocation<'_>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<api::HostCreateTransactionResponse, AuthorityError> {
        self.require_current_session(invocation.session)?;
        let granted = match &request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                self.invocation_auto_signing(&invocation, &payload.signer)?
                    && payload.contacts.is_empty()
            }
            _ => false,
        };
        if !granted {
            self.consent
                .review(
                    invocation.call,
                    invocation.caller,
                    request.review(invocation.caller),
                )
                .await?;
        }
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        remote_authority_call(&cx, async {
            self.with_keys(invocation.session, |keys| {
                Self::transaction_keypair(keys, &request).map(|_| ())
            })?;
            let (genesis_hash, call_data, extensions, tx_ext_version) = match &request {
                CreateTransactionAuthorityRequest::Product(payload) => (
                    payload.genesis_hash,
                    &payload.call_data,
                    &payload.extensions,
                    payload.tx_ext_version,
                ),
                CreateTransactionAuthorityRequest::LegacyAccount { request, .. }
                | CreateTransactionAuthorityRequest::IdentityAccount(request) => (
                    request.genesis_hash,
                    &request.call_data,
                    &request.extensions,
                    request.tx_ext_version,
                ),
            };
            let metadata = local_transaction_metadata(&self.services.chain, genesis_hash).await?;
            self.with_keys(invocation.session, |keys| {
                let keypair = Self::transaction_keypair(keys, &request)?;
                Ok(api::HostCreateTransactionResponse {
                    transaction: build_signed_transaction(
                        &Sr25519Signer::from_keypair(&keypair),
                        genesis_hash,
                        call_data,
                        extensions,
                        tx_ext_version,
                        metadata,
                    )?,
                })
            })
        })
        .await
    }

    async fn account_alias(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountGetAliasRequest,
    ) -> Result<api::ContextualAlias, RingVrfError> {
        let session = invocation.session;
        self.require_current_session(session)?;
        // Aliases expose the same identity as `create_proof`, so both must enforce
        // the same RFC-0024 grant and context restrictions.
        let granted = match self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await
        {
            Ok(granted) => Some(granted),
            Err(RingVrfError::NotAllowlisted) => None,
            Err(err) => return Err(err),
        };
        let key_handle = match granted {
            Some((key_handle, access)) => {
                crate::runtime::product_manifest::require_own_context(&access, &request.context)?;
                key_handle
            }
            None => {
                // Permission lookups and prompts must use the same canonical identifiers.
                let requester = normalize_product_identifier(
                    invocation
                        .caller
                        .product_id()
                        .ok_or(RingVrfError::NotAllowlisted)?,
                )
                .map_err(|_| RingVrfError::NotAllowlisted)?;
                let owner = normalize_product_identifier(&request.key_handle.dot_ns_identifier)
                    .map_err(|_| RingVrfError::NotAllowlisted)?;
                self.require_account_access(&invocation, &requester, &owner)
                    .await?;
                api::ProductAccountId {
                    dot_ns_identifier: owner,
                    derivation_index: request.key_handle.derivation_index.clone(),
                }
            }
        };
        let vrf = vrf::load().await?;
        let entry = self
            .registered_ring_vrf_entry_for_ring(session, &key_handle, &request.ring_location)
            .await?;
        self.ring_resolver.validate(&request.ring_location).await?;
        let context = development_context_bytes(&request.context);
        self.with_keys(session, |keys| {
            let entropy = keys.ring_vrf_entropy(&key_handle)?;
            Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
            let alias = vrf.alias(&entropy, &context)?;
            Ok(api::ContextualAlias {
                context,
                alias: alias.to_vec(),
            })
        })
    }

    async fn create_proof(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountCreateProofRequest,
    ) -> Result<api::HostAccountCreateProofResponse, RingVrfError> {
        let session = invocation.session;
        self.require_current_session(session)?;
        let (key_handle, access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        // A grant must not expose the owner's alias in an unrelated product's context.
        crate::runtime::product_manifest::require_own_context(&access, &request.context)?;
        let vrf = vrf::load().await?;
        let entry = self
            .registered_ring_vrf_entry_for_ring(session, &key_handle, &request.ring_location)
            .await?;
        let candidate = self.with_keys(session, |keys| {
            let entropy = keys.ring_vrf_entropy(&key_handle)?;
            Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
            Ok::<_, RingVrfError>(MemberCandidate {
                member: vrf.member(&entropy)?,
            })
        })?;
        let resolved = self
            .ring_resolver
            .resolve(&request.ring_location, &[candidate])
            .await?;
        self.with_keys(session, |keys| {
            let entropy = keys.ring_vrf_entropy(&key_handle)?;
            let context = development_context_bytes(&request.context);
            let (proof, alias) =
                create_proof(&vrf, &entropy, &resolved, &context, &request.message)?;
            Ok(api::HostAccountCreateProofResponse {
                proof,
                contextual_alias: api::ContextualAlias {
                    context,
                    alias: alias.to_vec(),
                },
                ring_index: resolved.ring_index,
                ring_revision: resolved.ring_revision,
            })
        })
    }

    async fn register_ring_vrf_key(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<[u8; 32], RingVrfError> {
        let session = invocation.session;
        self.require_current_session(session)?;
        self.ring_resolver.validate(&request.ring).await?;

        let handle = api::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(
                invocation
                    .caller
                    .product_id()
                    .ok_or(RingVrfError::NotAllowlisted)?,
            )
            .map_err(|err| RingVrfError::Unknown {
                reason: err.to_string(),
            })?,
            derivation_index: request.index,
        };
        let vrf = vrf::load().await?;
        let public_key = self.with_keys(session, |keys| {
            vrf.member(&*keys.ring_vrf_entropy(&handle)?)
        })?;
        let mut update = self
            .ring_vrf_registry
            .prepare_update(session.public_key)
            .await?;
        self.require_current_session(session)?;
        update.register(handle, request.ring, public_key)?;
        update.persist().await?;
        self.require_current_session(session)?;
        Ok(public_key)
    }

    async fn list_ring_vrf_keys(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountListRingVrfKeysRequest,
    ) -> Result<Vec<api::RegisteredRingVrfKey>, RingVrfError> {
        let session = invocation.session;
        self.require_current_session(session)?;
        let owner =
            normalize_product_identifier(&request.owner).map_err(|err| RingVrfError::Unknown {
                reason: err.to_string(),
            })?;
        // Ownership checks and permission decisions must use canonical identifiers.
        let caller = normalize_product_identifier(
            invocation
                .caller
                .product_id()
                .ok_or(RingVrfError::NotAllowlisted)?,
        )
        .map_err(|_| RingVrfError::NotAllowlisted)?;
        if caller != owner {
            self.require_account_access(&invocation, &caller, &owner)
                .await?;
        }

        self.register_builtin_personhood_keys_if_needed(session, &owner)
            .await?;
        let mut entries = self
            .ring_vrf_registry
            .owner_entries(session.public_key, &owner)
            .await?;
        self.require_current_session(session)?;
        if request.disclosure == api::RingVrfKeyDisclosure::Anonymized {
            for entry in &mut entries {
                entry.public_key = None;
            }
        }
        Ok(entries)
    }

    async fn ring_vrf_sign(
        &self,
        invocation: AccountInvocation<'_>,
        request: HostAccountRingVrfSignRequest,
    ) -> Result<Vec<u8>, RingVrfError> {
        let session = invocation.session;
        self.require_current_session(session)?;
        let (key_handle, _access) = self
            .require_ring_vrf_key_access(invocation.caller, &request.key_handle)
            .await?;
        let vrf = vrf::load().await?;
        let entry = self
            .registered_ring_vrf_entry(session, &key_handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        self.with_keys(session, |keys| {
            let entropy = keys.ring_vrf_entropy(&key_handle)?;
            Self::require_matching_registered_public_key(&vrf, &entry, &entropy)?;
            vrf.sign(&entropy, &request.message)
        })
    }

    async fn sign_statement_store_product_payload(
        &self,
        invocation: AccountInvocation<'_>,
        account: api::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.consent
            .review(
                invocation.call,
                invocation.caller,
                UserConfirmationReview::StatementStoreProductSign(StatementStoreProductSignReview {
                    calling_product_id: invocation.caller.product_id().map(str::to_string),
                    account: account.clone(),
                    payload: payload.clone(),
                }),
            )
            .await?;
        let cx = match invocation.caller {
            AccountCaller::Local { .. } => remote_authority_context(invocation.call),
            AccountCaller::Remote { .. } => invocation.call.clone(),
        };
        remote_authority_call(&cx, async {
            self.with_keys(invocation.session, |keys| {
                let keypair = keys.product_keypair(&account)?;
                Ok(keypair
                    .secret
                    .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
                    .to_bytes())
            })
        })
        .await
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        self.with_keys(session, |keys| keys.derive_entropy(product_id, context))
    }

    fn contacts_handle_key(&self, session: &AuthoritySession) -> Result<[u8; 32], AuthorityError> {
        self.with_keys(session, |keys| Ok(keys.contacts_handle_key()))
    }
}
