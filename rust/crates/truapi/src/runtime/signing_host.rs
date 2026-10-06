//! Native wallet lifecycle, shared host accounts and incoming SSO.
//!
//! Wallet replacement and retained grants use the same lifecycle guard.

mod local_activation;
mod ring_vrf;
pub use ring_vrf::{
    ChainRingResolver, MemberCandidate, RingResolver, create_proof, development_context_bytes,
};
mod sso_replay;
mod sso_responder;
mod wallet_account_holder;

use std::sync::Arc;

#[cfg(any(test, feature = "test-host"))]
pub use local_activation::LocalActivation;
pub use sso_responder::{
    AnnouncedPairing, DevicePairingObserver, MAX_PAIRING_METADATA_CHARS, PairedSsoPeer,
    PairingProposal, PairingProposalMetadata, ResponderExit,
};
pub use sso_responder::{
    disconnect_paired_host, establish_pairing, notify_pairing_allowance_allocation,
    notify_pairing_failed, respond_to_pairing, resume_pairing,
};
#[cfg(not(target_arch = "wasm32"))]
pub use wallet_account_holder::TrackedStatementRenewalTarget;
pub use wallet_account_holder::{StatementRenewalTarget, WalletAccountHolder};

#[cfg(not(target_arch = "wasm32"))]
use super::allowances::current_unix_secs;
use super::authority::AuthorityError;
use super::ring_vrf_registry::RingVrfRegistryStore;
use super::{
    HostAccounts, HostGrantStore, HostSession, RuntimeServices, connected_session_ui_info,
};
use crate::host_logic::session::SessionState;
use crate::runtime::auth_state::AuthStateMachine;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_allowance;
use sso_replay::SsoReplayLocks;

/// The network suffix the unit tests configure their signing host for. `dot`
/// keeps the `peopl.dot` handles the RFC examples use meaningful; the
/// per-network behaviour has its own tests.
#[cfg(test)]
const TEST_NETWORK_SUFFIX: &str = "dot";

#[cfg(test)]
use crate::platform::Platform;
use crate::platform::{ProductContext, normalize_product_identifier};
use truapi::CallError;
use truapi::latest::{HostRequestLoginError, HostRequestLoginResponse};

/// Native host lifecycle and retained grants over a shared wallet.
pub struct SigningHost {
    #[cfg(any(not(target_arch = "wasm32"), test))]
    services: Arc<RuntimeServices>,
    wallet: Arc<WalletAccountHolder>,
    auth_state: AuthStateMachine,
    grants: Arc<HostGrantStore>,
    accounts: Arc<HostAccounts<WalletAccountHolder>>,
    /// Serializes replay-ledger updates within each wallet and peer scope.
    sso_replay_locks: SsoReplayLocks,
    #[cfg(not(target_arch = "wasm32"))]
    renewal_loop_started: std::sync::atomic::AtomicBool,
}

impl SigningHost {
    /// Wallet shared by native account operations and incoming SSO.
    pub fn account_holder(&self) -> &Arc<WalletAccountHolder> {
        &self.wallet
    }

    /// Product account policy and grants shared by native connections.
    pub fn accounts(&self) -> &Arc<HostAccounts<WalletAccountHolder>> {
        &self.accounts
    }

