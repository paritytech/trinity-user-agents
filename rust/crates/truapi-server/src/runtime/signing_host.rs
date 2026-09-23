//! Signing-host role for wallet-local account authority.
//!
//! A signing host owns the user's keys and serves authority requests locally,
//! with no pairing flow and no SSO channel. Secret material is provided by the
//! embedding host at unlock through [`LocalActivation::activate_local_session`]
//! (the host owns its persistence, e.g. the OS keychain) and kept in memory
//! for the session, zeroized on disconnect.
//!
//! Implemented: local session lifecycle, raw-bytes signing, extrinsic-payload
//! signing, v4 transaction construction (payload fields and extensions arrive
//! pre-encoded, so no chain metadata is needed), RFC-0007 product entropy,
//! bandersnatch ring-VRF aliases and membership proofs, and product-scoped
//! Statement Store allowance keys (native and browser), and Bulletin allowance
//! keys (native only).
mod allowance_renewal;
mod local_activation;
mod local_identity;
pub(super) mod ring_vrf;
mod sso_replay;
mod sso_responder;
mod sso_service;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use truapi::latest::{
    HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
    HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
};

#[cfg(not(target_arch = "wasm32"))]
pub use allowance_renewal::{StatementRenewalTarget, TrackedStatementRenewalTarget};
pub(crate) use local_activation::LocalActivation;
pub use local_identity::{LocalIdentity, LocalIdentityContext};
pub use sso_responder::{PairedSsoPeer, ResponderExit};
pub(crate) use sso_responder::{
    disconnect_paired_host, establish_pairing, respond_to_pairing, resume_pairing,
};
pub(crate) use sso_service::SigningHostSsoService;

use super::authority::{
    AuthorityError, AuthoritySession, AutoSigningGrant, BulletinAllowanceKey,
    CreateTransactionAuthorityRequest, PaymentTopUpAuthorityError, ProductAuthority,
    ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
    SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
    authority_session_validation_id,
};
use super::native_chat::{NativeChatContext, NativeChatRegistry};
use super::ring_vrf_registry::RingVrfRegistryStore;
use super::{RuntimeServices, connected_session_ui_info, validate_vrf_transcript};
use crate::host_logic::entropy::derive_product_entropy;
use crate::host_logic::extrinsic::build_local_transaction;
use crate::host_logic::product_account::{
    ProductAccountError, SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_identity_keypair,
    derive_product_keypair, derive_product_subtree_keypair, derive_ring_vrf_entropy,
    derive_root_keypair_from_entropy,
};
use crate::host_logic::product_account::{
    derive_full_person_ring_vrf_entropy, derive_lite_person_ring_vrf_entropy,
};
use crate::host_logic::raw_signing::raw_payload_bytes;
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::sso::messages::{
    OnExistingAllowancePolicy, PaymentTopUpRequest, ProductRequest, RingVrfError,
};
use crate::host_logic::transaction::sign_extrinsic_payload;
use crate::runtime::auth_state::AuthStateMachine;
use crate::runtime::statement_allowance::CollectionCandidate;
use crate::runtime::statement_allowance::collection::PersonhoodCollection;
use ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, alias_from_entropy, create_proof,
    development_context_bytes, member_from_entropy, sign_from_entropy,
};
use sso_replay::SsoReplayLocks;

/// The network suffix the unit tests configure their signing host for. `dot`
/// keeps the `peopl.dot` handles the RFC examples use meaningful; the
/// per-network behaviour has its own tests.
#[cfg(test)]
const TEST_NETWORK_SUFFIX: &str = "dot";

use truapi::versioned::account::{HostRequestLoginError, HostRequestLoginResponse};
use truapi::{CallContext, CallError, v01};
use truapi_platform::{
    PermissionAuthorizationStatus, Platform, ProductContext, SignVrfReview, UserConfirmationReview,
    normalize_product_identifier,
};
use zeroize::Zeroizing;

#[derive(Default)]
struct LocalGrantState {
    activation_generation: u64,
    auto_signing_grants: HashSet<([u8; 32], String)>,
}

impl LocalGrantState {
    fn advance_activation(&mut self) {
        self.activation_generation = self
            .activation_generation
            .checked_add(1)
            .expect("local activation generation exhausted");
        self.auto_signing_grants.clear();
    }

    fn revoke_product(&mut self, product_id: &str) {
        self.activation_generation = self
            .activation_generation
            .checked_add(1)
            .expect("local activation generation exhausted");
        self.auto_signing_grants
            .retain(|(_, granted_product_id)| granted_product_id != product_id);
    }
}

/// Wallet-local account authority for a signing host.
pub(crate) struct SigningHost {
    services: Arc<RuntimeServices>,
    platform: Arc<dyn Platform>,
    /// The dotNS TLD of the network this wallet serves, from
    /// [`truapi_platform::SigningHostConfig::network_suffix`]. Every reserved
    /// RFC-0022 derivation (`uid.<suffix>`, `peopl.<suffix>`) ends in it.
    network_suffix: String,
    coinage_instance_id: Option<u32>,
    session_state: Arc<SessionState>,
    auth_state: AuthStateMachine,
    ring_resolver: Arc<dyn RingResolver>,
    /// Root BIP-39 entropy held only while a session is active.
    root_entropy: Mutex<Option<Zeroizing<Vec<u8>>>>,
    /// In-memory grants and the activation generation that owns them. The
    /// lifecycle mutex also makes session replacement and snapshot creation
    /// atomic with respect to generation changes.
    local_grants: Arc<Mutex<LocalGrantState>>,
    /// Durable RFC-0024 registry, scoped by the active wallet root.
    ring_vrf_registry: Arc<RingVrfRegistryStore>,
    /// Serializes replay-ledger updates within each wallet and peer scope.
    sso_replay_locks: SsoReplayLocks,
    /// Wallet/network engine and product-scoped Host-only transport devices.
    native_chat: NativeChatRegistry,
    renewal: allowance_renewal::RenewalState,
}

impl Drop for SigningHost {
    fn drop(&mut self) {
        self.clear_local_session();
    }
}

impl SigningHost {
    /// Build a signing host with no active session, serving the network whose
    /// dotNS TLD is `network_suffix`.
    pub(crate) fn new(
        services: Arc<RuntimeServices>,
        network_suffix: String,
        coinage_instance_id: Option<u32>,
    ) -> Arc<Self> {
        let platform = services.platform.clone();
        let ring_resolver = ChainRingResolver::new(services.chain.clone());
        Arc::new(Self {
            services,
            platform: platform.clone(),
            network_suffix,
            coinage_instance_id,
            session_state: SessionState::new(),
            auth_state: AuthStateMachine::new(platform.clone()),
            ring_resolver,
            root_entropy: Mutex::new(None),
            local_grants: Arc::new(Mutex::new(LocalGrantState::default())),
            ring_vrf_registry: RingVrfRegistryStore::new(platform),
            sso_replay_locks: SsoReplayLocks::default(),
            native_chat: NativeChatRegistry::default(),
            renewal: allowance_renewal::RenewalState::default(),
        })
    }

    /// The shared services this role was built over, for tests that also need
    /// to build a product runtime against the same platform and cache.
    #[cfg(test)]
    fn services(&self) -> Arc<RuntimeServices> {
        self.services.clone()
    }

    #[cfg(test)]
    fn new_with_ring_resolver(
        platform: Arc<dyn Platform>,
        ring_resolver: Arc<dyn RingResolver>,
    ) -> Arc<Self> {
        Self::new_with_ring_resolver_on(platform, ring_resolver, TEST_NETWORK_SUFFIX)
    }