    /// Build a locked signing host for the network's dotNS suffix.
    pub fn new(services: Arc<RuntimeServices>, network_suffix: String) -> Arc<Self> {
        let platform = services.platform.clone();
        let registry = RingVrfRegistryStore::new(platform.clone());
        let grants = Arc::new(HostGrantStore::new(platform.clone(), platform.clone()));
        let wallet = Arc::new(WalletAccountHolder::new(
            services.clone(),
            network_suffix,
            registry.clone(),
        ));
        let accounts = HostAccounts::new(
            services.clone(),
            wallet.clone(),
            wallet.session_state(),
            grants.clone(),
            registry,
            #[cfg(feature = "test-host")]
            wallet.resource_controls().clone(),
        );
        Arc::new(Self {
            #[cfg(any(not(target_arch = "wasm32"), test))]
            services,
            wallet,
            auth_state: AuthStateMachine::new(platform),
            grants,
            accounts,
            sso_replay_locks: SsoReplayLocks::default(),
            #[cfg(not(target_arch = "wasm32"))]
            renewal_loop_started: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Answer resource allocation as granted without performing it in test hosts.
    #[cfg(feature = "test-host")]
    pub fn set_grant_allowances_unchecked(&self, granted: bool) {
        self.wallet
            .resource_controls()
            .set_grant_allowances_unchecked(granted);
    }

    /// Replace resource tags refused by this test host.
    #[cfg(feature = "test-host")]
    pub fn set_withheld_resources(&self, tags: Vec<String>) {
        self.wallet.resource_controls().set_withheld_resources(tags);
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
            crate::platform::HostInfo {
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
        let registry = RingVrfRegistryStore::new(platform.clone());
        let grants = Arc::new(HostGrantStore::new(platform.clone(), platform.clone()));
        let wallet = Arc::new(WalletAccountHolder::new_with_ring_resolver(
            services.clone(),
            network_suffix.to_string(),
            ring_resolver,
            registry.clone(),
        ));
        let accounts = HostAccounts::new(
            services.clone(),
            wallet.clone(),
            wallet.session_state(),
            grants.clone(),
            registry,
            #[cfg(feature = "test-host")]
            wallet.resource_controls().clone(),
        );
        Arc::new(Self {
            services,
            wallet,
            auth_state: AuthStateMachine::new(platform),
            grants,
            accounts,
            sso_replay_locks: SsoReplayLocks::default(),
            #[cfg(not(target_arch = "wasm32"))]
            renewal_loop_started: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn sso_replay_locks(&self) -> &SsoReplayLocks {
        &self.sso_replay_locks
    }

    /// Revoke one product's grants while preserving the active wallet.
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), AuthorityError> {
        let product_id = normalize_product_identifier(product_id).map_err(|error| {
            AuthorityError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        self.grants.lifecycle().revoke_native_product(&product_id);
        let storage = self.grants.persistence().await;
        storage.begin_cleanup();
        storage
            .drain_cleanup()
            .await
            .map_err(|reason| AuthorityError::Unavailable { reason })
    }

    /// Invalidate wallet work and drop secrets without deleting durable records.
    pub fn lock_wallet(&self) {
        {
            let mut state = self.grants.lifecycle();
            state.clear_memory();
            self.wallet.clear();
        }
        self.auth_state.store_disconnected();
    }

    /// Permanently invalidate this wallet owner and its pending activations.
    pub fn retire(&self) {
        {
            let mut state = self.grants.lifecycle();
            state.clear_memory();
            self.wallet.retire();
        }
        self.auth_state.store_disconnected();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SigningHost {
    /// Record statement-store accounts to keep renewed across periods.
    pub async fn track_statement_renewal_targets(
        &self,
        targets: Vec<StatementRenewalTarget>,
    ) -> Result<(), String> {
        self.wallet.track_statement_renewal_targets(targets).await
    }

    /// Every statement account the ledger currently tracks.
    pub async fn statement_renewal_targets(
        &self,
    ) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
        self.wallet.statement_renewal_targets().await
    }

    /// Root public key the active identity records its fixed entries under.
    pub fn statement_renewal_owner_key(&self) -> Result<[u8; 32], String> {
        self.wallet.statement_renewal_owner_key()
    }

    /// Stop renewing one fixed statement account.
    pub async fn untrack_statement_renewal_account(
        &self,
        account_id: &[u8; 32],
    ) -> Result<bool, String> {
        self.wallet
            .untrack_statement_renewal_account(account_id)
            .await
    }

    /// Run one statement-store renewal pass over the tracked targets.
    pub async fn renew_statement_allowances(
        &self,
    ) -> Result<crate::runtime::statement_allowance::renewal::StatementRenewalReport, String> {
        self.wallet.renew_statement_allowances().await
    }

    /// The most recent pass the in-process loop ran.
    ///
    /// A direct call to [`Self::renew_statement_allowances`] returns its own
    /// report, so only the loop needs somewhere to leave one.
    pub fn last_statement_renewal_report(
        &self,
    ) -> Option<crate::runtime::statement_allowance::renewal::StatementRenewalReport> {
        self.wallet.last_statement_renewal_report()
    }

    /// Start the periodic statement-store renewal loop. Idempotent.
    pub fn start_statement_allowance_renewal(self: &Arc<Self>) {
        const CLOCK_FAILURE_TICK_DELAY: core::time::Duration =
            core::time::Duration::from_secs(3_600);
        if self
            .renewal_loop_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let weak_host = Arc::downgrade(self);
        (self.services.spawner)(Box::pin(async move {
            loop {
                {
                    let Some(host) = weak_host.upgrade() else {
                        return;
                    };
                    host.wallet.renewal_tick().await;
                }
                let delay = match current_unix_secs() {
                    Ok(now) => statement_allowance::renewal::next_tick_delay(now),
                    Err(_) => CLOCK_FAILURE_TICK_DELAY,
                };
                futures_timer::Delay::new(delay).await;
            }
        }));
    }
}

#[crate::platform::async_trait]
impl HostSession for SigningHost {
    /// Shared session holder for connection-status subscriptions.
    fn session_state(&self) -> Arc<SessionState> {
        self.wallet.session_state()
    }

    async fn request_login(
        &self,
        _product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if let Some(session) = self.wallet.session_state().current() {
            self.auth_state
                .connected(&connected_session_ui_info(&session));
            Ok(HostRequestLoginResponse::AlreadyConnected)
        } else {
            // Wallet unlock and session activation are platform-owned.
            Ok(HostRequestLoginResponse::Rejected)
        }
    }

    async fn disconnect(&self) {
        self.lock_wallet();
    }

    async fn primary_username(&self) -> Option<String> {
        self.wallet
            .current_session()?
            .primary_username()
            .map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    mod activation;
    mod allowance_keys;
    mod auto_signing;
    mod cross_product_account;
    mod raw_signing;
    mod remote_consent;
    #[cfg(feature = "test-host")]
    mod withheld_resources;

    use std::sync::Arc;

    use super::super::authority::{
        AccountCaller, AccountInvocation, AuthorityError, AuthoritySession,
        CreateTransactionAuthorityRequest, SignPayloadAuthorityRequest, SignRawAuthorityRequest,
        StatementStoreAllowanceKey,
    };
    use super::super::{
        AccountHolder, HostSession, ProductRuntimeHost, RuntimeServices, SigningHostRole,
        WalletAccountHolder,
    };
    use super::LocalActivation;
    use super::TEST_NETWORK_SUFFIX;
    use super::ring_vrf::{MemberCandidate, ResolvedRing, RingResolver};
    use crate::host_internal::extrinsic::tests::split_v4;
    use crate::host_internal::sso_messages::ProductRequest;
    use crate::host_internal::sso_messages::RingVrfError;
    use crate::host_internal::transaction::{
        extrinsic_payload_extensions, extrinsic_payload_preimage,
    };
    use crate::host_logic::product_account::SR25519_SIGNING_CONTEXT;
    use crate::host_logic::product_account::{
        derive_identity_keypair, derive_product_keypair, derive_ring_vrf_entropy,
        derive_root_keypair_from_entropy, index_bytes,
    };
    use crate::platform::{HostInfo, Platform, PlatformInfo, ProductContext, SigningHostConfig};
    use crate::runtime::authority::AutoSigningGrant;
    use crate::test_support::{StubPlatform, test_spawner};
    use truapi::api::{Account, Entropy, ResourceAllocation, Signing};
    use truapi::latest::{
        HostAccountCreateProofRequest, HostAccountGetAliasRequest,
        HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
    };
    use truapi::versioned::account::{HostAccountGetError, HostAccountGetRequest};
    use truapi::versioned::entropy::HostDeriveEntropyRequest;
    use truapi::versioned::resource_allocation::{
        HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
    };
    use truapi::versioned::signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse};
    use truapi::{CallContext, CallError, v01};

    const ENTROPY: [u8; 16] = [0xAB; 16];

    #[derive(Clone)]
    struct StubRingResolver {
        validation_gate: Arc<std::sync::Mutex<Option<futures::channel::oneshot::Receiver<()>>>>,
        collection: [u8; 32],
        ring: ResolvedRing,
    }

    #[async_trait::async_trait]
    impl RingResolver for StubRingResolver {
        async fn members_pallet_index(&self, _chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
            Ok(42)
        }

        async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
            let gate = self.validation_gate.lock().unwrap().take();
            if let Some(gate) = gate {
                gate.await.unwrap();
            }
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
            crate::host_internal::product_manifest::Granted::Context,
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
        let signing_host = SigningHostRole::new(services.clone(), config.network_suffix);
        (services, signing_host)
    }

    fn product_runtime(
        services: Arc<RuntimeServices>,
        authority: Arc<SigningHostRole>,
    ) -> ProductRuntimeHost<WalletAccountHolder> {
        ProductRuntimeHost::from_services(
            services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&services),
            authority.accounts().clone(),
            authority,
            ProductContext::new("myapp.dot".to_string()).expect("valid product id"),
        )
    }

    fn product_runtime_for(
        services: Arc<RuntimeServices>,
        authority: Arc<SigningHostRole>,
        product_id: &str,
    ) -> ProductRuntimeHost<WalletAccountHolder> {
        ProductRuntimeHost::from_services(
            services.clone(),
            crate::host_core::ConnectionAdapters::from_services(&services),
            authority.accounts().clone(),
            authority,
            ProductContext::new(product_id.to_string()).expect("valid product id"),
        )
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
        let full_member = futures::executor::block_on(crate::runtime::vrf::load())
            .expect("verifiable is linked")
            .member(&full_entropy)
            .expect("full-person member");
        Arc::new(StubRingResolver {
            validation_gate: Default::default(),
            collection: *b"pop:polkadot.network/people     ",
            ring: ResolvedRing {
                selected: MemberCandidate {
                    member: full_member,
                },
                ring_index: 7,
                ring_revision: 11,
                domain_size: crate::runtime::vrf::DOMAIN_2E11,
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
            fetched_at_secs: crate::unix_time::current_unix_secs(),
            json: Some(json),
        };
        futures::executor::block_on(
            <StubPlatform as crate::platform::CoreStorage>::write_core_storage(
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
            // Bare-labelled on both sides, as `account_access_authorization`
            // writes it in production.
            crate::host_internal::permissions::set_account_access_status(
                platform,
                crate::host_internal::product_manifest::bare_product_label(caller),
                crate::host_internal::product_manifest::bare_product_label(target),
                crate::platform::PermissionAuthorizationStatus::Denied,
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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

    #[test]
    fn a_blessed_product_proves_with_a_context_grant_despite_a_stored_denial() {
        let proof =
            foreign_proof_through_the_authority_with(Some(r#"{"dim2":["context"]}"#), |platform| {
                deny_account_access(platform, "dim2.dot", "peopl.dot")
            });
        assert!(
            proof.is_ok(),
            "blessed account access ignores the saved denial: {proof:?}"
        );
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
    /// An ordinary product with no grant still takes the prompt, and a stored
    /// refusal still overrides its grant.
    #[test]
    fn a_context_grant_covers_the_identity_read() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let context = v01::ProductProofContext {
            product_id: "dim2.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let alias_for = |caller: &str| {
            futures::executor::block_on(authority.account_holder().account_alias(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new(caller.to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
                v01::HostAccountGetAliasRequest {
                    key_handle: full_person_key_handle(),
                    context: context.clone(),
                    ring_location: ring.clone(),
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
            alias_for("ordinary.dot").err(),
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
    /// here. The grant is to act in the grantee's own context or the granting
    /// product's, not in anyone else's: a context naming the owner is the grant
    /// read literally, and is the one a chain-wide proof context resolves to.
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        // `raw:` reaches `development_context_bytes`, which uses the caller's own
        // 32 bytes verbatim, so admitting it would let a grantee name any
        // context at all, including a third product's.
        let mint_raw = |caller: &str| {
            futures::executor::block_on(authority.account_holder().create_proof(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new(caller.to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
                v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: "raw:".to_string(),
                        suffix: v01::DerivationIndex::Raw([0x11; 32]),
                    },
                    ring_location: ring.clone(),
                    message: b"m".to_vec(),
                },
            ))
        };

        let mint = |caller: &str, context: &str| {
            futures::executor::block_on(authority.account_holder().create_proof(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new(caller.to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
                v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: context.to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: ring.clone(),
                    message: b"m".to_vec(),
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
            "the grant admits the grantee acting in its own context"
        );
        assert!(
            mint("dim2.dot", "peopl.dot").is_ok(),
            "the grant admits the grantee acting in the granting product's context"
        );
        assert!(
            mint("peopl.dot", "bank.dot").is_ok(),
            "the owner may still mint its own alias in any context"
        );
        assert!(
            mint("dim2.dot", "app.peopl.dot").is_ok(),
            "the granting product is all its executables, so its context is too"
        );
        assert_eq!(
            mint_raw("dim2.dot").err(),
            Some(RingVrfError::NotAllowlisted),
            "a grant must not reach the development context, which names no \
             product and so binds the grantee to nothing"
        );
        assert_eq!(
            mint("dim2.dot", "dim2.paseo").err(),
            Some(RingVrfError::NotAllowlisted),
            "the grantee's own namesake on another network is a different \
             product, so its context is not the grantee's"
        );
        assert_eq!(
            mint("dim2.dot", "peopl.paseo").err(),
            Some(RingVrfError::NotAllowlisted),
            "a grant published on one network must not reach the pseudonym a \
             namesake presents on another"
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        let listed = futures::executor::block_on(authority.account_holder().list_ring_vrf_keys(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("PEOPL.DOT".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            v01::HostAccountListRingVrfKeysRequest {
                owner: "peopl.dot".to_string(),
                disclosure: v01::RingVrfKeyDisclosure::PublicKey,
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);

        // `peopl.paseo` shares a label with the key's owner `peopl.dot` but is a
        // different product, so the grant admits it and the context still binds.
        let minted = futures::executor::block_on(authority.account_holder().create_proof(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("peopl.paseo".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            v01::HostAccountCreateProofRequest {
                key_handle: full_person_key_handle(),
                context: v01::ProductProofContext {
                    product_id: "bank.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location: ring.clone(),
                message: b"m".to_vec(),
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let mint = |context: &str| {
            futures::executor::block_on(authority.account_holder().create_proof(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new("dim2.dot".to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
                v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: context.to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: ring.clone(),
                    message: b"m".to_vec(),
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
        use crate::host_internal::product_manifest::Granted;
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
        // Written exactly as the previous release wrote it: full ids, both sides.
        futures::executor::block_on(
            crate::host_internal::permissions::set_account_access_status(
                platform.as_ref(),
                "ordinary.dot",
                "peopl.dot",
                crate::platform::PermissionAuthorizationStatus::Denied,
            ),
        )
        .expect("stub core storage accepts the decision");
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        assert!(
            !futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                platform.as_ref(),
                "ordinary.dot",
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
        use crate::host_internal::product_manifest::Granted;
        let platform = Arc::new(StubPlatform {
            // The user declines, so the refusal is written by the production
            // path rather than by a test helper: this has to pin where
            // `account_access_authorization` files it, not where a fixture does.
            account_access_confirmed: false,
            ..StubPlatform::default()
        });
        cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
        futures::executor::block_on(crate::runtime::account_access_authorization(
            platform.as_ref(),
            &Default::default(),
            "ordinary.dot",
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
                    "ordinary.dot",
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
    /// third product that granted nothing. Both calls refuse it, and both admit
    /// the granting product's own context.
    #[test]
    fn a_grantee_cannot_read_the_owners_alias_in_a_third_partys_context() {
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
        let (_services, authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let alias = |caller: &str, context: &str| {
            futures::executor::block_on(authority.account_holder().account_alias(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new(caller.to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
                v01::HostAccountGetAliasRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: context.to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: ring.clone(),
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
            "the grant covers the grantee's own context"
        );
        assert!(
            alias("dim2.dot", "peopl.dot").is_ok(),
            "the grant covers the granting product's own context"
        );
        assert!(
            alias("dim2.dot", "app.peopl.dot").is_ok(),
            "the granting product is all its executables here too"
        );
        assert_eq!(
            alias("dim2.dot", "peopl.paseo").err(),
            Some(RingVrfError::NotAllowlisted),
            "the network rule binds the read as well as the proof: the alias and \
             the proof come out of one VRF evaluation"
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring);
        let prove = |caller: &str| {
            futures::executor::block_on(authority.account_holder().create_proof(
                AccountInvocation {
                    call: &CallContext::default(),
                    session: &session,
                    caller: AccountCaller::Remote {
                        product_id: Some(caller),
                    },
                },
                v01::HostAccountCreateProofRequest {
                    key_handle: full_person_key_handle(),
                    context: v01::ProductProofContext {
                        product_id: "peopl.dot".to_string(),
                        suffix: v01::DerivationIndex::Index(0),
                    },
                    ring_location: ring.clone(),
                    message: b"m".to_vec(),
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
        use crate::host_internal::product_manifest::Granted;
        let platform = Arc::new(StubPlatform::default());
        cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
        deny_account_access(&platform, "ordinary.dot", "peopl.dot");
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
        for spelling in [
            "ordinary.dot",
            "app.ordinary.dot",
            "worker.ordinary.dot",
            "ordinary.paseo",
        ] {
            assert!(
                !granted(spelling),
                "{spelling} is the refused product wearing another name"
            );
        }

        // Control: a product the user never refused still holds its own grant,
        // so the assertions above are not passing because nothing is granted.
        let clean = Arc::new(StubPlatform::default());
        cache_grant(&clean, "peopl.dot", r#"{"ordinary":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(clean.clone(), full_person_ring_resolver());
        assert!(
            futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                clean.as_ref(),
                "app.ordinary.dot",
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

    /// An unreadable permission store refuses an ordinary product's grant.
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
        cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

        let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
            &services,
            platform.as_ref(),
            "ordinary.dot",
            "peopl.dot",
            crate::host_internal::product_manifest::Granted::Context,
        ));
        assert!(
            !granted,
            "an unreadable permission store must refuse, not fall through to the manifest"
        );

        // Control: the identical grant, with the store readable, is honoured.
        // Without this the assertion above would also pass if the grant never
        // worked at all.
        let readable = Arc::new(StubPlatform::default());
        cache_grant(&readable, "peopl.dot", r#"{"ordinary":["context"]}"#);
        let (services, _authority) =
            signing_runtime_with_ring_resolver(readable.clone(), full_person_ring_resolver());
        assert!(
            futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
                &services,
                readable.as_ref(),
                "ordinary.dot",
                "peopl.dot",
                crate::host_internal::product_manifest::Granted::Context,
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
            crate::host_internal::product_manifest::Granted::Context,
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        register_full_person_key(&authority, &session, &full_person_ring_location());

        futures::executor::block_on(authority.account_holder().ring_vrf_sign(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Remote {
                    product_id: Some(caller),
                },
            },
            v01::HostAccountRingVrfSignRequest {
                key_handle: v01::ProductAccountId {
                    dot_ns_identifier: handle_owner.to_string(),
                    derivation_index: v01::DerivationIndex::Index(0),
                },
                message: b"sign me".to_vec(),
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        register_full_person_key(&authority, &session, &full_person_ring_location());

        futures::executor::block_on(authority.account_holder().ring_vrf_sign(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Remote {
                    product_id: Some(caller),
                },
            },
            v01::HostAccountRingVrfSignRequest {
                key_handle: full_person_key_handle(),
                message: b"sign me".to_vec(),
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        futures::executor::block_on(authority.account_holder().create_proof(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("dim2.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            v01::HostAccountCreateProofRequest {
                key_handle: full_person_key_handle(),
                context: v01::ProductProofContext {
                    product_id: "dim2.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location,
                message: b"prove me".to_vec(),
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
        futures::executor::block_on(authority.account_holder().register_ring_vrf_key(
            AccountInvocation {
                call: &CallContext::default(),
                session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("peopl.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountRegisterRingVrfKeyRequest {
                index: v01::DerivationIndex::Index(0),
                ring: ring.clone(),
            },
        ))
        .expect("full person key registration succeeds");
    }

    #[test]
    fn ring_alias_and_proof_share_the_explicit_registered_key() {
        let resolver = full_person_ring_resolver();
        let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
        let authority = SigningHostRole::new_with_ring_resolver(platform, resolver);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "myapp.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let alias = futures::executor::block_on(authority.account_holder().account_alias(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("peopl.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountGetAliasRequest {
                key_handle: full_person_key_handle(),
                context: context.clone(),
                ring_location: ring_location.clone(),
            },
        ))
        .expect("alias succeeds");
        let proof = futures::executor::block_on(authority.account_holder().create_proof(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("peopl.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountCreateProofRequest {
                key_handle: full_person_key_handle(),
                context,
                ring_location,
                message: b"prove me".to_vec(),
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
        let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let registered_ring = full_person_ring_location();
        register_full_person_key(&authority, &session, &registered_ring);

        let error = futures::executor::block_on(authority.account_holder().account_alias(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("peopl.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountGetAliasRequest {
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
        ))
        .unwrap_err();

        assert_eq!(error, RingVrfError::KeyNotInRing);
    }

    #[test]
    fn direct_signing_rejects_registry_public_key_mismatched_with_wallet() {
        let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
        let authority =
            SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let handle = v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(8),
        };
        futures::executor::block_on(crate::runtime::ring_vrf_registry::RingVrfRegistryStore::new(platform.clone()).register(
            session.public_key,
            handle.clone(),
            full_person_ring_location(),
            [0xFF; 32],
        ))
        .expect("synthetic registry entry persists");

        let error = futures::executor::block_on(authority.account_holder().ring_vrf_sign(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountRingVrfSignRequest {
                key_handle: handle,
                message: b"reject mismatched registry state".to_vec(),
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let cx = CallContext::default();
        let context = v01::ProductProofContext {
            product_id: "other.dot".to_string(),
            suffix: v01::DerivationIndex::Index(0),
        };
        let ring_location = full_person_ring_location();
        register_full_person_key(&authority, &session, &ring_location);

        let alias = futures::executor::block_on(authority.account_holder().account_alias(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountGetAliasRequest {
                key_handle: full_person_key_handle(),
                context: context.clone(),
                ring_location: ring_location.clone(),
            },
        ));
        assert_eq!(alias, Err(RingVrfError::Rejected));

        let proof = futures::executor::block_on(authority.account_holder().create_proof(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            HostAccountCreateProofRequest {
                key_handle: full_person_key_handle(),
                context,
                ring_location,
                message: b"prove me".to_vec(),
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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

        futures::executor::block_on(authority.account_holder().account_alias(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new(request.calling_product_id.clone()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            request.payload.clone(),
        ))
        .expect("first alias succeeds");
        futures::executor::block_on(authority.account_holder().account_alias(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new(request.calling_product_id.clone()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            request.payload,
        ))
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

        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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

        let signature = futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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

        let authorization = authority
            .accounts()
            .wallet_authorization(
                &authority.accounts().current_operation().unwrap(),
                &ProductContext::new("myapp.dot".to_string()).unwrap(),
            )
            .unwrap();
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: authorization.as_ref(),
                    outbound_review: None,
                },
            },
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

        let error = futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("other.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
    fn product_clear_revokes_only_current_activation_grant_and_fences_stale_work() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
        let operation = authority.accounts().current_operation().unwrap();
        for product in ["myapp.dot", "other.dot"] {
            auto_signing::grant_auto_signing(&product_runtime_for(
                services.clone(),
                authority.clone(),
                product,
            ));
        }
        let authorization = authority
            .accounts()
            .wallet_authorization(
                &operation,
                &ProductContext::new("myapp.dot".to_string()).unwrap(),
            )
            .unwrap()
            .unwrap();
        futures::executor::block_on(authority.clear_product_state("myapp.dot")).unwrap();
        let current_session = authority.account_holder().current_session().unwrap();
        let own = authority.account_holder().auto_signing_status(
            &current_session,
            "myapp.dot",
            &product_account(0),
            authority
                .accounts()
                .wallet_authorization(
                    &authority.accounts().current_operation().unwrap(),
                    &ProductContext::new("myapp.dot".to_string()).unwrap(),
                )
                .unwrap()
                .as_ref(),
        );
        let other = authority.account_holder().auto_signing_status(
            &current_session,
            "other.dot",
            &v01::ProductAccountId {
                dot_ns_identifier: "other.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            authority
                .accounts()
                .wallet_authorization(
                    &authority.accounts().current_operation().unwrap(),
                    &ProductContext::new("other.dot".to_string()).unwrap(),
                )
                .unwrap()
                .as_ref(),
        );
        assert_eq!(
            (
                current_session,
                own,
                other,
                authority.grants.lifecycle().retain_wallet_authorization(
                    &authority.wallet.session_state(),
                    &operation,
                    "myapp.dot",
                    authorization,
                )
            ),
            (
                operation.session.clone(),
                Ok(AutoSigningGrant::Absent),
                Ok(AutoSigningGrant::Active),
                Err(AuthorityError::Disconnected)
            ),
        );
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
        let replacement = authority
            .account_holder()
            .current_session()
            .expect("replacement session");
        let error = futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &replacement,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
        let reactivated = authority
            .account_holder()
            .current_session()
            .expect("reactivated session");
        let error = futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &reactivated,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
        let stale = authority
            .accounts()
            .current_operation()
            .expect("first session snapshot");

        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("same wallet replacement activation succeeds");
        let current = authority
            .account_holder()
            .current_session()
            .expect("replacement session");
        assert_ne!(stale.session.validation_id, current.validation_id);

        let error = futures::executor::block_on(authority.accounts().allocate_resources(
            &CallContext::default(),
            &stale,
            &ProductContext::new("myapp.dot".to_string()).unwrap(),
            v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            },
        ))
        .expect_err("completion captured from the old activation is stale");
        assert_eq!(error, AuthorityError::Disconnected);

        let error = futures::executor::block_on(authority.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &current,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
        let granting_runtime = product_runtime(services, granting_authority.clone());
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
        let authorization = granting_authority
            .accounts()
            .wallet_authorization(
                &granting_authority.accounts().current_operation().unwrap(),
                &ProductContext::new("myapp.dot".to_string()).unwrap(),
            )
            .unwrap()
            .unwrap();
        let operation = replacement.accounts().current_operation().unwrap();
        let retained = replacement.grants.lifecycle().retain_wallet_authorization(
            &replacement.wallet.session_state(),
            &operation,
            "myapp.dot",
            authorization,
        );
        let session = replacement
            .account_holder()
            .current_session()
            .expect("replacement session");
        let error = futures::executor::block_on(replacement.account_holder().sign_vrf(
            AccountInvocation {
                call: &CallContext::default(),
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            vrf_request("myapp.dot"),
        ))
        .expect_err("a separate runtime must receive its own confirmation");
        assert_eq!(
            (
                retained,
                error,
                platform.sign_vrf_reviews.lock().unwrap().len()
            ),
            (Err(AuthorityError::Rejected), AuthorityError::Rejected, 1),
        );
    }

    #[test]
    fn sign_payload_product_and_legacy_use_the_substrate_preimage() {
        let (_services, authority) = signing_runtime_with_platform(Arc::new(StubPlatform {
            sign_payload_confirmed: true,
            ..StubPlatform::default()
        }));
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
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

        let product_response =
            futures::executor::block_on(authority.account_holder().sign_payload(
                AccountInvocation {
                    call: &cx,
                    session: &session,
                    caller: AccountCaller::Local {
                        product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                        authorization: None,
                        outbound_review: None,
                    },
                },
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

        let legacy_response = futures::executor::block_on(authority.account_holder().sign_payload(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("active session");
        let cx = CallContext::default();
        let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
        let request = |account| SignRawAuthorityRequest::IdentityAccount {
            account,
            request: v01::HostSignRawWithLegacyAccountRequest {
                signer: String::new(),
                payload: v01::RawPayload::Bytes {
                    bytes: b"hello".to_vec(),
                },
            },
        };

        let response = futures::executor::block_on(authority.account_holder().sign_raw(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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

        let error = futures::executor::block_on(authority.account_holder().sign_raw(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
            contacts: Vec::new(),
        }
    }

    #[test]
    fn create_transaction_reaches_chain_metadata_resolution() {
        let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            chain_connect_error: Some("fixture has no live chain"),
            ..StubPlatform::default()
        });
        let (_services, activation) = signing_runtime_with_platform(platform);
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation
            .account_holder()
            .current_session()
            .expect("active session");
        let cx = CallContext::default();

        let err = futures::executor::block_on(activation.account_holder().create_transaction(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            CreateTransactionAuthorityRequest::Product(tx_payload(0)),
        ))
        .expect_err("fixture cannot resolve metadata");
        assert!(
            matches!(err, AuthorityError::Unavailable { reason } if reason.contains("cannot load chain metadata")),
            "choosing the extrinsic format reads the runtime metadata"
        );
    }

    #[test]
    fn create_transaction_legacy_signer_mismatch_errors() {
        let (_services, activation) = signing_runtime_with_platform(Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            ..StubPlatform::default()
        }));
        futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        let session = activation
            .account_holder()
            .current_session()
            .expect("active session");
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
        let err = futures::executor::block_on(activation.account_holder().create_transaction(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
            request,
        ))
        .expect_err("mismatched legacy signer");
        assert!(
            matches!(err, AuthorityError::Unknown { reason } if reason.contains("does not match"))
        );
    }

    #[test]
    fn create_transaction_requires_active_session() {
        let (_services, activation) = signing_runtime();
        // A session snapshot cannot exist without activation, so construct the
        // request against a role that has never been activated.
        let (_s2, other) = signing_runtime();
        futures::executor::block_on(other.activate_local_session(ENTROPY.to_vec())).unwrap();
        let stale_session = other.account_holder().current_session().expect("session");
        futures::executor::block_on(other.disconnect());
        let cx = CallContext::default();

        let err = futures::executor::block_on(activation.account_holder().create_transaction(
            AccountInvocation {
                call: &cx,
                session: &stale_session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
    fn reactivation_invalidates_prior_session_snapshot() {
        let (_services, authority) = signing_runtime();
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("first activation");
        let stale = authority
            .account_holder()
            .current_session()
            .expect("snapshot");

        // Re-activate with different entropy: a fresh public key, hence a
        // different validation id.
        futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
            .expect("second activation");
        assert_ne!(
            authority
                .account_holder()
                .current_session()
                .expect("session")
                .public_key,
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
        let err = futures::executor::block_on(authority.account_holder().sign_raw(
            AccountInvocation {
                call: &cx,
                session: &stale,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
        let session = authority
            .account_holder()
            .current_session()
            .expect("connected");

        futures::executor::block_on(authority.disconnect());
        assert!(authority.account_holder().current_session().is_none());

        let cx = CallContext::default();
        let request = v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes { bytes: vec![1] },
        };
        let err = futures::executor::block_on(authority.account_holder().sign_raw(
            AccountInvocation {
                call: &cx,
                session: &session,
                caller: AccountCaller::Local {
                    product: &ProductContext::new("myapp.dot".to_string()).unwrap(),
                    authorization: None,
                    outbound_review: None,
                },
            },
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
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.accounts().current_operation().expect("connected");
        let cx = CallContext::default();
        let product = ProductContext::new("myapp.dot".to_string()).unwrap();

        let mut allocation = Box::pin(authority.accounts().allocate_resources(
            &cx,
            &session,
            &product,
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

    /// Each allocation spends on chain, so a withdrawn call must not start the
    /// next one.
    #[test]
    fn a_withdrawn_allocation_starts_no_further_resource() {
        let (_services, authority) =
            signing_runtime_with_platform(Arc::new(StubPlatform::default()));
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.accounts().current_operation().expect("connected");
        let cancel = truapi::CancellationToken::default();
        cancel.cancel();
        let cx = CallContext::with_parts("allocation-withdrawn".to_string(), cancel);
        let product = ProductContext::new("myapp.dot".to_string()).unwrap();

        let result = futures::executor::block_on(authority.accounts().allocate_resources(
            &cx,
            &session,
            &product,
            v01::HostRequestResourceAllocationRequest {
                resources: vec![v01::AllocatableResource::AutoSigning],
            },
        ));

        assert_eq!(
            result,
            Err(AuthorityError::Cancelled(
                crate::runtime::authority::AuthorityCancelError::new(
                    "allocation-withdrawn",
                    truapi::CancellationReason::Cancelled,
                )
            ))
        );
    }

    #[test]
    fn direct_allocation_handles_empty_and_optional_resource_batches() {
        // A PGAS request now reaches Asset Hub, so the stub has to fail the
        // connect for this to be deterministic. What the batch pins is that one
        // resource failing does not poison the others.
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("asset hub unavailable"),
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (_services, authority) = signing_runtime_with_platform(platform);
        futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
            .expect("activation");
        let session = authority.accounts().current_operation().expect("connected");
        let cx = CallContext::default();
        let product = ProductContext::new("myapp.dot".to_string()).unwrap();

        let empty = futures::executor::block_on(authority.accounts().allocate_resources(
            &cx,
            &session,
            &product,
            v01::HostRequestResourceAllocationRequest { resources: vec![] },
        ))
        .expect("empty allocation succeeds");
        assert!(empty.outcomes.is_empty());

        let optional = futures::executor::block_on(authority.accounts().allocate_resources(
            &cx,
            &session,
            &product,
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