    #[cfg(test)]
    fn new_with_ring_resolver_on(
        platform: Arc<dyn Platform>,
        ring_resolver: Arc<dyn RingResolver>,
        network_suffix: &str,
    ) -> Arc<Self> {
        let services = RuntimeServices::new(
            platform.clone(),
            truapi_platform::HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: None,
                version: None,
                platform: truapi::latest::HostPlatform::Unknown,
            },
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            crate::test_support::test_spawner(),
        );
        Arc::new(Self {
            services,
            platform: platform.clone(),
            network_suffix: network_suffix.to_string(),
            coinage_instance_id: None,
            session_state: SessionState::new(),
            auth_state: AuthStateMachine::new(platform.clone()),
            ring_resolver,
            root_entropy: Mutex::new(None),
            local_grants: Arc::new(Mutex::new(LocalGrantState::default())),
            ring_vrf_registry: RingVrfRegistryStore::new(platform),
            sso_replay_locks: SsoReplayLocks::default(),
            native_chat: NativeChatRegistry::default(),
            renewal: allowance_renewal::RenewalState::default(),
        })
    }

    /// Shared session holder for connection-status subscriptions.
    pub(super) fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// The dotNS TLD of the network this wallet serves: the suffix of every
    /// reserved identity it derives.
    pub(super) fn network_suffix(&self) -> &str {
        &self.network_suffix
    }

    /// Current root entropy, or [`AuthorityError::Disconnected`] when no local
    /// session is active.
    fn root_entropy(&self) -> Result<Zeroizing<Vec<u8>>, AuthorityError> {
        self.root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned")
            .clone()
            .ok_or(AuthorityError::Disconnected)
    }

    fn product_subtree_secret(&self, product_id: &str) -> Result<[u8; 64], AuthorityError> {
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id)
            .map(|keypair| keypair.secret.to_bytes())
            .map_err(product_authority_error)
    }

    fn sso_replay_locks(&self) -> &SsoReplayLocks {
        &self.sso_replay_locks
    }

    fn grant_auto_signing(
        &self,
        session: &AuthoritySession,
        product_id: &str,
    ) -> Result<(), AuthorityError> {
        let (_, activation_generation) = self.require_current_session(session)?;
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let owner = root.public.to_bytes();
        if owner != session.public_key {
            return Err(AuthorityError::Disconnected);
        }
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id).map_err(product_authority_error)?;

        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if state.activation_generation != activation_generation {
            return Err(AuthorityError::Disconnected);
        }
        state.auto_signing_grants.insert((owner, product_id));
        Ok(())
    }

    fn has_auto_signing_grant(
        &self,
        activation_generation: u64,
        owner: [u8; 32],
        calling_product_id: &str,
        account_product_id: &str,
    ) -> bool {
        let (Ok(calling_product_id), Ok(account_product_id)) = (
            normalize_product_identifier(calling_product_id),
            normalize_product_identifier(account_product_id),
        ) else {
            return false;
        };
        if calling_product_id != account_product_id {
            return false;
        }

        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.activation_generation == activation_generation
            && state
                .auto_signing_grants
                .contains(&(owner, calling_product_id))
    }

    /// Fence in-flight grant work and revoke this product's grants from the
    /// current local activation while preserving unrelated products.
    pub(crate) async fn clear_product_state(&self, product_id: &str) -> Result<(), AuthorityError> {
        let product_id = normalize_product_identifier(product_id).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        self.local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned")
            .revoke_product(&product_id);
        if let Some(session) = self.current_local_session() {
            let context = self.native_chat_context(&session).map_err(|error| {
                AuthorityError::Unavailable {
                    reason: format!("native Chat product state unavailable: {error:?}"),
                }
            })?;
            let forgotten = self.native_chat.forget_product(&context, &product_id).await;
            // The registry preserves wallet custody and unrelated product authority.
            forgotten.map_err(|_| AuthorityError::Unavailable {
                reason: "native Chat product state could not be cleared".into(),
            })?;
        }
        Ok(())
    }

    /// Derive the product-account keypair for `account` from the root entropy.
    ///
    /// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
    /// `substrate-bip39`) rather than cached: the signing host holds only the
    /// raw, zeroizable entropy, never an expanded secret key.
    fn product_keypair_with_owner(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<([u8; 32], schnorrkel::Keypair), AuthorityError> {
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        let owner = root.public.to_bytes();
        let product_id =
            normalize_product_identifier(&account.dot_ns_identifier).map_err(|err| {
                AuthorityError::Unavailable {
                    reason: err.to_string(),
                }
            })?;
        derive_product_keypair(
            &root,
            &product_id,
            derivation_index_bytes(&account.derivation_index),
        )
        .map(|keypair| (owner, keypair))
        .map_err(product_authority_error)
    }

    fn product_keypair(
        &self,
        account: &v01::ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        self.product_keypair_with_owner(account)
            .map(|(_, keypair)| keypair)
    }

    fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
        let entropy = self.root_entropy()?;
        derive_identity_keypair(&entropy, &self.network_suffix).map_err(product_authority_error)
    }

    fn install_local_session(&self, secret: Zeroizing<Vec<u8>>, session: SessionInfo) {
        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.advance_activation();
        *self
            .root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned") = Some(secret);
        self.session_state.set_session(session);
        drop(state);
        self.native_chat.release();
        if let Some(session) = self.current_local_session() {
            if let Ok(context) = self.native_chat_context(&session) {
                self.native_chat.resume_wallet_recovery(context);
            }
        }
    }

    fn clear_local_session(&self) {
        let mut state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        state.advance_activation();
        self.root_entropy
            .lock()
            .expect("signing host entropy mutex poisoned")
            .take();
        self.session_state.clear_session();
        drop(state);
        self.native_chat.release();
    }

    fn current_local_session(&self) -> Option<AuthoritySession> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let session = self.session_state.current()?;
        Some(AuthoritySession::from_session_info(
            &session,
            local_session_validation_id(&session, state.activation_generation),
        ))
    }

    fn require_current_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<(SessionInfo, u64), AuthorityError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let current = self
            .session_state
            .current()
            .ok_or(AuthorityError::Disconnected)?;
        if local_session_validation_id(&current, state.activation_generation)
            != session.validation_id
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok((current, state.activation_generation))
    }

    fn native_chat_context(
        &self,
        session: &AuthoritySession,
    ) -> Result<NativeChatContext, ProductDeviceChatAuthorityError> {
        self.require_current_session(session)?;
        let entropy = self.root_entropy()?;
        self.require_current_session(session)?;
        let session_state = self.session_state.clone();
        let local_grants = self.local_grants.clone();
        let validation_id = session.validation_id.clone();
        let session_valid = Arc::new(move || {
            let grants = local_grants
                .lock()
                .expect("local AutoSigning grant mutex poisoned");
            session_state.current().is_some_and(|current| {
                local_session_validation_id(&current, grants.activation_generation) == validation_id
            })
        });
        Ok(NativeChatContext {
            services: self.services.clone(),
            session: session.clone(),
            entropy,
            session_valid,
            network_suffix: self.network_suffix.clone(),
            genesis_hash: self.services.people_chain_genesis_hash,
            coinage_instance_id: self.coinage_instance_id,
        })
    }

    fn ring_vrf_entropy(
        &self,
        session: &AuthoritySession,
        handle: &v01::ProductAccountId,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        self.require_current_session(session)?;
        let root = self.root_entropy()?;
        derive_ring_vrf_entropy(&root, &handle.dot_ns_identifier, &handle.derivation_index)
            .map(Zeroizing::new)
            .map_err(|err| RingVrfError::Unknown {
                reason: err.to_string(),
            })
    }

    /// Every personhood collection this wallet can derive allowance aliases for,
    /// widest slot budget first.
    ///
    /// Wallet-internal allowance proofs use the reserved `peopl.<suffix>` keys
    /// the mobile hosts derive on the same network. Product-facing RFC-0024
    /// operations are unrelated: those resolve only explicitly registered
    /// handles.
    ///
    /// Both entropies are always returned; which collections the person is
    /// actually a member of is settled on chain by looking for a ring that
    /// includes each member key, not by local state. That keeps the two hosts
    /// from disagreeing about personhood.
    fn reserved_person_collection_candidates(
        &self,
        session: &AuthoritySession,
    ) -> Result<Vec<CollectionCandidate>, AuthorityError> {
        self.require_current_session(session)?;
        let root = self.root_entropy()?;
        Ok(vec![
            CollectionCandidate {
                collection: PersonhoodCollection::People,
                entropy: derive_full_person_ring_vrf_entropy(&root, &self.network_suffix),
            },
            CollectionCandidate {
                collection: PersonhoodCollection::LitePeople,
                entropy: derive_lite_person_ring_vrf_entropy(&root, &self.network_suffix),
            },
        ])
    }

    async fn registered_ring_vrf_entry(
        &self,
        session: &AuthoritySession,
        handle: &v01::ProductAccountId,
    ) -> Result<Option<v01::RegisteredRingVrfKey>, RingVrfError> {
        self.require_current_session(session)?;
        self.ring_vrf_registry
            .entry(session.public_key, handle)
            .await
    }

    async fn resolve_ring_vrf_key_for_ring(
        &self,
        session: &AuthoritySession,
        handle: &v01::ProductAccountId,
        ring: &v01::RingLocation,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        let entry = self
            .registered_ring_vrf_entry(session, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        if !entry.rings.contains(ring) {
            return Err(RingVrfError::KeyNotInRing);
        }
        let entropy = self.ring_vrf_entropy(session, handle)?;
        Self::require_matching_registered_public_key(&entry, &entropy)?;
        Ok(entropy)
    }

    async fn resolve_registered_ring_vrf_key(
        &self,
        session: &AuthoritySession,
        handle: &v01::ProductAccountId,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        let entry = self
            .registered_ring_vrf_entry(session, handle)
            .await?
            .ok_or(RingVrfError::KeyNotRegistered)?;
        let entropy = self.ring_vrf_entropy(session, handle)?;
        Self::require_matching_registered_public_key(&entry, &entropy)?;
        Ok(entropy)
    }

    fn require_matching_registered_public_key(
        entry: &v01::RegisteredRingVrfKey,
        entropy: &[u8; 32],
    ) -> Result<(), RingVrfError> {
        if entry.public_key != Some(member_from_entropy(entropy)?) {
            return Err(RingVrfError::Unknown {
                reason: "registered ring-VRF public key does not match the active wallet"
                    .to_string(),
            });
        }
        Ok(())
    }

    fn ring_vrf_member_candidate(
        &self,
        entropy: &[u8; 32],
    ) -> Result<MemberCandidate, RingVrfError> {
        Ok(MemberCandidate {
            member: member_from_entropy(entropy)?,
        })
    }

    /// Whether `calling_product_id` may act on `handle`'s ring-VRF key.
    ///
    /// Delegates to [`crate::runtime::ring_vrf_key_access_granted`], which
    /// resolves the owner's manifest here rather than trusting the request: on
    /// this role the request can have arrived over the pairing wire.
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
            self.platform.as_ref(),
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

    pub(crate) async fn ring_vrf_providers(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Vec<v01::ProductAccountId>, RingVrfError> {
        let session = self.current_local_session().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .providers(session.public_key, ring)
            .await
    }

    pub(crate) async fn selected_ring_vrf_provider(
        &self,
        ring: &v01::RingLocation,
    ) -> Result<Option<v01::ProductAccountId>, RingVrfError> {
        let session = self.current_local_session().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .selected_provider(session.public_key, ring)
            .await
    }

    pub(crate) async fn select_ring_vrf_provider(
        &self,
        ring: v01::RingLocation,
        handle: v01::ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let session = self.current_local_session().ok_or(RingVrfError::Unknown {
            reason: "no active session".to_string(),
        })?;
        self.ring_vrf_registry
            .select_provider(session.public_key, ring, handle)
            .await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SigningHost {
    /// Record statement-store accounts to keep renewed across periods.
    pub(crate) async fn track_statement_renewal_targets(
        &self,
        targets: Vec<StatementRenewalTarget>,
    ) -> Result<(), String> {
        allowance_renewal::track(self, targets).await
    }

    /// Every statement account the ledger currently tracks.
    pub(crate) async fn statement_renewal_targets(
        &self,
    ) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
        allowance_renewal::list(self).await
    }

    /// Root public key the active identity records its fixed entries under.
    pub(crate) fn statement_renewal_owner_key(&self) -> Result<[u8; 32], String> {
        allowance_renewal::active_owner_key(self)
    }

    /// Stop renewing one fixed statement account.
    pub(crate) async fn untrack_statement_renewal_account(
        &self,
        account_id: &[u8; 32],
    ) -> Result<bool, String> {
        allowance_renewal::untrack_account_for_signing_host(self, account_id).await
    }

    /// Run one statement-store renewal pass over the tracked targets.
    pub(crate) async fn renew_statement_allowances(
        &self,
    ) -> Result<crate::runtime::statement_allowance::renewal::StatementRenewalReport, String> {
        allowance_renewal::renew_now(&self.services, self).await
    }

    /// The most recent pass the in-process loop ran.
    ///
    /// A direct call to [`Self::renew_statement_allowances`] returns its own
    /// report, so only the loop needs somewhere to leave one.
    pub(crate) fn last_statement_renewal_report(
        &self,
    ) -> Option<crate::runtime::statement_allowance::renewal::StatementRenewalReport> {
        self.renewal.last_report()
    }

    /// Start the periodic statement-store renewal loop. Idempotent.
    pub(crate) fn start_statement_allowance_renewal(self: &Arc<Self>) {
        allowance_renewal::start_renewal_loop(&self.services, self);
    }
}

#[async_trait::async_trait]
impl ProductAuthority for SigningHost {
    fn current_session(&self) -> Option<AuthoritySession> {
        self.current_local_session()
    }

    async fn refresh_session_identity(&self) -> Option<AuthoritySession> {
        let context = self.local_identity_context().ok()?;
        if let Err(error) = self.refresh_local_identity(&context.activation_id).await {
            tracing::warn!(reason = %error.reason, "local dotNS identity refresh failed");
        }
        if self.local_identity_context().ok()?.activation_id != context.activation_id {
            return None;
        }
        self.current_local_session()
    }

    fn session_state(&self) -> Arc<SessionState> {
        SigningHost::session_state(self)
    }

    async fn request_login(
        &self,
        _product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if let Some(session) = self.session_state.current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ))
        } else {
            // The host activates a local session out of band once the wallet
            // is unlocked; there is no in-core login prompt to drive.
            Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::Rejected,
            ))
        }
    }

    async fn disconnect(&self) {
        self.clear_local_session();
        self.auth_state.store_disconnected();
    }

    async fn product_subtree_public_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<[u8; 32], AuthorityError> {
        self.require_current_session(session)?;
        let product_id = normalize_product_identifier(&product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        let entropy = self.root_entropy()?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(product_authority_error)?;
        derive_product_subtree_keypair(&root, &product_id)
            .map(|keypair| keypair.public.to_bytes())
            .map_err(product_authority_error)
    }

    async fn subtree_resolution_reaches_account_holder(
        &self,
        _session: &AuthoritySession,
        _product_id: &str,
    ) -> bool {
        // A signing host derives the subtree locally from root entropy, so
        // resolution never reaches a remote Account Holder and never prompts.
        false
    }

    async fn auto_signing_status(
        &self,
        session: &AuthoritySession,
        calling_product_id: &str,
        account: &v01::ProductAccountId,
    ) -> Result<AutoSigningGrant, AuthorityError> {
        // A stale session is not a grant, and is answered here rather than
        // raising a prompt against a session that no longer exists.
        // `grant_auto_signing` refuses to record a grant whose owner is not
        // the session's own key, so the session carries the owner a grant can
        // be keyed on and no root derivation is needed to answer this.
        let (current, activation_generation) = self.require_current_session(session)?;
        if self.has_auto_signing_grant(
            activation_generation,
            current.public_key,
            calling_product_id,
            &account.dot_ns_identifier,
        ) {
            Ok(AutoSigningGrant::Active)
        } else {
            Ok(AutoSigningGrant::Absent)
        }
    }

    async fn sign_vrf(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        calling_product_id: String,
        request: v01::HostAccountSignVrfRequest,
    ) -> Result<v01::VrfSignature, AuthorityError> {
        let (_, activation_generation) = self.require_current_session(session)?;
        validate_vrf_transcript(&request).map_err(|reason| AuthorityError::Unknown { reason })?;
        let (owner, keypair) = self.product_keypair_with_owner(&request.account)?;
        if !self.has_auto_signing_grant(
            activation_generation,
            owner,
            &calling_product_id,
            &request.account.dot_ns_identifier,
        ) {
            let confirmed = self
                .platform
                .confirm_user_action(UserConfirmationReview::SignVrf(SignVrfReview {
                    calling_product_id,
                    request: request.clone(),
                }))
                .await
                .map_err(|err| AuthorityError::Unknown {
                    reason: format!("VRF signing confirmation failed: {err:?}"),
                })?;
            if !confirmed {
                return Err(AuthorityError::Rejected);
            }
        }
        let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
            &keypair,
            &request.transcript_label,
            request
                .items
                .iter()
                .map(|item| (item.label.as_slice(), item.value.as_slice())),
        );
        Ok(v01::VrfSignature { pre_output, proof })
    }

    async fn sign_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: SignPayloadAuthorityRequest,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        self.require_current_session(session)?;
        let (keypair, payload) = match request {
            SignPayloadAuthorityRequest::Product(request) => {
                (self.product_keypair(&request.account)?, request.payload)
            }
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => (self.product_keypair(&product_account)?, request.payload),
        };
        Ok(sign_extrinsic_payload(&keypair, payload)?)
    }

    async fn sign_raw(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: SignRawAuthorityRequest,
        watermarked: bool,
    ) -> Result<v01::HostSignPayloadResponse, AuthorityError> {
        let (keypair, payload) = match request {
            SignRawAuthorityRequest::Product(request) => {
                (self.product_keypair(&request.account)?, request.payload)
            }
            SignRawAuthorityRequest::LegacyAccount { account, request } => {
                let keypair = self.identity_keypair()?;
                if keypair.public.to_bytes() != account {
                    return Err(AuthorityError::Unavailable {
                        reason: "signing host: the requested legacy account is not available in \
                                 this CLI wallet"
                            .to_string(),
                    });
                }
                (keypair, request.payload)
            }
        };
        self.require_current_session(session)?;
        let message = raw_payload_bytes(payload, watermarked)?;
        let signature = keypair
            .secret
            .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
            .to_bytes();
        Ok(v01::HostSignPayloadResponse {
            signature: signature.to_vec(),
            signed_transaction: None,
        })
    }

    async fn create_transaction(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        request: CreateTransactionAuthorityRequest,
    ) -> Result<v01::HostCreateTransactionResponse, AuthorityError> {
        self.require_current_session(session)?;
        match request {
            CreateTransactionAuthorityRequest::Product(payload) => {
                // The product account is authoritative and caller-scoping is
                // enforced upstream, so the derived key defines the signer.
                let keypair = self.product_keypair(&payload.signer)?;
                build_local_transaction(
                    &self.services.chain,
                    &keypair,
                    payload.genesis_hash,
                    &payload.call_data,
                    &payload.extensions,
                    payload.tx_ext_version,
                )
                .await
                .map_err(AuthorityError::from)
            }
            CreateTransactionAuthorityRequest::LegacyAccount {
                product_account,
                request,
            } => {
                let keypair = self.product_keypair(&product_account)?;
                // Defense-in-depth: the slot-zero key must match the legacy
                // signer the caller asked for (also validated upstream). Never
                // sign with a diverging key.
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unknown {
                        reason: "signing host: legacy signer does not match the product \
                                 slot-zero account"
                            .to_string(),
                    });
                }
                build_local_transaction(
                    &self.services.chain,
                    &keypair,
                    request.genesis_hash,
                    &request.call_data,
                    &request.extensions,
                    request.tx_ext_version,
                )
                .await
                .map_err(AuthorityError::from)
            }
            CreateTransactionAuthorityRequest::IdentityAccount(request) => {
                let keypair = self.identity_keypair()?;
                if keypair.public.to_bytes() != request.signer {
                    return Err(AuthorityError::Unavailable {
                        reason: "signing host: the requested identity account is not available in \
                                 this CLI wallet"
                            .to_string(),
                    });
                }
                build_local_transaction(
                    &self.services.chain,
                    &keypair,
                    request.genesis_hash,
                    &request.call_data,
                    &request.extensions,
                    request.tx_ext_version,
                )
                .await
                .map_err(AuthorityError::from)
            }
        }
    }

    async fn account_alias(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountGetAliasRequest>,
    ) -> Result<v01::ContextualAlias, RingVrfError> {
        self.require_current_session(session)?;
        // A `context` grant covers this. RFC-0024 defines the scope as "acting
        // as the granting product's account: reading it and the identity that
        // follows from it", and the contextual alias is that identity: it and
        // the proof come out of one VRF evaluation, so a grantee that may
        // `create_proof` already holds the alias the proof attests. Prompting
        // here would ask the user to approve what the publisher's grant has
        // already authorized, and would leave the two calls disagreeing about
        // what `context` means.
        //
        // The gate is the same one `create_proof` uses, so a stored refusal
        // still overrides the grant. Falling through to the prompt keeps the
        // ungranted case exactly as it was.
        let granted = match self
            .require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
            .await
        {
            Ok(granted) => Some(granted),
            Err(RingVrfError::NotAllowlisted) => None,
            Err(err) => return Err(err),
        };
        // The grant admits the caller's own context and no one else's, exactly
        // as on `create_proof`. The alias this returns and the alias a proof
        // attests are one VRF evaluation, so guarding only the proof would leave
        // the same bytes reachable through this read.
        let key_handle = match granted {
            Some((key_handle, access)) => {
                crate::runtime::product_manifest::require_own_context(
                    &access,
                    &request.payload.context,
                )?;
                key_handle
            }
            None => {
                // No grant: the prompt path, as before. Both arguments are
                // normalized first so the decision is filed under, and read
                // back from, the identity the gate would have decided about.
                let requester = normalize_product_identifier(&request.calling_product_id)
                    .map_err(|_| RingVrfError::NotAllowlisted)?;
                let owner =
                    normalize_product_identifier(&request.payload.key_handle.dot_ns_identifier)
                        .map_err(|_| RingVrfError::NotAllowlisted)?;
                match super::account_access_authorization(
                    self.services.platform.as_ref(),
                    &requester,
                    &owner,
                )
                .await
                {
                    Ok(PermissionAuthorizationStatus::Authorized) => {}
                    Ok(
                        PermissionAuthorizationStatus::Denied
                        | PermissionAuthorizationStatus::NotDetermined,
                    ) => return Err(RingVrfError::Rejected),
                    Err(err) => {
                        return Err(RingVrfError::Unknown {
                            reason: err.to_string(),
                        });
                    }
                }
                v01::ProductAccountId {
                    dot_ns_identifier: owner,
                    derivation_index: request.payload.key_handle.derivation_index.clone(),
                }
            }
        };
        let entropy = self
            .resolve_ring_vrf_key_for_ring(session, &key_handle, &request.payload.ring_location)
            .await?;
        self.ring_resolver
            .validate(&request.payload.ring_location)
            .await?;
        let context = development_context_bytes(&request.payload.context);
        let alias = alias_from_entropy(&entropy, &context)?;
        Ok(v01::ContextualAlias {
            context,
            alias: alias.to_vec(),
        })
    }

    async fn create_proof(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountCreateProofRequest>,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        self.require_current_session(session)?;
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
        // The owner's own calls are unaffected; only a cross-product caller is
        // held to its own context.
        crate::runtime::product_manifest::require_own_context(&access, &request.payload.context)?;
        let entropy = self
            .resolve_ring_vrf_key_for_ring(session, &key_handle, &request.payload.ring_location)
            .await?;
        let candidate = self.ring_vrf_member_candidate(&entropy)?;
        let resolved = self
            .ring_resolver
            .resolve(&request.payload.ring_location, &[candidate])
            .await?;
        // Reject a stale request if the local session disconnected or changed
        // while its chain snapshot was being resolved.
        self.require_current_session(session)?;
        let context = development_context_bytes(&request.payload.context);
        let (proof, alias) = create_proof(&entropy, &resolved, &context, &request.payload.message)?;
        Ok(v01::HostAccountCreateProofResponse {
            proof,
            contextual_alias: v01::ContextualAlias {
                context,
                alias: alias.to_vec(),
            },
            ring_index: resolved.ring_index,
            ring_revision: resolved.ring_revision,
        })
    }

    async fn register_ring_vrf_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRegisterRingVrfKeyRequest>,
    ) -> Result<v01::RingVrfPublicKey, RingVrfError> {
        self.require_current_session(session)?;
        self.ring_resolver.validate(&request.payload.ring).await?;

        let handle = v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&request.calling_product_id).map_err(
                |err| RingVrfError::Unknown {
                    reason: err.to_string(),
                },
            )?,
            derivation_index: request.payload.index,
        };
        let entropy = self.ring_vrf_entropy(session, &handle)?;
        let public_key = member_from_entropy(&entropy)?;
        self.ring_vrf_registry
            .register(session.public_key, handle, request.payload.ring, public_key)
            .await?;
        Ok(public_key)
    }

    async fn list_ring_vrf_keys(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountListRingVrfKeysRequest>,
    ) -> Result<Vec<v01::RegisteredRingVrfKey>, RingVrfError> {
        self.require_current_session(session)?;
        let owner = normalize_product_identifier(&request.payload.owner).map_err(|err| {
            RingVrfError::Unknown {
                reason: err.to_string(),
            }
        })?;
        // Normalized before comparing, and before the prompt. `sso_responder`
        // hands `calling_product_id` through untouched, so comparing it raw
        // asks an owner to consent to its own account for spelling itself
        // differently, and files that decision under the spelling the peer
        // chose rather than the one the grant path reads back.
        let caller = normalize_product_identifier(&request.calling_product_id)
            .map_err(|_| RingVrfError::NotAllowlisted)?;
        if caller != owner {
            match super::account_access_authorization(
                self.services.platform.as_ref(),
                &caller,
                &owner,
            )
            .await
            {
                Ok(PermissionAuthorizationStatus::Authorized) => {}
                Ok(
                    PermissionAuthorizationStatus::Denied
                    | PermissionAuthorizationStatus::NotDetermined,
                ) => return Err(RingVrfError::Rejected),
                Err(err) => {
                    return Err(RingVrfError::Unknown {
                        reason: err.to_string(),
                    });
                }
            }
        }

        let mut entries = self
            .ring_vrf_registry
            .owner_entries(session.public_key, &owner)
            .await?;
        if request.payload.disclosure == v01::RingVrfKeyDisclosure::Anonymized {
            for entry in &mut entries {
                entry.public_key = None;
            }
        }
        Ok(entries)
    }

    async fn ring_vrf_sign(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductRequest<HostAccountRingVrfSignRequest>,
    ) -> Result<Vec<u8>, RingVrfError> {
        self.require_current_session(session)?;
        let (key_handle, _access) = self
            .require_ring_vrf_key_access(&request.calling_product_id, &request.payload.key_handle)
            .await?;
        let entropy = self
            .resolve_registered_ring_vrf_key(session, &key_handle)
            .await?;
        sign_from_entropy(&entropy, &request.payload.message)
    }

    async fn product_device_chat(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: ProductDeviceChatAuthorityRequest,
    ) -> Result<truapi::latest::HostProductDeviceChatResponse, ProductDeviceChatAuthorityError>
    {
        self.require_current_session(session)?;
        let calling_product_id = normalize_product_identifier(&request.calling_product_id)
            .map_err(|_| {
                ProductDeviceChatAuthorityError::Domain(
                    truapi::latest::HostProductDeviceChatError::InvalidRequest,
                )
            })?;
        let context = self.native_chat_context(session)?;
        self.native_chat
            .execute(context, calling_product_id, request.operation)
            .await
            .map_err(ProductDeviceChatAuthorityError::Domain)
    }

    async fn payment_top_up(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        request: PaymentTopUpRequest,
    ) -> Result<(), PaymentTopUpAuthorityError> {
        self.require_current_session(session)?;
        let product = normalize_product_identifier(&request.calling_product_id).map_err(|_| {
            PaymentTopUpAuthorityError::Domain(v01::HostPaymentTopUpError::InvalidSource)
        })?;
        let context = self
            .native_chat_context(session)
            .map_err(|error| match error {
                ProductDeviceChatAuthorityError::Disconnected => AuthorityError::Disconnected,
                _ => AuthorityError::Unavailable {
                    reason: "Wallet payment context unavailable".to_string(),
                },
            })?;
        let (_, payload) = request.into_parts();
        self.native_chat
            .top_up(context, product, payload)
            .await
            .map_err(PaymentTopUpAuthorityError::Domain)
    }

    async fn allocate_resources(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
        request: v01::HostRequestResourceAllocationRequest,
    ) -> Result<v01::HostRequestResourceAllocationResponse, AuthorityError> {
        self.require_current_session(session)?;
        let mut outcomes = Vec::with_capacity(request.resources.len());
        for resource in request.resources {
            let outcome = match resource {
                v01::AllocatableResource::StatementStoreAllowance => {
                    sso_responder::allocate_statement_store_allowance(
                        &self.services,
                        self,
                        session,
                        &product_id,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|_| v01::AllocationOutcome::Allocated)
                }
                v01::AllocatableResource::BulletinAllowance => {
                    sso_responder::allocate_bulletin_allowance(
                        &self.services,
                        self,
                        session,
                        &product_id,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|_| v01::AllocationOutcome::Allocated)
                }
                v01::AllocatableResource::SmartContractAllowance(index) => {
                    sso_responder::allocate_smart_contract_allowance(
                        &self.services,
                        self,
                        session,
                        &product_id,
                        index,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|()| v01::AllocationOutcome::Allocated)
                }
                v01::AllocatableResource::AutoSigning => self
                    .grant_auto_signing(session, &product_id)
                    .map(|_| v01::AllocationOutcome::Allocated)
                    .map_err(sso_responder::AllowanceAllocationError::Authority),
                v01::AllocatableResource::ProductStatementStoreAllowance(index) => {
                    sso_responder::allocate_product_statement_store_allowance(
                        &self.services,
                        self,
                        session,
                        &product_id,
                        &index,
                        OnExistingAllowancePolicy::Increase,
                    )
                    .await
                    .map(|()| v01::AllocationOutcome::Allocated)
                }
            };
            match outcome {
                Ok(outcome) => outcomes.push(outcome),
                Err(reason) => {
                    tracing::warn!(%product_id, %reason, "direct resource allocation item failed");
                    outcomes.push(v01::AllocationOutcome::NotAvailable);
                }
            }
        }
        Ok(v01::HostRequestResourceAllocationResponse { outcomes })
    }

    async fn statement_store_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<StatementStoreAllowanceKey, AuthorityError> {
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_statement_store_allowance(
            &self.services,
            self,
            session,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        StatementStoreAllowanceKey::from_secret_bytes(secret)
    }

    async fn bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            session,
            &product_id,
            OnExistingAllowancePolicy::Ignore,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn refresh_bulletin_allowance_key(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        product_id: String,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        self.require_current_session(session)?;
        let secret = sso_responder::allocate_bulletin_allowance(
            &self.services,
            self,
            session,
            &product_id,
            OnExistingAllowancePolicy::Increase,
        )
        .await
        .map_err(sso_responder::AllowanceAllocationError::into_authority_error)?;
        BulletinAllowanceKey::from_secret_bytes(secret)
    }

    async fn sign_statement_store_product_payload(
        &self,
        _cx: &CallContext,
        session: &AuthoritySession,
        _calling_product_id: Option<&str>,
        account: v01::ProductAccountId,
        payload: Vec<u8>,
    ) -> Result<[u8; 64], AuthorityError> {
        self.require_current_session(session)?;
        let keypair = self.product_keypair(&account)?;
        Ok(keypair
            .secret
            .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &keypair.public)
            .to_bytes())
    }

    fn derive_entropy(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        self.require_current_session(session)?;
        let entropy = self.root_entropy()?;
        derive_product_entropy(&entropy, product_id, context).map_err(|err| {
            AuthorityError::Unknown {
                reason: err.to_string(),
            }
        })
    }
}

fn local_session_validation_id(session: &SessionInfo, activation_generation: u64) -> Vec<u8> {
    let mut id = authority_session_validation_id(session);
    id.extend_from_slice(b":activation:");
    id.extend_from_slice(&activation_generation.to_le_bytes());
    id
}

fn product_authority_error(err: ProductAccountError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    mod auto_signing;
    mod raw_signing;

    use std::sync::Arc;

    use super::super::authority::{
        AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
        ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest,
        SignPayloadAuthorityRequest, SignRawAuthorityRequest,
    };
    use super::super::{ProductAuthority, ProductRuntimeHost, RuntimeServices, SigningHostRole};
    use super::TEST_NETWORK_SUFFIX;
    use super::ring_vrf::{MemberCandidate, ResolvedRing, RingResolver, member_from_entropy};
    use super::{LocalActivation, RingVrfError, SR25519_SIGNING_CONTEXT};
    use crate::host_logic::extrinsic::tests::split_v4;
    use crate::host_logic::permissions::PermissionsService;
    use crate::host_logic::product_account::{
        derive_identity_keypair, derive_product_keypair, derive_ring_vrf_entropy,
        derive_root_keypair_from_entropy, index_bytes,
    };
    use crate::host_logic::sso::messages::{
        ProductDeviceChatResponse, ProductRequest, RemoteMessage, RemoteMessageData,
        SsoProductDeviceChatOperation, v1,
    };
    use crate::host_logic::transaction::{
        extrinsic_payload_extensions, extrinsic_payload_preimage,
    };
    use crate::runtime::sso_service::Dispatch;
    use crate::runtime::statement_allowance::collection::PersonhoodCollection;
    use crate::test_support::{StubPlatform, test_spawner};
    use truapi::api::{Account, Entropy, ResourceAllocation, Signing};
    use truapi::latest::{
        HostAccountCreateProofRequest, HostAccountGetAliasRequest,
        HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
    };
    use truapi::versioned::account::{
        HostAccountGetError, HostAccountGetRequest, HostProductDeviceChatError,
        HostProductDeviceChatRequest, HostProductDeviceChatResponse,
    };
    use truapi::versioned::entropy::HostDeriveEntropyRequest;
    use truapi::versioned::resource_allocation::{
        HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
    };
    use truapi::versioned::signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse};
    use truapi::{CallContext, CallError, v01};
    use truapi_platform::{
        HostInfo, PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform,
        PlatformInfo, ProductContext, SigningHostConfig,
    };
    use verifiable::ring::RingDomainSize;

    const ENTROPY: [u8; 16] = [0xAB; 16];

    #[derive(Clone)]
    struct StubRingResolver {
        collection: [u8; 32],
        ring: ResolvedRing,
    }

    #[async_trait::async_trait]
    impl RingResolver for StubRingResolver {
        async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
            Ok(self.collection)
        }

        async fn resolve(
            &self,
            _location: &v01::RingLocation,
            candidates: &[MemberCandidate],
        ) -> Result<ResolvedRing, RingVrfError> {
            assert!(
                candidates.contains(&self.ring.selected),
                "signing host offered the explicitly registered key"
            );
            Ok(self.ring.clone())
        }
    }

    /// #660 and #655 join here: the hash the signing role installs is the one
    /// the grant path adjudicates against.
    ///
    /// Worth pinning because the two halves are testable apart and were built
    /// apart. #660's own tests prove the hash is installed; #655's grant tests
    /// seed the manifest **cache**, and `root_manifest` reads the cache before
    /// it ever needs a genesis hash, so every one of them would pass with
    /// #660 absent. This asserts the seam itself: the grant path's chain
    /// lookup has an Asset Hub to run against on a role whose config used to
    /// carry none.
    ///
    /// A cache miss still refuses, because the stub reaches no chain. That is
    /// the closed default, and it is why this seam needs its own test rather
    /// than being visible in a refusal.
    #[test]
    fn the_signing_role_adjudicates_grants_against_the_asset_hub_it_installed() {
        let (services, _authority) = signing_runtime();
        assert_eq!(
            services.asset_hub_chain_genesis_hash(),
            Some([0xcc; 32]),
            "#660 must install the config's Asset Hub, or #655 resolves no manifest here"
        );

        let platform = Arc::new(StubPlatform::default());
        let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
            &services,
            platform.as_ref(),
            "dim2.dot",
            "peopl.dot",
            crate::host_logic::product_manifest::Granted::Context,
        ));
        // Documentation, not a guard, and labelled so nobody reads it as one:
        // with no cached manifest and no reachable chain this is false whether
        // or not #660 installed a hash, so no mutation of the production path
        // can turn it red. The load-bearing assertion in this test is the one
        // above; the granted path is guarded by
        // `a_context_grant_lets_a_foreign_product_prove_with_the_owners_key`
        // and its M14 pair.
        assert!(
            !granted,
            "the closed default: no manifest reachable means no grant"
        );
    }

    fn signing_runtime() -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
        // Auto-confirm raw signing so the role-neutral confirmation gate does
        // not reject before reaching the signing authority.
        let platform: Arc<dyn Platform> = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            sign_vrf_confirmed: true,
            ..StubPlatform::default()
        });
        signing_runtime_with_platform(platform)
    }

    fn signing_runtime_with_platform(
        platform: Arc<dyn Platform>,
    ) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
        let config = SigningHostConfig::new(
            HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: None,
                version: None,
                platform: truapi::latest::HostPlatform::Ios,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            TEST_NETWORK_SUFFIX.to_string(),
        )
        .expect("signing host config is valid");
        let services = RuntimeServices::new(
            platform.clone(),
            config.host.host_info.clone(),
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            config.asset_hub_chain_genesis_hash,
            test_spawner(),
        );
        let signing_host = SigningHostRole::new(
            services.clone(),
            config.network_suffix,
            config.coinage_instance_id,
        );
        (services, signing_host)
    }

    fn product_runtime(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
    ) -> ProductRuntimeHost {
        ProductRuntimeHost::from_services(
            services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&services),
            authority,
            ProductContext::new("myapp.dot".to_string()).expect("valid product id"),
        )
    }

    fn product_runtime_for(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product_id: &str,
    ) -> ProductRuntimeHost {
        ProductRuntimeHost::from_services(
            services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&services),
            authority,
            ProductContext::new(product_id.to_string()).expect("valid product id"),
        )
    }

    #[test]
    fn payment_top_up_requires_current_wallet_session_without_chat_grants() {
        use crate::host_logic::sso::messages::PaymentTopUpRequest;
        use crate::runtime::authority::PaymentTopUpAuthorityError;
        use truapi::api::Payment;
        use truapi::versioned::payment::{HostPaymentTopUpError, HostPaymentTopUpRequest};

        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform {
                chain_connect_error: Some("offline"),
                ..Default::default()
            });
            let (services, authority) = signing_runtime_with_platform(platform.clone());
            let runtime = product_runtime(services, authority.clone());
            let request = || {
                HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
                    into: None,
                    amount: 1,
                    source: v01::PaymentTopUpSource::ProductAccount {
                        derivation_index: v01::DerivationIndex::Index(0),
                    },
                })
            };
            assert!(matches!(
                runtime.top_up(&CallContext::default(), request()).await,
                Err(CallError::Domain(HostPaymentTopUpError::V1(
                    v01::HostPaymentTopUpError::Unknown { .. }
                )))
            ));
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let session = authority.current_session().unwrap();
            // An active wallet reaches source validation, not a Chat/signing prompt.
            assert_eq!(
                runtime.top_up(&CallContext::default(), request()).await,
                Err(CallError::Domain(HostPaymentTopUpError::V1(
                    v01::HostPaymentTopUpError::InvalidSource
                )))
            );
            assert!(platform.chat_authority_reviews.lock().is_empty());
            assert!(
                platform
                    .main_purse_chat_payment_reviews
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            assert!(platform.sign_raw_reviews.lock().unwrap().is_empty());
            assert!(platform.sign_payload_reviews.lock().unwrap().is_empty());
            assert_eq!(
                platform
                    .identity_disclosure_calls
                    .load(std::sync::atomic::Ordering::SeqCst),
                0
            );

            authority.disconnect().await;
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            assert_eq!(
                authority
                    .payment_top_up(
                        &CallContext::default(),
                        &session,
                        PaymentTopUpRequest {
                            calling_product_id: "myapp.dot".to_string(),
                            payload: request(),
                        },
                    )
                    .await,
                Err(PaymentTopUpAuthorityError::Authority(
                    AuthorityError::Disconnected
                ))
            );
        });
    }

    #[test]
    fn statement_allowance_decisions_survive_runtime_restart_and_remain_scoped() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let selector = Some(v01::DerivationIndex::Index(0));
        let request = PermissionAuthorizationRequest::StatementStoreAllowance {
            derivation_index: selector.clone(),
        };
        futures::executor::block_on(async {
            let (services, authority) = signing_runtime_with_platform(platform.clone());
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let first = product_runtime(services, authority.clone());
            let session = authority.current_session().unwrap();
            first
                .require_statement_store_allowance(&session, selector.clone())
                .await
                .unwrap();
            drop(first);
            drop(authority);

            let (services, authority) = signing_runtime_with_platform(platform.clone());
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let restarted = product_runtime(services.clone(), authority.clone());
            let session = authority.current_session().unwrap();
            restarted
                .require_statement_store_allowance(&session, selector.clone())
                .await
                .unwrap();
            assert_eq!(
                platform.resource_allocation_reviews.lock().unwrap().len(),
                1
            );
            assert_eq!(
                restarted
                    .permission_authorization_status(request.clone())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
            for separate in [
                PermissionAuthorizationRequest::StatementStoreAllowance {
                    derivation_index: None,
                },
                PermissionAuthorizationRequest::StatementStoreAllowance {
                    derivation_index: Some(v01::DerivationIndex::Index(1)),
                },
                PermissionAuthorizationRequest::ChatAuthority,
                PermissionAuthorizationRequest::IdentityDisclosure,
            ] {
                assert_eq!(
                    restarted
                        .permission_authorization_status(separate)
                        .await
                        .unwrap(),
                    PermissionAuthorizationStatus::NotDetermined
                );
            }
            let other = product_runtime_for(services.clone(), authority.clone(), "other.dot");
            assert_eq!(
                other
                    .permission_authorization_status(request.clone())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined
            );

            // A connection with a different artifact store must not inherit a
            // decision even with the same product id and the same authority.
            let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
            adapters.platform = Arc::new(StubPlatform::default());
            let other_artifact = ProductRuntimeHost::from_services(
                services,
                adapters,
                authority,
                ProductContext::new("myapp.dot".to_string()).unwrap(),
            );
            assert_eq!(
                other_artifact
                    .permission_authorization_status(request.clone())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::NotDetermined
            );

            restarted
                .set_permission_authorization_status(
                    request.clone(),
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            let result = ResourceAllocation::request(
                &restarted,
                &CallContext::default(),
                HostRequestResourceAllocationRequest::V1(
                    v01::HostRequestResourceAllocationRequest {
                        resources: vec![v01::AllocatableResource::ProductStatementStoreAllowance(
                            v01::DerivationIndex::Index(0),
                        )],
                    },
                ),
            )
            .await;
            assert!(result.is_err());
            assert!(platform.sent_rpc.lock().unwrap().is_empty());
            assert_eq!(
                platform.resource_allocation_reviews.lock().unwrap().len(),
                1
            );
            assert_eq!(
                restarted
                    .permission_authorization_status(request.clone())
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Denied
            );
            restarted
                .set_permission_authorization_status(
                    request,
                    PermissionAuthorizationStatus::NotDetermined,
                )
                .await
                .unwrap();
            restarted
                .require_statement_store_allowance(&session, selector)
                .await
                .unwrap();
            assert_eq!(
                platform.resource_allocation_reviews.lock().unwrap().len(),
                2
            );
        });
    }

    #[test]
    fn explicit_statement_increases_each_prompt_and_initial_approval_also_grants_ensure() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(async {
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let runtime = product_runtime(services, authority.clone());
            let session = authority.current_session().unwrap();
            for expected_reviews in 1..=2 {
                // Chain availability is independent of consent: a failed
                // provisioning attempt must not force a second grant prompt.
                ResourceAllocation::request(
                    &runtime,
                    &CallContext::default(),
                    HostRequestResourceAllocationRequest::V1(
                        v01::HostRequestResourceAllocationRequest {
                            resources: vec![v01::AllocatableResource::StatementStoreAllowance],
                        },
                    ),
                )
                .await
                .unwrap();
                assert_eq!(
                    platform
                        .resource_allocation_reviews
                        .lock()
                        .expect("reviews")
                        .len(),
                    expected_reviews
                );
                runtime
                    .require_statement_store_allowance(&session, None)
                    .await
                    .unwrap();
                assert_eq!(
                    platform
                        .resource_allocation_reviews
                        .lock()
                        .expect("reviews")
                        .len(),
                    expected_reviews
                );
            }
        });
    }

    #[test]
    fn cancelling_an_explicit_increase_preserves_the_implicit_allowance_grant() {
        let platform = Arc::new(StubPlatform::default());
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(async {
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let runtime = product_runtime(services, authority.clone());
            let grant = PermissionAuthorizationRequest::StatementStoreAllowance {
                derivation_index: None,
            };
            runtime
                .set_permission_authorization_status(
                    grant.clone(),
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            assert!(
                ResourceAllocation::request(
                    &runtime,
                    &CallContext::default(),
                    HostRequestResourceAllocationRequest::V1(
                        v01::HostRequestResourceAllocationRequest {
                            resources: vec![v01::AllocatableResource::StatementStoreAllowance],
                        }
                    )
                )
                .await
                .is_err()
            );
            assert_eq!(
                runtime
                    .permission_authorization_status(grant)
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
            runtime
                .require_statement_store_allowance(&authority.current_session().unwrap(), None)
                .await
                .unwrap();
            assert_eq!(
                platform
                    .resource_allocation_reviews
                    .lock()
                    .expect("reviews")
                    .len(),
                1
            );
            assert!(platform.sent_rpc.lock().expect("rpc").is_empty());
        });
    }

    #[test]
    fn administration_during_explicit_review_wins_over_the_confirmation() {
        use futures::FutureExt;
        for (before, administrative) in [
            (
                PermissionAuthorizationStatus::NotDetermined,
                PermissionAuthorizationStatus::Denied,
            ),
            (
                PermissionAuthorizationStatus::Authorized,
                PermissionAuthorizationStatus::NotDetermined,
            ),
        ] {
            let (release, gate) = futures::channel::oneshot::channel();
            let platform = Arc::new(StubPlatform {
                resource_allocation_confirmed: true,
                ..Default::default()
            });
            *platform
                .resource_allocation_confirmation_gate
                .lock()
                .expect("gate") = Some(gate);
            let (services, authority) = signing_runtime_with_platform(platform.clone());
            futures::executor::block_on(async {
                authority
                    .activate_local_session(ENTROPY.to_vec())
                    .await
                    .unwrap();
                let runtime = product_runtime(services, authority);
                let grant = PermissionAuthorizationRequest::StatementStoreAllowance {
                    derivation_index: None,
                };
                runtime
                    .set_permission_authorization_status(grant.clone(), before)
                    .await
                    .unwrap();
                let cx = CallContext::default();
                let allocation = ResourceAllocation::request(
                    &runtime,
                    &cx,
                    HostRequestResourceAllocationRequest::V1(
                        v01::HostRequestResourceAllocationRequest {
                            resources: vec![v01::AllocatableResource::StatementStoreAllowance],
                        },
                    ),
                );
                futures::pin_mut!(allocation);
                assert!(allocation.as_mut().now_or_never().is_none());
                runtime
                    .set_permission_authorization_status(grant.clone(), administrative)
                    .await
                    .unwrap();
                release.send(()).unwrap();
                assert!(allocation.await.is_err());
                assert_eq!(
                    runtime
                        .permission_authorization_status(grant)
                        .await
                        .unwrap(),
                    administrative
                );
                assert!(platform.sent_rpc.lock().expect("rpc").is_empty());
            });
        }
    }

    #[test]
    fn statement_allowance_storage_failure_never_prompts_or_allocates() {
        let platform = Arc::new(StubPlatform {
            local_storage_error: Some("storage unavailable"),
            resource_allocation_confirmed: true,
            ..Default::default()
        });
        let (services, authority) =
            signing_runtime_with_platform(Arc::new(StubPlatform::default()));
        futures::executor::block_on(async {
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
            adapters.platform = platform.clone();
            let runtime = ProductRuntimeHost::from_services(
                services,
                adapters,
                authority,
                ProductContext::new("myapp.dot".to_string()).unwrap(),
            );
            let result = ResourceAllocation::request(
                &runtime,
                &CallContext::default(),
                HostRequestResourceAllocationRequest::V1(
                    v01::HostRequestResourceAllocationRequest {
                        resources: vec![v01::AllocatableResource::StatementStoreAllowance],
                    },
                ),
            )
            .await;
            assert!(result.is_err());
            assert!(
                platform
                    .resource_allocation_reviews
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            assert!(platform.sent_rpc.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn statement_consent_cannot_survive_same_account_reactivation() {
        use futures::FutureExt;
        for explicit in [false, true] {
            let (release, gate) = futures::channel::oneshot::channel();
            let platform = Arc::new(StubPlatform {
                resource_allocation_confirmed: true,
                ..Default::default()
            });
            *platform
                .resource_allocation_confirmation_gate
                .lock()
                .expect("gate lock") = Some(gate);
            let (services, authority) = signing_runtime_with_platform(platform.clone());
            futures::executor::block_on(async {
                authority
                    .activate_local_session(ENTROPY.to_vec())
                    .await
                    .unwrap();
                let runtime = product_runtime(services, authority.clone());
                let session = authority.current_session().unwrap();
                let consent = async {
                    if explicit {
                        ResourceAllocation::request(
                            &runtime,
                            &CallContext::default(),
                            HostRequestResourceAllocationRequest::V1(
                                v01::HostRequestResourceAllocationRequest {
                                    resources: vec![
                                        v01::AllocatableResource::StatementStoreAllowance,
                                    ],
                                },
                            ),
                        )
                        .await
                        .map(|_| ())
                        .map_err(|error| format!("{error:?}"))
                    } else {
                        runtime
                            .require_statement_store_allowance(&session, None)
                            .await
                    }
                };
                futures::pin_mut!(consent);
                assert!(consent.as_mut().now_or_never().is_none());
                authority.disconnect().await;
                authority
                    .activate_local_session(ENTROPY.to_vec())
                    .await
                    .unwrap();
                release.send(()).unwrap();
                assert!(consent.await.is_err());
                assert_eq!(
                    runtime
                        .permission_authorization_status(
                            PermissionAuthorizationRequest::StatementStoreAllowance {
                                derivation_index: None
                            },
                        )
                        .await
                        .unwrap(),
                    PermissionAuthorizationStatus::NotDetermined
                );
                assert!(platform.sent_rpc.lock().expect("rpc lock").is_empty());
            });
        }
    }

    fn vrf_request(product_id: &str) -> v01::HostAccountSignVrfRequest {
        v01::HostAccountSignVrfRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: product_id.to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            transcript_label: b"pop:autosigning".to_vec(),
            items: vec![v01::VrfTranscriptItem {
                label: b"round".to_vec(),
                value: vec![1],
            }],
        }
    }

    fn full_person_key_handle() -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: "peopl.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        }
    }

    fn full_person_ring_resolver() -> Arc<StubRingResolver> {
        let full_entropy =
            derive_ring_vrf_entropy(&ENTROPY, "peopl.dot", &v01::DerivationIndex::Index(0))
                .expect("full-person entropy");
        let full_member = member_from_entropy(&full_entropy).expect("full-person member");
        Arc::new(StubRingResolver {
            collection: *b"pop:polkadot.network/people     ",
            ring: ResolvedRing {
                selected: MemberCandidate {
                    member: full_member,
                },
                ring_index: 7,
                ring_revision: 11,
                domain_size: RingDomainSize::Domain11,
                members: vec![full_member],
            },
        })
    }

    /// Seed `owner`'s cached manifest so a grant lookup resolves without a
    /// chain. Mirrors `runtime::tests::cache_manifest`.
    fn cache_grant(platform: &StubPlatform, owner: &str, trusted_products: &str) {
        let json = format!(
            r#"{{"$v":1,"displayName":"D","description":"d",
                 "icon":{{"cid":"c","format":"png"}},"trustedProducts":{trusted_products}}}"#
        );
        let entry = crate::runtime::product_manifest::CachedManifest {
            fetched_at_secs: crate::host_logic::statement_store::current_unix_secs(),
            json: Some(json),
        };
        futures::executor::block_on(
            <StubPlatform as truapi_platform::CoreStorage>::write_core_storage(
                platform,
                crate::runtime::product_manifest::manifest_cache_key(owner),
                parity_scale_codec::Encode::encode(&entry),
            ),
        )
        .expect("stub core storage accepts the entry");
    }

    /// Persist a user refusal of `caller`'s access to `target`'s account.
    fn deny_account_access(platform: &StubPlatform, caller: &str, target: &str) {
        futures::executor::block_on(
            crate::host_logic::permissions::PermissionsService::new(
                platform,
                platform,
                crate::host_logic::product_manifest::bare_product_label(caller),
            )
            .set_authorization_status(
                &truapi_platform::PermissionAuthorizationRequest::AccountAccess {
                    // Bare-labelled on both sides, as `account_access_authorization`
                    // writes it in production.
                    target_product_id: crate::host_logic::product_manifest::bare_product_label(
                        target,
                    )
                    .to_string(),
                },
                truapi_platform::PermissionAuthorizationStatus::Denied,
            ),
        )
        .expect("stub core storage accepts the decision");
    }

    /// A `context` grant lets a foreign product prove with the owner's key.
    ///
    /// The test whose absence let the inert scope ship. The earlier
    /// `a_cached_context_grant_lets_a_foreign_proof_through` asserted
    /// `Rejected` with no session, which only proved the call reached the
    /// session guard, and the authority one layer down would have refused it
    /// anyway. This one runs the whole stack with a live session and a
    /// registered key, so a proof actually comes back.
    #[test]
    fn a_context_grant_lets_a_foreign_product_prove_with_the_owners_key() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let host = product_runtime_for(services, authority.clone(), "dim2.dot");
        let proof = futures::executor::block_on(host.create_account_proof(
            &CallContext::default(),
            foreign_proof_request(&ring_location),
        ));
        assert!(
            proof.is_ok(),
            "a granted cross-product proof must succeed, got {proof:?}"
        );
    }

    /// The same call with no grant. Same fixture, one line different.
    #[test]
    fn a_foreign_proof_is_refused_when_the_owner_granted_nothing() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"someone-else":["context"]}"#);
        let (services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let host = product_runtime_for(services, authority.clone(), "dim2.dot");
        let proof = futures::executor::block_on(host.create_account_proof(
            &CallContext::default(),
            foreign_proof_request(&ring_location),
        ));
        assert_eq!(
            proof.err(),
            Some(CallError::Domain(
                truapi::versioned::account::HostAccountCreateProofError::V1(
                    v01::HostAccountCreateProofError::NotAllowlisted
                )
            ))
        );
    }

    /// **The one that matters.** Nothing in the request can stand in for the
    /// manifest.
    ///
    /// This drives the authority directly, the way `sso_responder` does for a
    /// request arriving over the pairing wire. The frontend, and its grant
    /// check, are not on this path at all. The request names `dim2.dot` as the
    /// caller and `peopl.dot`'s key as the handle, which is the most a peer can
    /// assert. With no grant published it is refused; with the grant published
    /// and nothing else changed it succeeds. So the admitting fact is the
    /// manifest the authority resolved for itself, not any field the caller
    /// set.
    ///
    /// What makes this cover the wire path is an invariant, not a convention:
    /// the authority has exactly one behaviour, shared by both doors, because
    /// nothing it reads records which door a request came through. Should a
    /// door-dependent relaxation ever land (a dev allowlist consulted only for
    /// local callers, say), that invariant is gone, and this test has to
    /// declare the paired-peer door explicitly or it silently stops covering
    /// it. Whoever adds the distinction owns updating this.
    #[test]
    fn a_request_cannot_substitute_for_the_owners_manifest() {
        let refusal = foreign_proof_through_the_authority(None);
        assert_eq!(
            refusal.err(),
            Some(RingVrfError::NotAllowlisted),
            "with no manifest grant the authority must refuse, whatever the request says"
        );

        let granted = foreign_proof_through_the_authority(Some(r#"{"dim2":["context"]}"#));
        assert!(
            granted.is_ok(),
            "the identical request must succeed once the owner's manifest grants it, \
             which is what proves the manifest is the deciding input; got {granted:?}"
        );
    }

    /// A grant never overrides a refusal the user already gave.
    ///
    /// The stored `AccountAccess` decision is read before the manifest, and
    /// read-only: a grant lookup must not raise the prompt that would settle a
    /// `NotDetermined` one.
    #[test]
    fn a_stored_denial_survives_a_context_grant_at_the_authority() {
        let refusal =
            foreign_proof_through_the_authority_with(Some(r#"{"dim2":["context"]}"#), |platform| {
                deny_account_access(platform, "dim2.dot", "peopl.dot")
            });
        assert_eq!(refusal.err(), Some(RingVrfError::NotAllowlisted));
    }

    /// A `context` grant covers the identity read, so the two calls agree.
    ///
    /// The contextual alias and the proof come out of one VRF evaluation, so a
    /// grantee that may `create_proof` already holds the alias that proof
    /// attests. Gating `account_alias` on a prompt while `create_proof` is
    /// gated on the grant left the same bytes reachable one way and refused the
    /// other, which is not a policy anyone chose. RFC-0024 defines `context` as
    /// reading the account "and the identity that follows from it".
    ///
    /// A product with no grant still takes the prompt, and a stored refusal
    /// still overrides the grant.
    #[test]
    fn a_context_grant_covers_the_identity_read() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let context = v01::ProductProofContext {
            product_id: "dim2.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let alias_for = |caller: &str| {
            futures::executor::block_on(authority.account_alias(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: caller.to_string(),
                    payload: v01::HostAccountGetAliasRequest {
                        key_handle: full_person_key_handle(),
                        context: context.clone(),
                        ring_location: ring.clone(),
                    },
                },
            ))
        };

        let owner = alias_for("peopl.dot").expect("the owner reads its own alias");
        let granted = alias_for("dim2.dot").expect("a context grant covers the identity read");
        assert_eq!(
            granted.alias, owner.alias,
            "the grantee must read the owner's alias, not one derived for itself"
        );
        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("review list mutex poisoned")
                .len(),
            0,
            "a granted read must not raise the prompt the grant already answers"
        );
        assert_eq!(
            alias_for("stash.dot").err(),
            Some(RingVrfError::Rejected),
            "a product with no grant still takes the prompt path and is refused"
        );
    }

    /// A grant does not let the grantee choose whose pseudonym to mint.
    ///
    /// The contextual alias is a function of (owner key, context), so with the
    /// context unconstrained a `context` grant from `peopl.dot` let `dim2.dot`
    /// produce the alias `peopl.dot` presents to `bank.dot`, a third product
    /// that granted nothing, is not a party to the grant, and cannot consent
    /// here. The grant is to act in the grantee's own context, not in anyone's.
    ///
    /// The owner's own calls are untouched: minting your own aliases in any
    /// context is what the context parameter is for.
    #[test]
    fn a_grantee_cannot_mint_the_owners_alias_in_a_third_partys_context() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        let mint = |caller: &str, context: &str| {
            futures::executor::block_on(authority.create_proof(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: caller.to_string(),
                    payload: v01::HostAccountCreateProofRequest {
                        key_handle: full_person_key_handle(),
                        context: v01::ProductProofContext {
                            product_id: context.to_string(),
                            suffix: v01::DerivationIndex::Index(0),
                        },
                        ring_location: ring.clone(),
                        message: b"m".to_vec(),
                    },
                },
            ))
        };

        assert_eq!(
            mint("dim2.dot", "bank.dot").err(),
            Some(RingVrfError::NotAllowlisted),
            "the grantee must not mint the owner's pseudonym for a third product"
        );
        assert!(
            mint("dim2.dot", "dim2.dot").is_ok(),
            "the grant still admits the grantee acting in its own context"
        );
        assert!(
            mint("peopl.dot", "bank.dot").is_ok(),
            "the owner may still mint its own alias in any context"
        );
    }

    /// An owner listing its own keys is not asked to consent to its own account.
    ///
    /// `sso_responder` hands `calling_product_id` through untouched, so
    /// comparing it raw against the normalized owner makes an owner that spells
    /// itself differently look like a stranger: it is prompted, and the decision
    /// is filed under the spelling the peer chose rather than the one the grant
    /// path reads back.
    #[test]
    fn an_owner_listing_its_own_keys_is_not_prompted_for_its_own_account() {
        let platform = Arc::new(StubPlatform::default());
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        let listed = futures::executor::block_on(authority.list_ring_vrf_keys(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "PEOPL.DOT".to_string(),
                payload: v01::HostAccountListRingVrfKeysRequest {
                    owner: "peopl.dot".to_string(),
                    disclosure: v01::RingVrfKeyDisclosure::PublicKey,
                },
            },
        ));
        assert!(
            listed.is_ok(),
            "an owner must reach its own keys however it spells itself; got {:?}",
            listed.err()
        );
        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("review list mutex poisoned")
                .len(),
            0,
            "and must not be asked to consent to its own account"
        );
    }

    /// The context guard answers "is this the owner?" the way the gate does.
    ///
    /// The gate short-circuits on full normalized ids; comparing bare labels in
    /// the guard made a caller `peopl.paseo` against a handle `peopl.dot` a
    /// stranger to one and the owner to the other, so the grant was demanded and
    /// the context then left unconstrained. Two functions answering one question
    /// differently is what this change otherwise collapses.
    #[test]
    fn a_cross_network_namesake_is_not_treated_as_the_owner() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"peopl":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        // `peopl.paseo` shares a label with the key's owner `peopl.dot` but is a
        // different product, so the grant admits it and the context still binds.
        let minted = futures::executor::block_on(authority.create_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "peopl.paseo".to_string(),
                payload: v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: "bank.dot".to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: ring.clone(),
                    message: b"m".to_vec(),
                },
            },
        ));
        assert_eq!(
            minted.err(),
            Some(RingVrfError::NotAllowlisted),
            "a namesake on another network is not the owner, so its context binds"
        );
    }

    /// A grantee spelling its own context differently is still in its own context.
    ///
    /// `context.product_id` arrives straight off the request payload. Left raw it
    /// was the one identity the gate had not normalized, so a grantee naming its
    /// context `DIM2.DOT` was refused for the spelling rather than the scope.
    #[test]
    fn a_grantee_may_spell_its_own_context_differently() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let mint = |context: &str| {
            futures::executor::block_on(authority.create_proof(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: "dim2.dot".to_string(),
                    payload: v01::HostAccountCreateProofRequest {
                        key_handle: full_person_key_handle(),
                        context: v01::ProductProofContext {
                            product_id: context.to_string(),
                            suffix: v01::DerivationIndex::Index(0),
                        },
                        ring_location: ring.clone(),
                        message: b"m".to_vec(),
                    },
                },
            ))
        };
        assert!(mint("dim2.dot").is_ok(), "control: the canonical spelling");
        assert!(
            mint("DIM2.DOT").is_ok(),
            "the same context in another spelling is still the caller's own"
        );
        assert_eq!(
            mint("bank.dot").err(),
            Some(RingVrfError::NotAllowlisted),
            "and a third party's context is still refused"
        );
    }

    /// A refusal recorded by an earlier release still overrides a grant.
    ///
    /// This release files the decision under the product label; earlier ones
    /// used the full product id. Reading only the new shape would discard the
    /// old decision, and on the granted path it would not even re-ask, because
    /// the lookup reads `NotDetermined`, admits the grant and raises no prompt.
    /// A user's "no" would become a "yes" on upgrade.
    #[test]
    fn a_refusal_recorded_before_this_release_still_overrides_a_grant() {
        use crate::host_logic::product_manifest::Granted;
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        // Written exactly as the previous release wrote it: full ids, both sides.
        futures::executor::block_on(
            crate::host_logic::permissions::PermissionsService::new(
                platform.as_ref(),
                platform.as_ref(),
                "dim2.dot",
            )
            .set_authorization_status(
                &truapi_platform::PermissionAuthorizationRequest::AccountAccess {
                    target_product_id: "peopl.dot".to_string(),
                },
                truapi_platform::PermissionAuthorizationStatus::Denied,
            ),
        )
        .expect("stub core storage accepts the decision");
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        assert!(
            !futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                platform.as_ref(),
                "dim2.dot",
                "peopl.dot",
                Granted::Context,
            )),
            "a decision stored under the previous key shape must still be honoured"
        );
        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("review list mutex poisoned")
                .len(),
            0,
            "and it must be honoured without re-asking"
        );
    }

    /// A refusal covers the refused product's subnames on the grantor side too.
    ///
    /// The grant is resolved by the target's bare label, so filing the refusal
    /// against the full target let `app.peopl.dot` carry a grant the user had
    /// refused for `peopl.dot`. The argument the PR makes for the requester
    /// applies unchanged to the grantor.
    #[test]
    fn a_refusal_covers_every_executable_of_the_refused_target() {
        use crate::host_logic::product_manifest::Granted;
        let platform = Arc::new(StubPlatform {
            // The user declines, so the refusal is written by the production
            // path rather than by a test helper: this has to pin where
            // `account_access_authorization` files it, not where a fixture does.
            account_access_confirmed: false,
            ..StubPlatform::default()
        });
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        futures::executor::block_on(crate::runtime::account_access_authorization(
            platform.as_ref(),
            "dim2.dot",
            "peopl.dot",
        ))
        .expect("the stub records the declined decision");
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        for target in ["peopl.dot", "app.peopl.dot", "worker.peopl.dot"] {
            assert!(
                !futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                    &services,
                    platform.as_ref(),
                    "dim2.dot",
                    target,
                    Granted::Context,
                )),
                "{target} is the refused product wearing another name"
            );
        }
    }

    /// The identity read is held to the caller's own context, like the proof.
    ///
    /// The alias and the proof come out of one VRF evaluation, so a guard on
    /// `create_proof` alone leaves the same bytes reachable through
    /// `account_alias`: a grantee could read the alias the owner presents to a
    /// third product that granted nothing. Both calls now refuse it.
    #[test]
    fn a_grantee_cannot_read_the_owners_alias_in_a_third_partys_context() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let alias = |caller: &str, context: &str| {
            futures::executor::block_on(authority.account_alias(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: caller.to_string(),
                    payload: v01::HostAccountGetAliasRequest {
                        key_handle: full_person_key_handle(),
                        context: v01::ProductProofContext {
                            product_id: context.to_string(),
                            suffix: v01::DerivationIndex::Index(0),
                        },
                        ring_location: ring.clone(),
                    },
                },
            ))
        };

        assert_eq!(
            alias("dim2.dot", "bank.dot").err(),
            Some(RingVrfError::NotAllowlisted),
            "a grantee must not read the owner's pseudonym for a third product"
        );
        assert!(
            alias("dim2.dot", "dim2.dot").is_ok(),
            "the grant still covers the grantee's own context"
        );
        assert!(
            alias("peopl.dot", "bank.dot").is_ok(),
            "the owner may still read its own alias in any context"
        );
    }

    /// An owner naming itself in another spelling is admitted, over the wire.
    ///
    /// The context guard compares the identities the gate normalized, not the
    /// ones the request carried. Deriving the caller from the request again
    /// compares a peer's spelling against a normalized owner and refuses the
    /// owner on its own key. `require_ring_vrf_key_access` returns the
    /// normalized owner to stop exactly that, and `sso_responder` hands
    /// `calling_product_id` through untouched.
    #[test]
    fn an_owner_spelled_differently_still_proves_with_its_own_key() {
        let platform = Arc::new(StubPlatform::default());
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let prove = |caller: &str| {
            futures::executor::block_on(authority.create_proof(
                &CallContext::default(),
                &session,
                ProductRequest {
                    calling_product_id: caller.to_string(),
                    payload: v01::HostAccountCreateProofRequest {
                        key_handle: full_person_key_handle(),
                        context: v01::ProductProofContext {
                            product_id: "peopl.dot".to_string(),
                            suffix: v01::DerivationIndex::Index(0),
                        },
                        ring_location: ring.clone(),
                        message: b"m".to_vec(),
                    },
                },
            ))
        };
        assert!(
            prove("peopl.dot").is_ok(),
            "control: the canonical spelling"
        );
        assert!(
            prove("PEOPL.DOT").is_ok(),
            "the owner must not be refused on its own key for spelling itself differently"
        );
    }

    /// A refusal covers the product, not one spelling of it.
    ///
    /// The manifest grants a product and a product is all its executables, so a
    /// refusal that overrides the grant has to name the same party. Keyed by the
    /// full id instead, the user refusing `dim2.dot` left `app.dim2.dot` holding
    /// the identical grant: the product re-entered under a subname it already
    /// owns and the override was gone. Both halves now key by the bare label.
    #[test]
    fn a_refusal_covers_every_executable_of_the_refused_product() {
        use crate::host_logic::product_manifest::Granted;
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        deny_account_access(&platform, "dim2.dot", "peopl.dot");
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        let granted = |caller: &str| {
            futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                platform.as_ref(),
                caller,
                "peopl.dot",
                Granted::Context,
            ))
        };
        for spelling in ["dim2.dot", "app.dim2.dot", "worker.dim2.dot", "dim2.paseo"] {
            assert!(
                !granted(spelling),
                "{spelling} is the refused product wearing another name"
            );
        }

        // Control: a product the user never refused still holds its own grant,
        // so the assertions above are not passing because nothing is granted.
        let clean = Arc::new(StubPlatform::default());
        cache_grant(&clean, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(clean.clone(), full_person_ring_resolver());
        assert!(
            futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                clean.as_ref(),
                "app.dim2.dot",
                "peopl.dot",
                Granted::Context,
            )),
            "control: with no refusal recorded the same subname is granted"
        );
    }

    /// `all` is a superset, so it satisfies `context` at the runtime seam and
    /// not only in the manifest parser.
    #[test]
    fn a_grant_of_all_satisfies_context_at_the_authority() {
        let granted = foreign_proof_through_the_authority(Some(r#"{"dim2":["all"]}"#));
        assert!(
            granted.is_ok(),
            "`all` must satisfy `context`, got {granted:?}"
        );
    }

    fn signing_runtime_with_ring_resolver(
        platform: Arc<StubPlatform>,
        ring_resolver: Arc<StubRingResolver>,
    ) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
        let authority = SigningHostRole::new_with_ring_resolver(platform, ring_resolver);
        (authority.services(), authority)
    }

    /// The grant admits `ring_vrf_sign`, not only `create_proof`.
    ///
    /// #655 lists this as untested and it was: the other grant tests here all
    /// drive `create_proof`. Both authorities call the same
    /// `require_ring_vrf_key_access` from both methods, so the code was
    /// covered, but a scope that admits one call and not the other is exactly
    /// the kind of half-wired gate this issue exists to fix, and nothing
    /// asserted the second half.
    ///
    /// Driven at the authority, where the wire path also arrives, so this
    /// covers the paired case as well. Success is the owner's own signature:
    /// the grant lets `dim2.dot` produce what `peopl.dot` would have.
    #[test]
    fn a_context_grant_lets_a_foreign_product_sign_with_the_owners_key() {
        let granted = foreign_ring_vrf_sign_through_the_authority(Some(r#"{"dim2":["context"]}"#));
        let owners_own = foreign_ring_vrf_sign_through_the_authority_as(
            Some(r#"{"dim2":["context"]}"#),
            "peopl.dot",
        );
        assert!(
            granted.is_ok(),
            "a granted cross-product ring-VRF signature must be produced, got {granted:?}"
        );
        assert_eq!(
            granted, owners_own,
            "the grant must yield the owner's own signature, not a caller-derived one"
        );
    }

    /// The same call with no grant.
    #[test]
    fn a_foreign_ring_vrf_sign_is_refused_when_the_owner_granted_nothing() {
        assert_eq!(
            foreign_ring_vrf_sign_through_the_authority(None).err(),
            Some(RingVrfError::NotAllowlisted)
        );
    }

    /// An unreadable permission store refuses the grant rather than honouring it.
    ///
    /// The stored `AccountAccess` decision is the only thing that can override a
    /// publisher's grant. Reading a storage fault as "not refused" would let a
    /// locked keychain turn the user's explicit no into a yes, on the strength
    /// of a manifest the publisher controls. `account_access_authorization`,
    /// which writes that same decision, already fails closed; this is the read
    /// side agreeing with it.
    ///
    /// The error is scoped to permission keys so the manifest cache still
    /// answers: otherwise the call would refuse for want of a manifest and the
    /// assertion would prove nothing.
    #[test]
    fn a_grant_is_refused_when_the_stored_decision_cannot_be_read() {
        let platform = Arc::new(StubPlatform {
            permission_storage_error: Some("keychain locked"),
            ..StubPlatform::default()
        });
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
            &services,
            platform.as_ref(),
            "dim2.dot",
            "peopl.dot",
            crate::host_logic::product_manifest::Granted::Context,
        ));
        assert!(
            !granted,
            "an unreadable permission store must refuse, not fall through to the manifest"
        );

        // Control: the identical grant, with the store readable, is honoured.
        // Without this the assertion above would also pass if the grant never
        // worked at all.
        let readable = Arc::new(StubPlatform::default());
        cache_grant(&readable, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(readable.clone(), full_person_ring_resolver());
        assert!(
            futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                readable.as_ref(),
                "dim2.dot",
                "peopl.dot",
                crate::host_logic::product_manifest::Granted::Context,
            )),
            "control: the same grant must be honoured when the store reads cleanly"
        );
    }

    /// A grant lookup with nothing cached reaches the chain, and dials the
    /// Asset Hub the role was configured with.
    ///
    /// Every other grant test here seeds the manifest cache, and `root_manifest`
    /// serves that before it consults the genesis hash, so the whole suite
    /// passes on a role with no Asset Hub, and deleting the production install
    /// would not turn any of it red. That is the blind spot #660 survived in.
    /// This is the one case that takes the other branch: it asserts the dial
    /// itself, so removing the install breaks it rather than going unnoticed.
    ///
    /// The stub answers no RPC, so the lookup fails closed and the call is
    /// refused. What is pinned is that the chain was reached at all, and which
    /// chain.
    #[test]
    fn a_grant_lookup_with_a_cold_cache_dials_the_configured_asset_hub() {
        let platform = Arc::new(StubPlatform::default());
        // Deliberately no `cache_grant`: this must take the chain branch.
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
            &services,
            platform.as_ref(),
            "dim2.dot",
            "peopl.dot",
            crate::host_logic::product_manifest::Granted::Context,
        ));
        assert!(
            !granted,
            "the stub answers no RPC, so the lookup must fail closed"
        );

        let dialled = platform
            .chain_connects
            .lock()
            .expect("chain connect list mutex poisoned")
            .clone();
        assert!(
            dialled.contains(&[0xcc; 32]),
            "a cold-cache grant lookup must dial the configured Asset Hub; dialled {dialled:?}"
        );
    }

    /// The gate and the key derivation act on one identity, by construction.
    ///
    /// Previously the gate normalized the handle to decide, then handed the
    /// caller's own spelling on to derivation, and the only thing between an
    /// authorization about `peopl.dot` and a key derived from `PEOPL.DOT` was a
    /// registry lookup that happened to miss. A later "make the registry
    /// case-insensitive" change would have turned that into key confusion with
    /// nothing to catch it. The gate now returns the owner it decided about and
    /// the authority derives from that, so the two cannot diverge.
    #[test]
    fn the_gate_and_the_derivation_act_on_the_same_identity() {
        let signed = ring_vrf_sign_at_the_authority("peopl.dot", "PEOPL.DOT");
        assert!(
            signed.is_ok(),
            "an owner's own key must sign however it is spelled, because the gate \
             hands the normalized owner to the derivation; got {signed:?}"
        );
        assert_eq!(
            signed.ok(),
            ring_vrf_sign_at_the_authority("peopl.dot", "peopl.dot").ok(),
            "the two spellings must produce the same signature, not merely both succeed"
        );
    }

    /// A handle that does not normalize names no product, so it takes the same
    /// refusal as a product that granted nothing rather than a distinguishable
    /// error the caller could probe with.
    #[test]
    fn a_handle_that_does_not_normalize_takes_the_uniform_refusal() {
        assert_eq!(
            ring_vrf_sign_at_the_authority("peopl.dot", "not a product").err(),
            Some(RingVrfError::NotAllowlisted)
        );
    }

    /// Drive `ring_vrf_sign` at the authority with an arbitrary caller/handle
    /// spelling, bypassing the frontend as `sso_responder` does.
    fn ring_vrf_sign_at_the_authority(
        caller: &str,
        handle_owner: &str,
    ) -> Result<Vec<u8>, RingVrfError> {
        let platform = Arc::new(StubPlatform::default());
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform, full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        register_full_person_key(&authority, &session, &full_person_ring_location());

        futures::executor::block_on(authority.ring_vrf_sign(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: caller.to_string(),
                payload: v01::HostAccountRingVrfSignRequest {
                    key_handle: v01::ProductAccountId {
                        dot_ns_identifier: handle_owner.to_string(),
                        derivation_index: v01::DerivationIndex::Index(0),
                    },
                    message: b"sign me".to_vec(),
                },
            },
        ))
    }

    fn foreign_ring_vrf_sign_through_the_authority(
        trusted_products: Option<&str>,
    ) -> Result<Vec<u8>, RingVrfError> {
        foreign_ring_vrf_sign_through_the_authority_as(trusted_products, "dim2.dot")
    }

    /// Drive `ring_vrf_sign` straight at the authority with `caller` naming
    /// `peopl.dot`'s key handle, bypassing the frontend exactly as
    /// `sso_responder` does.
    fn foreign_ring_vrf_sign_through_the_authority_as(
        trusted_products: Option<&str>,
        caller: &str,
    ) -> Result<Vec<u8>, RingVrfError> {
        let platform = Arc::new(StubPlatform::default());
        if let Some(trusted_products) = trusted_products {
            cache_grant(&platform, "peopl.dot", trusted_products);
        }
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform, full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        register_full_person_key(&authority, &session, &full_person_ring_location());

        futures::executor::block_on(authority.ring_vrf_sign(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: caller.to_string(),
                payload: v01::HostAccountRingVrfSignRequest {
                    key_handle: full_person_key_handle(),
                    message: b"sign me".to_vec(),
                },
            },
        ))
    }

    fn foreign_proof_through_the_authority(
        trusted_products: Option<&str>,
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        foreign_proof_through_the_authority_with(trusted_products, |_| {})
    }

    /// Drive `create_proof` straight at the authority, bypassing the frontend,
    /// with `dim2.dot` naming `peopl.dot`'s key handle.
    fn foreign_proof_through_the_authority_with(
        trusted_products: Option<&str>,
        seed: impl FnOnce(&StubPlatform),
    ) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
        let platform = Arc::new(StubPlatform::default());
        if let Some(trusted_products) = trusted_products {
            cache_grant(&platform, "peopl.dot", trusted_products);
        }
        seed(&platform);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        futures::executor::block_on(authority.create_proof(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "dim2.dot".to_string(),
                payload: v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: "dim2.dot".to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location,
                    message: b"prove me".to_vec(),
                },
            },
        ))
    }

    fn foreign_proof_request(
        ring_location: &v01::RingLocation,
    ) -> truapi::versioned::account::HostAccountCreateProofRequest {
        truapi::versioned::account::HostAccountCreateProofRequest::V1(
            v01::HostAccountCreateProofRequest {
                key_handle: full_person_key_handle(),
                context: v01::ProductProofContext {
                    product_id: "dim2.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location: ring_location.clone(),
                message: b"prove me".to_vec(),
            },
        )
    }

    fn full_person_ring_location() -> v01::RingLocation {
        v01::RingLocation {
            chain_id: [0x22; 32],
            junctions: vec![
                v01::RingLocationJunction::PalletInstance(42),
                v01::RingLocationJunction::CollectionId(
                    b"pop:polkadot.network/people     ".to_vec(),
                ),
            ],
        }
    }

    fn register_full_person_key(
        authority: &SigningHostRole,
        session: &AuthoritySession,
        ring: &v01::RingLocation,
    ) {
        futures::executor::block_on(authority.register_ring_vrf_key(
            &CallContext::default(),
            session,
            ProductRequest {
                calling_product_id: "peopl.dot".to_string(),
                payload: HostAccountRegisterRingVrfKeyRequest {
                    index: v01::DerivationIndex::Index(0),
                    ring: ring.clone(),
                },
            },
        ))
        .expect("full person key registration succeeds");
    }

    #[test]
    fn internal_allowances_offer_both_reserved_person_handles_widest_first() {
        let (_, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let candidates = authority
            .reserved_person_collection_candidates(&session)
            .expect("reserved keys derive");

        // Index 0 is the full-person handle and index 1 the light-person one, and
        // People leads so a full person spends its wider slot budget first.
        let expected = [
            (PersonhoodCollection::People, 0u32),
            (PersonhoodCollection::LitePeople, 1),
        ];
        assert_eq!(candidates.len(), expected.len());
        for (candidate, (collection, index)) in candidates.iter().zip(expected) {
            assert_eq!(candidate.collection, collection);
            assert_eq!(
                candidate.entropy,
                derive_ring_vrf_entropy(&ENTROPY, "peopl.dot", &v01::DerivationIndex::Index(index))
                    .expect("reserved RFC-0024 handle derives"),
                "{collection} candidate does not use peopl.dot/{index}",
            );
        }
        assert_ne!(candidates[0].entropy, candidates[1].entropy);
    }

    #[test]
    fn reserved_identities_follow_the_configured_network_suffix() {
        // A wallet on paseo-next-v2 is the `peopl.paseo` person and the
        // `uid.paseo` account: the ones a `peopl.paseo` product registers and the
        // ones the identity backend records a lite username for. The `.dot`
        // derivations of the same seed are a different person.
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform::default());
        let authority = SigningHostRole::new_with_ring_resolver_on(
            platform,
            full_person_ring_resolver(),
            "paseo",
        );
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");

        let candidates = authority
            .reserved_person_collection_candidates(&session)
            .expect("reserved keys derive");
        for (candidate, index) in candidates.iter().zip([0u32, 1]) {
            assert_eq!(
                candidate.entropy,
                derive_ring_vrf_entropy(
                    &ENTROPY,
                    "peopl.paseo",
                    &v01::DerivationIndex::Index(index)
                )
                .expect("reserved RFC-0024 handle derives"),
                "{} candidate does not use peopl.paseo/{index}",
                candidate.collection
            );
            assert_ne!(
                candidate.entropy,
                derive_ring_vrf_entropy(&ENTROPY, "peopl.dot", &v01::DerivationIndex::Index(index))
                    .expect("reserved RFC-0024 handle derives"),
            );
        }

        let identity = derive_identity_keypair(&ENTROPY, "paseo")
            .expect("uid.paseo identity derivation")
            .public
            .to_bytes();
        assert_eq!(session.identity_account_id, Some(identity));
        assert_eq!(
            authority
                .identity_keypair()
                .expect("identity")
                .public
                .to_bytes(),
            identity
        );
    }

    #[test]
    fn ring_alias_and_proof_share_the_explicit_registered_key() {
        let resolver = full_person_ring_resolver();
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform::default());
        let authority = SigningHostRole::new_with_ring_resolver(platform, resolver);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "myapp.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let alias = futures::executor::block_on(authority.account_alias(
            &cx,
            &session,
            ProductRequest {
                calling_product_id: "peopl.dot".to_string(),
                payload: HostAccountGetAliasRequest {
                    key_handle: full_person_key_handle(),
                    context: context.clone(),
                    ring_location: ring_location.clone(),
                },
            },
        ))
        .expect("alias succeeds");
        let proof = futures::executor::block_on(authority.create_proof(
            &cx,
            &session,
            ProductRequest {
                calling_product_id: "peopl.dot".to_string(),
                payload: HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context,
                    ring_location,
                    message: b"prove me".to_vec(),
                },
            },
        ))
        .expect("proof succeeds");

        assert!(!proof.proof.is_empty());
        assert_eq!(proof.contextual_alias, alias);
        assert_eq!(proof.ring_index, 7);
        assert_eq!(proof.ring_revision, 11);
    }

    #[test]
    fn alias_checks_the_exact_registry_ring_before_resolving_it() {
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform, full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let registered_ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &registered_ring);

        let error = futures::executor::block_on(authority.account_alias(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "peopl.dot".to_string(),
                payload: HostAccountGetAliasRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: "myapp.dot".to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: v01::RingLocation {
                        chain_id: registered_ring.chain_id,
                        junctions: vec![],
                    },
                },
            },
        ))
        .unwrap_err();

        assert_eq!(error, RingVrfError::KeyNotInRing);
    }

    #[test]
    fn direct_signing_rejects_registry_public_key_mismatched_with_wallet() {
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform, full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let handle = v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(8),
        };
        futures::executor::block_on(authority.ring_vrf_registry.register(
            session.public_key,
            handle.clone(),
            full_person_ring_location(),
            [0xFF; 32],
        ))
        .expect("synthetic registry entry persists");

        let error = futures::executor::block_on(authority.ring_vrf_sign(
            &CallContext::default(),
            &session,
            ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: HostAccountRingVrfSignRequest {
                    key_handle: handle,
                    message: b"reject mismatched registry state".to_vec(),
                },
            },
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            RingVrfError::Unknown { reason } if reason.contains("does not match the active wallet")
        ));
    }

    #[test]
    fn foreign_alias_prompts_but_foreign_proof_is_refused_without_a_prompt() {
        let platform = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "other.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let alias = futures::executor::block_on(authority.account_alias(
            &cx,
            &session,
            ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: HostAccountGetAliasRequest {
                    key_handle: full_person_key_handle(),
                    context: context.clone(),
                    ring_location: ring_location.clone(),
                },
            },
        ));
        assert_eq!(alias, Err(RingVrfError::Rejected));

        let proof = futures::executor::block_on(authority.create_proof(
            &cx,
            &session,
            ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context,
                    ring_location,
                    message: b"prove me".to_vec(),
                },
            },
        ));
        assert_eq!(proof, Err(RingVrfError::NotAllowlisted));
        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("account access review list mutex poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn cross_product_alias_reuses_persisted_account_access_grant() {
        let platform = Arc::new(StubPlatform {
            account_access_confirmed: true,
            ..StubPlatform::default()
        });
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let request = ProductRequest {
            calling_product_id: "myapp.dot".to_string(),
            payload: HostAccountGetAliasRequest {
                key_handle: full_person_key_handle(),
                context: v01::ProductProofContext {
                    product_id: "other.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location: full_person_ring_location(),
            },
        };
        register_full_person_key(&authority, &session, &request.payload.ring_location);

        futures::executor::block_on(authority.account_alias(&cx, &session, request.clone()))
            .expect("first alias succeeds");
        futures::executor::block_on(authority.account_alias(&cx, &session, request))
            .expect("second alias succeeds from cached grant");

        assert_eq!(
            platform
                .account_access_reviews
                .lock()
                .expect("account access review list mutex poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn local_activation_exposes_the_uid_dot_identity_account() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");

        let session = authority.current_session().expect("active session");
        let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX)
            .expect("uid.dot identity derivation")
            .public
            .to_bytes();
        assert_eq!(session.identity_account_id, Some(identity));
    }

    #[test]
    fn activate_then_sign_raw_verifies_against_derived_product_key() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, activation);
        let cx = CallContext::default();

        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        assert!(response.signed_transaction.is_none());

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let signature =
            schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
                .is_ok(),
            "signature verifies over the <Bytes>-wrapped message",
        );
    }

    #[test]
    fn sign_vrf_replays_transcript_and_returns_verifiable_proof() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let request = v01::HostAccountSignVrfRequest {
            account: product_account(0),
            transcript_label: b"pop:airdrop".to_vec(),
            items: vec![
                v01::VrfTranscriptItem {
                    label: b"domain".to_vec(),
                    value: b"lottery".to_vec(),
                },
                v01::VrfTranscriptItem {
                    label: b"round".to_vec(),
                    value: 7u32.to_le_bytes().to_vec(),
                },
            ],
        };

        let signature = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            request,
        ))
        .expect("VRF signing succeeds");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let mut transcript = merlin::Transcript::new(b"pop:airdrop");
        transcript.append_message(b"domain", b"lottery");
        transcript.append_message(b"round", &7u32.to_le_bytes());
        let pre_output = schnorrkel::vrf::VRFPreOut::from_bytes(&signature.pre_output).unwrap();
        let proof = schnorrkel::vrf::VRFProof::from_bytes(&signature.proof).unwrap();
        keypair
            .public
            .vrf_verify(transcript, &pre_output, &proof)
            .expect("VRF proof verifies");
    }

    #[test]
    fn approved_auto_signing_product_skips_vrf_confirmation_but_other_product_does_not() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        let allocation = futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("approved AutoSigning allocation succeeds");
        let HostRequestResourceAllocationResponse::V1(allocation) = allocation;
        assert_eq!(allocation.outcomes, vec![v01::AllocationOutcome::Allocated],);
        assert_eq!(
            platform
                .resource_allocation_reviews
                .lock()
                .expect("resource allocation review list mutex poisoned")
                .len(),
            1,
        );

        let session = authority.current_session().expect("active session");
        futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect("granted product signs without another confirmation");
        assert!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .is_empty(),
            "the allocation grant bypasses only the subsequent VRF prompt",
        );

        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &session,
            "other.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("different calling product remains confirmation-bound");
        assert_eq!(error, AuthorityError::Rejected);
        let reviews = platform
            .sign_vrf_reviews
            .lock()
            .expect("VRF signing review list mutex poisoned");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].calling_product_id, "other.dot");
    }

    #[test]
    fn lock_and_wallet_replacement_release_native_custody_and_fence_old_authority() {
        futures::executor::block_on(async {
            use truapi::latest::{HostProductDeviceChatError, HostProductDeviceChatRequest};
            let (services, authority) = signing_runtime();
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            crate::host_logic::permissions::PermissionsService::new(
                services.platform.as_ref(),
                services.platform.as_ref(),
                "chat.dot",
            )
            .set_authorization_status(
                &PermissionAuthorizationRequest::ChatAuthority,
                PermissionAuthorizationStatus::Authorized,
            )
            .await
            .unwrap();
            let session = authority.current_local_session().unwrap();
            let context = authority.native_chat_context(&session).unwrap();
            let first = authority
                .native_chat
                .execute(
                    context.clone(),
                    "chat.dot".into(),
                    HostProductDeviceChatRequest::Initialize,
                )
                .await
                .unwrap();
            let wallet = authority.native_chat.wallet(&context).await.unwrap();
            let lifetime = Arc::downgrade(&wallet);
            drop(wallet);
            authority.clear_local_session();
            assert!(lifetime.upgrade().is_none());
            assert_eq!(
                context.require_current(),
                Err(HostProductDeviceChatError::NotConnected)
            );
            assert_eq!(
                authority
                    .native_chat
                    .execute(
                        context,
                        "chat.dot".into(),
                        HostProductDeviceChatRequest::Initialize,
                    )
                    .await,
                Err(HostProductDeviceChatError::NotConnected)
            );

            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let session = authority.current_local_session().unwrap();
            let context = authority.native_chat_context(&session).unwrap();
            let restored = authority
                .native_chat
                .execute(
                    context.clone(),
                    "chat.dot".into(),
                    HostProductDeviceChatRequest::Initialize,
                )
                .await
                .unwrap();
            assert_eq!(restored.device, first.device);
            let wallet = authority.native_chat.wallet(&context).await.unwrap();
            let lifetime = Arc::downgrade(&wallet);
            drop(wallet);
            authority
                .activate_local_session(vec![0xCD; 16])
                .await
                .unwrap();
            assert!(lifetime.upgrade().is_none());
            assert_eq!(
                context.require_current(),
                Err(HostProductDeviceChatError::NotConnected)
            );
        });
    }

    #[test]
    fn product_clear_revokes_only_current_activation_grant_and_fences_stale_work() {
        let platform = Arc::new(StubPlatform::default());
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let stale_session = authority.current_session().expect("active session");
        authority
            .grant_auto_signing(&stale_session, "myapp.dot")
            .expect("first product grant succeeds");
        authority
            .grant_auto_signing(&stale_session, "other.dot")
            .expect("other product grant succeeds");
        let context = authority.native_chat_context(&stale_session).unwrap();
        let wallet = futures::executor::block_on(authority.native_chat.wallet(&context)).unwrap();
        let custody = Arc::downgrade(&wallet);
        drop(wallet);

        futures::executor::block_on(authority.clear_product_state("myapp.dot"))
            .expect("product clear succeeds");
        assert!(
            custody.upgrade().is_some(),
            "product clear must preserve wallet custody"
        );

        let current_session = authority.current_session().expect("session remains active");
        let (_, current_generation) = authority
            .require_current_session(&current_session)
            .expect("current session validates");
        assert!(!authority.has_auto_signing_grant(
            current_generation,
            current_session.public_key,
            "myapp.dot",
            "myapp.dot",
        ));
        assert!(authority.has_auto_signing_grant(
            current_generation,
            current_session.public_key,
            "other.dot",
            "other.dot",
        ));
        assert!(matches!(
            authority.grant_auto_signing(&stale_session, "myapp.dot"),
            Err(AuthorityError::Disconnected)
        ));
    }

    #[test]
    fn auto_signing_grant_does_not_cross_root_identity_replacement() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
            .expect("replacement activation succeeds");
        let replacement = authority.current_session().expect("replacement session");
        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &replacement,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("replacement root must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn auto_signing_grant_does_not_cross_disconnect_and_same_wallet_reactivation() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let runtime = product_runtime(services, authority.clone());
        futures::executor::block_on(ResourceAllocation::request(
            &runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        futures::executor::block_on(authority.disconnect());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("same wallet reactivation succeeds");
        let reactivated = authority.current_session().expect("reactivated session");
        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &reactivated,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("reactivated wallet must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn stale_auto_signing_completion_cannot_grant_same_wallet_reactivation() {
        let platform = Arc::new(StubPlatform {
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation succeeds");
        let stale = authority.current_session().expect("first session snapshot");

        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("same wallet replacement activation succeeds");
        let current = authority.current_session().expect("replacement session");
        assert_ne!(stale.validation_id, current.validation_id);

        let error = futures::executor::block_on(authority.allocate_resources(
            &CallContext::default(),
            &stale,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            },
        ))
        .expect_err("completion captured from the old activation is stale");
        assert_eq!(error, AuthorityError::Disconnected);

        let error = futures::executor::block_on(authority.sign_vrf(
            &CallContext::default(),
            &current,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("stale allocation must not grant the replacement activation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn auto_signing_grant_does_not_cross_runtime_instance() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            sign_vrf_confirmed: false,
            ..StubPlatform::default()
        });
        let (services, granting_authority) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(granting_authority.activate_local_session(ENTROPY.to_vec()))
            .expect("granting runtime activates");
        let granting_runtime = product_runtime(services, granting_authority);
        futures::executor::block_on(ResourceAllocation::request(
            &granting_runtime,
            &CallContext::default(),
            HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            }),
        ))
        .expect("AutoSigning allocation succeeds");

        let (_replacement_services, replacement) = signing_runtime_with_platform(platform.clone());
        futures::executor::block_on(replacement.activate_local_session(ENTROPY.to_vec()))
            .expect("replacement runtime activates with the same root");
        let session = replacement.current_session().expect("replacement session");
        let error = futures::executor::block_on(replacement.sign_vrf(
            &CallContext::default(),
            &session,
            "myapp.dot".to_string(),
            vrf_request("myapp.dot"),
        ))
        .expect_err("a separate runtime must receive its own confirmation");
        assert_eq!(error, AuthorityError::Rejected);
        assert_eq!(
            platform
                .sign_vrf_reviews
                .lock()
                .expect("VRF signing review list mutex poisoned")
                .len(),
            1,
        );
    }

    #[test]
    fn sign_payload_product_and_legacy_use_the_substrate_preimage() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let mut payload = crate::test_support::sign_payload_data();
        payload.signed_extensions = vec![
            "CheckSpecVersion".to_string(),
            "CheckTxVersion".to_string(),
            "CheckGenesis".to_string(),
            "CheckMortality".to_string(),
            "CheckNonce".to_string(),
            "ChargeTransactionPayment".to_string(),
        ];
        payload.with_signed_transaction = parity_scale_codec::OptionBool(Some(true));
        let preimage = extrinsic_payload_preimage(&payload).expect("preimage builds");

        let product_response = futures::executor::block_on(authority.sign_payload(
            &cx,
            &session,
            None,
            SignPayloadAuthorityRequest::Product(v01::HostSignPayloadRequest {
                account: product_account(0),
                payload: payload.clone(),
            }),
        ))
        .expect("product payload signing succeeds");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        assert_eq!(product_response.signature.len(), 65);
        assert_eq!(product_response.signature[0], 1);
        let signature =
            schnorrkel::Signature::from_bytes(&product_response.signature[1..]).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
                .is_ok()
        );
        let signed_transaction = product_response
            .signed_transaction
            .as_ref()
            .expect("requested signed transaction");
        let (account, embedded_signature, tail) = split_v4(signed_transaction);
        assert_eq!(account, keypair.public.to_bytes());
        assert_eq!(
            embedded_signature.as_slice(),
            &product_response.signature[1..]
        );
        let extensions = extrinsic_payload_extensions(&payload).unwrap();
        let expected_tail = extensions
            .iter()
            .flat_map(|extension| extension.extra.iter().copied())
            .chain(payload.method.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(tail, expected_tail);

        let legacy_response = futures::executor::block_on(authority.sign_payload(
            &cx,
            &session,
            None,
            SignPayloadAuthorityRequest::LegacyAccount {
                product_account: product_account(0),
                request: v01::HostSignPayloadWithLegacyAccountRequest {
                    signer: format!("0x{}", hex::encode(keypair.public.to_bytes())),
                    payload,
                },
            },
        ))
        .expect("legacy payload signing succeeds");
        assert_eq!(legacy_response.signature[0], 1);
        let signature = schnorrkel::Signature::from_bytes(&legacy_response.signature[1..]).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
                .is_ok()
        );
        assert!(legacy_response.signed_transaction.is_some());
    }

    #[test]
    fn sign_raw_legacy_accepts_only_the_uid_dot_identity_key() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority.current_session().expect("active session");
        let cx = CallContext::default();
        let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
        let request = |account| SignRawAuthorityRequest::LegacyAccount {
            account,
            request: v01::HostSignRawWithLegacyAccountRequest {
                signer: String::new(),
                payload: v01::RawPayload::Bytes {
                    bytes: b"hello".to_vec(),
                },
            },
        };

        let response = futures::executor::block_on(authority.sign_raw(
            &cx,
            &session,
            None,
            request(identity.public.to_bytes()),
            true,
        ))
        .expect("identity raw signing succeeds");
        let signature = schnorrkel::Signature::from_bytes(&response.signature).unwrap();
        assert!(
            identity
                .public
                .verify_simple(SR25519_SIGNING_CONTEXT, b"<Bytes>hello</Bytes>", &signature)
                .is_ok()
        );

        let error = futures::executor::block_on(authority.sign_raw(
            &cx,
            &session,
            None,
            request([0xff; 32]),
            true,
        ))
        .expect_err("unknown legacy account is rejected");
        assert!(matches!(error, AuthorityError::Unavailable { .. }));
    }

    #[test]
    fn sign_raw_requires_active_session() {
        let (services, authority) = signing_runtime();
        let runtime = product_runtime(services, authority);
        let cx = CallContext::default();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: vec![1, 2, 3],
            },
        });
        let err =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect_err("no session");
        assert!(matches!(err, CallError::Domain(HostSignRawError::V1(_))));
    }

    fn product_account(index: u32) -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(index),
        }
    }

    fn tx_payload(tx_ext_version: u8) -> v01::ProductAccountTxPayload {
        v01::ProductAccountTxPayload {
            signer: product_account(0),
            genesis_hash: [0xaa; 32],
            call_data: vec![0x00, 0x00],
            extensions: vec![v01::TxPayloadExtension {
                id: "CheckNonce".to_string(),
                extra: vec![1],
                additional_signed: vec![2, 3],
            }],
            tx_ext_version,
        }
    }

    #[test]
    fn create_transaction_product_builds_verifiable_v4() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::default();

        let response = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            None,
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect("create_transaction ok");

        let (account, signature, tail) = split_v4(&response.transaction);
        assert_eq!(tail, vec![1, 0x00, 0x00], "body tail is extra ++ call_data");

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        assert_eq!(account, keypair.public.to_bytes());

        // Payload = call_data ++ extra ++ additional_signed (call first).
        let payload = vec![0x00, 0x00, 1, 2, 3];
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", &payload, &signature)
                .is_ok()
        );
    }

    #[test]
    fn create_transaction_rejects_unknown_tx_ext_version() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::default();

        let err = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            None,
            CreateTransactionAuthorityRequest::Product(tx_payload(1)),
        ))
        .expect_err("unknown transaction version is unsupported");
        assert!(
            matches!(err, AuthorityError::NotSupported { reason } if reason.contains("tx_ext_version 1"))
        );
    }

    #[test]
    fn create_transaction_v5_reaches_chain_metadata_resolution() {
        let platform: Arc<dyn truapi_platform::Platform> = Arc::new(StubPlatform {
            chain_connect_error: Some("fixture has no live chain"),
            ..StubPlatform::default()
        });
        let (_services, activation) = signing_runtime_with_platform(platform);
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::default();

        let err = futures::executor::block_on(activation.create_transaction(
            &cx,
            &session,
            None,
            CreateTransactionAuthorityRequest::Product(tx_payload(5)),
        ))
        .expect_err("fixture cannot resolve metadata");
        assert!(
            matches!(err, AuthorityError::Unavailable { reason } if reason.contains("cannot load V5 chain metadata")),
            "V5 must pass the former NotSupported gate and attempt metadata resolution"
        );
    }

    #[test]
    fn create_transaction_legacy_signer_mismatch_errors() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::default();

        let payload = tx_payload(0);
        let request = CreateTransactionAuthorityRequest::LegacyAccount {
            product_account: product_account(0),
            request: v01::LegacyAccountTxPayload {
                signer: [0xff; 32], // does not match the derived slot-zero key
                genesis_hash: payload.genesis_hash,
                call_data: payload.call_data.clone(),
                extensions: payload.extensions.clone(),
                tx_ext_version: 0,
            },
        };
        let err = futures::executor::block_on(
            activation.create_transaction(&cx, &session, None, request),
        )
        .expect_err("mismatched legacy signer");
        assert!(
            matches!(err, AuthorityError::Unknown { reason } if reason.contains("does not match"))
        );
    }

    #[test]
    fn create_transaction_legacy_builds_verifiable_v4() {
        let (_services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation.current_session().expect("active session");
        let cx = CallContext::default();

        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();

        let request = CreateTransactionAuthorityRequest::LegacyAccount {
            product_account: product_account(0),
            request: v01::LegacyAccountTxPayload {
                signer: keypair.public.to_bytes(), // matches the derived slot-zero key
                genesis_hash: [0xaa; 32],
                call_data: vec![0x00, 0x00],
                extensions: vec![v01::TxPayloadExtension {
                    id: "CheckNonce".to_string(),
                    extra: vec![1],
                    additional_signed: vec![2, 3],
                }],
                tx_ext_version: 0,
            },
        };
        let response = futures::executor::block_on(
            activation.create_transaction(&cx, &session, None, request),
        )
        .expect("legacy create_transaction ok");

        let (account, signature, tail) = split_v4(&response.transaction);
        assert_eq!(account, keypair.public.to_bytes());
        assert_eq!(tail, vec![1, 0x00, 0x00]);
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", &[0x00, 0x00, 1, 2, 3], &signature)
                .is_ok()
        );
    }

    #[test]
    fn create_transaction_requires_active_session() {
        let (_services, activation) = signing_runtime();
        // A session snapshot cannot exist without activation, so construct the
        // request against a role that has never been activated.
        let (_s2, other) = signing_runtime();
        futures::executor::block_on(other.activate_local_session(ENTROPY.to_vec())).unwrap();
        let stale_session = other.current_session().expect("session");
        futures::executor::block_on(other.disconnect());
        let cx = CallContext::default();

        let err = futures::executor::block_on(activation.create_transaction(
            &cx,
            &stale_session,
            None,
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect_err("no active session");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    #[test]
    fn derive_entropy_matches_ios_vector_over_local_session() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime_for(services, activation, "test.product.dot");
        let cx = CallContext::default();
        let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
            context: b"my-key".to_vec(),
        });
        let response =
            futures::executor::block_on(runtime.derive(&cx, request)).expect("derive ok");
        let truapi::versioned::entropy::HostDeriveEntropyResponse::V1(inner) = response;
        assert_eq!(
            hex::encode(inner.entropy),
            "479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a",
        );
    }

    #[test]
    fn get_account_gates_on_local_session() {
        let (services, authority) = signing_runtime();
        let runtime = product_runtime(services, authority);
        let cx = CallContext::default();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
        });
        let err = futures::executor::block_on(runtime.get_account(&cx, request))
            .expect_err("no session yet");
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected
            ))
        ));
    }

    #[test]
    fn sign_raw_leaves_already_wrapped_payload_untouched() {
        let (services, activation) = signing_runtime();
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let runtime = product_runtime(services, activation);
        let cx = CallContext::default();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"<Bytes>hi</Bytes>".to_vec(),
            },
        });
        let HostSignRawResponse::V1(response) =
            futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
        let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
        let keypair = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
        let signature =
            schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
        assert!(
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>hi</Bytes>", &signature)
                .is_ok(),
            "signature verifies over the unchanged wrapped message",
        );
        assert!(
            keypair
                .public
                .verify_simple(
                    b"substrate",
                    b"<Bytes><Bytes>hi</Bytes></Bytes>",
                    &signature
                )
                .is_err(),
            "payload was not double-wrapped",
        );
    }

    #[test]
    fn chat_context_rejects_logout_and_same_wallet_reactivation() {
        futures::executor::block_on(async {
            let (_, authority) = signing_runtime();
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let session = authority.current_session().unwrap();
            let context = authority.native_chat_context(&session).unwrap();
            assert!((context.session_valid)());
            authority.disconnect().await;
            assert!(!(context.session_valid)());
            authority
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            assert!(!(context.session_valid)());
            assert!(matches!(
                authority
                    .product_device_chat(
                        &CallContext::default(),
                        &session,
                        ProductDeviceChatAuthorityRequest {
                            calling_product_id: "myapp.dot".to_string(),
                            operation: truapi::latest::HostProductDeviceChatRequest::Initialize,
                        },
                    )
                    .await,
                Err(ProductDeviceChatAuthorityError::Disconnected)
            ));
            let current = authority.current_session().unwrap();
            assert!((authority
                .native_chat_context(&current)
                .unwrap()
                .session_valid)());
        });
    }

    #[test]
    fn product_chat_username_grant_does_not_authorize_chat() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform::default());
            let (services, activation) = signing_runtime_with_platform(platform.clone());
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let runtime = product_runtime(services, activation);
            runtime
                .set_permission_authorization_status(
                    PermissionAuthorizationRequest::IdentityDisclosure,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            let cx = CallContext::default();
            let request = HostProductDeviceChatRequest::V2(
                truapi::latest::HostProductDeviceChatRequest::Initialize,
            );

            for _ in 0..2 {
                assert!(matches!(
                    runtime.product_device_chat(&cx, request.clone()).await,
                    Err(CallError::Domain(HostProductDeviceChatError::V1(
                        truapi::latest::HostProductDeviceChatError::AccessNotGranted
                    )))
                ));
            }
            assert_eq!(
                platform.chat_authority_reviews.lock().len(),
                1,
                "Chat requires its own prompt, then respects the persisted refusal"
            );
            assert_eq!(
                runtime
                    .permission_authorization_status(PermissionAuthorizationRequest::ChatAuthority)
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Denied
            );
            assert_eq!(
                runtime
                    .permission_authorization_status(
                        PermissionAuthorizationRequest::IdentityDisclosure,
                    )
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
        });
    }

    #[test]
    fn product_chat_consent_is_cached_and_revocation_blocks_chat() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform {
                chat_authority_confirmed: true,
                ..StubPlatform::default()
            });
            let (services, activation) = signing_runtime_with_platform(platform.clone());
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let runtime = product_runtime(services, activation);
            let cx = CallContext::default();
            let initialize = HostProductDeviceChatRequest::V2(
                truapi::latest::HostProductDeviceChatRequest::Initialize,
            );
            let HostProductDeviceChatResponse::V2(first) = runtime
                .product_device_chat(&cx, initialize.clone())
                .await
                .unwrap();
            let HostProductDeviceChatResponse::V2(second) = runtime
                .product_device_chat(&cx, initialize.clone())
                .await
                .unwrap();
            assert_eq!(first.device, second.device);
            assert_eq!(first.device.product_account.dot_ns_identifier, "myapp.dot");
            assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
            runtime
                .set_permission_authorization_status(
                    PermissionAuthorizationRequest::ChatAuthority,
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            assert!(matches!(
                runtime.product_device_chat(&cx, initialize).await,
                Err(CallError::Domain(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::AccessNotGranted
                )))
            ));
            assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
        });
    }

    #[test]
    fn product_chat_authority_validation_does_not_require_statement_delivery() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform {
                chat_authority_confirmed: true,
                ..StubPlatform::default()
            });
            let (services, activation) = signing_runtime_with_platform(platform);
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let runtime = product_runtime(services, activation);
            runtime
                .set_permission_authorization_status(
                    PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest {
                        permission: v01::RemotePermission::StatementSubmit,
                    }),
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            let request = HostProductDeviceChatRequest::V2(
                truapi::latest::HostProductDeviceChatRequest::SendPayment {
                    peer_identity: [0x55; 32],
                    request_id: "not-authorized".to_string(),
                    amount_cents: 10,
                },
            );
            assert!(matches!(
                runtime
                    .product_device_chat(&CallContext::default(), request)
                    .await,
                Err(CallError::Domain(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::PeerNotReady
                )))
            ));
            let request = HostProductDeviceChatRequest::V2(
                truapi::latest::HostProductDeviceChatRequest::Prepare {
                    peer_identity: [0x55; 32],
                    route: truapi::latest::HostNativeChatRoute::Identity,
                    plaintext: vec![],
                },
            );
            assert!(matches!(
                runtime
                    .product_device_chat(&CallContext::default(), request)
                    .await,
                Err(CallError::Domain(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::InvalidRequest
                )))
            ));
        });
    }

    async fn sso_chat(
        service: &super::sso_service::SigningHostSsoService,
        payload: SsoProductDeviceChatOperation,
    ) -> ProductDeviceChatResponse {
        let message = RemoteMessage::request(
            "chat-consent".to_string(),
            ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload,
            },
        );
        let Dispatch::Response(answer) = service.dispatch(service.current_session(), message).await
        else {
            panic!("expected SSO response");
        };
        let RemoteMessageData::V1(v1::RemoteMessage::ProductDeviceChatResponse(response)) =
            answer.message.data
        else {
            panic!("expected SSO Chat response");
        };
        response.payload
    }

    #[test]
    fn sso_chat_username_grant_does_not_authorize_chat() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform::default());
            let (_, activation) = signing_runtime_with_platform(platform.clone());
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let service = super::sso_service::SigningHostSsoService::new(activation);
            let permissions =
                PermissionsService::new(platform.as_ref(), platform.as_ref(), "myapp.dot");
            permissions
                .set_authorization_status(
                    &PermissionAuthorizationRequest::IdentityDisclosure,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            let request = SsoProductDeviceChatOperation::V3(
                truapi::latest::HostProductDeviceChatRequest::Initialize,
            );

            for _ in 0..2 {
                assert_eq!(
                    sso_chat(&service, request.clone()).await,
                    Err(HostProductDeviceChatError::V1(
                        truapi::latest::HostProductDeviceChatError::AccessNotGranted,
                    ))
                );
            }
            assert_eq!(
                platform.chat_authority_reviews.lock().len(),
                1,
                "SSO Chat requires its own prompt, then respects the persisted refusal"
            );
            assert_eq!(
                permissions
                    .authorization_status(&PermissionAuthorizationRequest::ChatAuthority)
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Denied
            );
            assert_eq!(
                permissions
                    .authorization_status(&PermissionAuthorizationRequest::IdentityDisclosure)
                    .await
                    .unwrap(),
                PermissionAuthorizationStatus::Authorized
            );
        });
    }

    #[test]
    fn sso_chat_consent_is_cached_and_revocation_blocks_chat() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform {
                chat_authority_confirmed: true,
                ..StubPlatform::default()
            });
            let (_, activation) = signing_runtime_with_platform(platform.clone());
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let service = super::sso_service::SigningHostSsoService::new(activation);
            let permissions =
                PermissionsService::new(platform.as_ref(), platform.as_ref(), "myapp.dot");
            let initialize = SsoProductDeviceChatOperation::V3(
                truapi::latest::HostProductDeviceChatRequest::Initialize,
            );
            let HostProductDeviceChatResponse::V2(first) =
                sso_chat(&service, initialize.clone()).await.unwrap();
            let HostProductDeviceChatResponse::V2(second) =
                sso_chat(&service, initialize.clone()).await.unwrap();
            assert_eq!(first.device, second.device);
            assert_eq!(first.device.product_account.dot_ns_identifier, "myapp.dot");
            assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
            permissions
                .set_authorization_status(
                    &PermissionAuthorizationRequest::ChatAuthority,
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            assert_eq!(
                sso_chat(&service, initialize).await,
                Err(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::AccessNotGranted,
                ))
            );
            assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
        });
    }

    #[test]
    fn sso_chat_authority_validation_does_not_require_statement_delivery() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform {
                chat_authority_confirmed: true,
                ..StubPlatform::default()
            });
            let (_, activation) = signing_runtime_with_platform(platform.clone());
            activation
                .activate_local_session(ENTROPY.to_vec())
                .await
                .unwrap();
            let service = super::sso_service::SigningHostSsoService::new(activation);
            let permissions =
                PermissionsService::new(platform.as_ref(), platform.as_ref(), "myapp.dot");
            permissions
                .set_authorization_status(
                    &PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest {
                        permission: v01::RemotePermission::StatementSubmit,
                    }),
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            let request = SsoProductDeviceChatOperation::V3(
                truapi::latest::HostProductDeviceChatRequest::SendPayment {
                    peer_identity: [0x55; 32],
                    request_id: "not-authorized".to_string(),
                    amount_cents: 10,
                },
            );
            assert_eq!(
                sso_chat(&service, request).await,
                Err(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::PeerNotReady,
                ))
            );
            let request = SsoProductDeviceChatOperation::V3(
                truapi::latest::HostProductDeviceChatRequest::Prepare {
                    peer_identity: [0x55; 32],
                    route: truapi::latest::HostNativeChatRoute::Identity,
                    plaintext: vec![],
                },
            );
            assert_eq!(
                sso_chat(&service, request).await,
                Err(HostProductDeviceChatError::V1(
                    truapi::latest::HostProductDeviceChatError::InvalidRequest,
                ))
            );
        });
    }

    #[test]
    fn reactivation_invalidates_prior_session_snapshot() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation");
        let stale = authority.current_session().expect("snapshot");

        // Re-activate with different entropy: a fresh public key, hence a
        // different validation id.
        futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
            .expect("second activation");
        assert_ne!(
            authority.current_session().expect("session").public_key,
            stale.public_key,
        );

        let cx = CallContext::default();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: vec![1, 2, 3],
            },
        };
        let err = futures::executor::block_on(authority.sign_raw(
            &cx,
            &stale,
            None,
            SignRawAuthorityRequest::Product(request),
            true,
        ))
        .expect_err("stale snapshot rejected");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    #[test]
    fn disconnect_clears_local_session() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");

        futures::executor::block_on(authority.disconnect());
        assert!(authority.current_session().is_none());

        let cx = CallContext::default();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes { bytes: vec![1] },
        };
        let err = futures::executor::block_on(authority.sign_raw(
            &cx,
            &session,
            None,
            SignRawAuthorityRequest::Product(request),
            true,
        ))
        .expect_err("no session after disconnect");
        assert_eq!(err, AuthorityError::Disconnected);
    }

    /// A PGAS request has to actually reach Asset Hub. Asserting the outcome alone
    /// cannot show that: the stub this replaced answered `NotAvailable` with no
    /// chain access at all, which looks identical from outside. Pin instead that
    /// the request is left waiting on the connection.
    #[test]
    fn a_pgas_request_waits_on_the_asset_hub_connection() {
        use std::future::Future;
        use std::task::{Context, Poll};

        let platform = Arc::new(StubPlatform {
            chain_connect_pending: true,
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::default();

        let mut allocation = Box::pin(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::SmartContractAllowance(
                    v01::DerivationIndex::Index(0),
                )],
            },
        ));
        let waker = futures::task::noop_waker();
        let mut task_cx = Context::from_waker(&waker);

        match allocation.as_mut().poll(&mut task_cx) {
            Poll::Pending => {}
            Poll::Ready(outcome) => {
                panic!("a PGAS claim should be waiting on Asset Hub, got {outcome:?}")
            }
        }
    }

    #[test]
    fn direct_allocation_handles_empty_and_optional_resource_batches() {
        // A PGAS request now reaches Asset Hub, so the stub has to fail the
        // connect for this to be deterministic. What the batch pins is that one
        // resource failing does not poison the others.
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("asset hub unavailable"),
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.current_session().expect("connected");
        let cx = CallContext::default();

        let empty = futures::executor::block_on(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest { resources: vec![] },
        ))
        .expect("empty allocation succeeds");
        assert!(empty.outcomes.is_empty());

        let optional = futures::executor::block_on(authority.allocate_resources(
            &cx,
            &session,
            "myapp.dot".to_string(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![
                    v01::AllocatableResource::SmartContractAllowance(v01::DerivationIndex::Index(
                        0,
                    )),
                    v01::AllocatableResource::AutoSigning,
                ],
            },
        ))
        .expect("optional allocation succeeds");
        assert_eq!(
            optional.outcomes,
            vec![
                // The chain is unreachable, so the claim degrades rather than
                // failing the request.
                v01::AllocationOutcome::NotAvailable,
                // And the resource that needs no chain still allocates.
                v01::AllocationOutcome::Allocated,
            ]
        );
    }
}
