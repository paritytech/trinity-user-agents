//! Wallet activation, authorization and resource issuance.

mod account;
mod allowance;
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
mod allowance_renewal;
#[cfg(test)]
mod allowance_tests;
pub use allowance::AllowanceAllocationError;
pub use allowance_renewal::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use allowance_renewal::TrackedStatementRenewalTarget;

use crate::runtime::WalletAuthorization;
use std::sync::{Arc, Mutex};
use truapi::latest::ProductAccountId;
use zeroize::Zeroizing;

use crate::host_internal::sso_messages::RingVrfError;
use crate::host_logic::entropy::{derive_product_entropy, root_entropy_source};
use crate::host_logic::product_account::{
    ProductAccountError, derivation_index_bytes, derive_full_person_ring_vrf_entropy,
    derive_identity_keypair, derive_lite_person_ring_vrf_entropy, derive_product_keypair,
    derive_product_subtree_keypair, derive_ring_vrf_domain_entropy, derive_ring_vrf_entropy,
    derive_root_keypair_from_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::host_logic::sso::pairing::{
    ResponderIdentity, derive_identity_chat_private_key, derive_x25519_keypair_from_entropy,
};
use crate::platform::normalize_product_identifier;
use crate::runtime::authority::{
    AuthorityError, AuthoritySession, AutoSigningGrant, authority_session_validation_id,
};
use crate::runtime::statement_allowance::collection::PersonhoodCollection;
use crate::runtime::statement_allowance::{PersonhoodSigner, StatementAllowanceError};
use crate::runtime::statement_allowance::{proof, ring::RingParams};
use crate::runtime::vrf::{self, Vrf};

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
pub const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Owns the active wallet session and its zeroizable entropy.
pub struct WalletAccountHolder {
    services: Arc<crate::runtime::RuntimeServices>,
    ring_resolver: Arc<dyn super::ring_vrf::RingResolver>,
    ring_vrf_registry: Arc<crate::runtime::ring_vrf_registry::RingVrfRegistryStore>,
    renewal: allowance_renewal::RenewalState,
    #[cfg(feature = "test-host")]
    resource_controls: Arc<crate::runtime::test_resource_controls::TestResourceControls>,
    network_suffix: String,
    lifecycle: Mutex<WalletState>,
    session_state: Arc<SessionState>,
}

#[derive(Default)]
struct WalletState {
    activation: u64,
    keys: Option<WalletKeys>,
    retired: bool,
}

impl WalletState {
    fn advance(&mut self) {
        self.activation = self
            .activation
            .checked_add(1)
            .expect("wallet activation exhausted");
    }

    fn session(&self, session: &SessionInfo) -> AuthoritySession {
        let mut validation_id = authority_session_validation_id(session);
        validation_id.extend_from_slice(b":activation:");
        validation_id.extend_from_slice(&self.activation.to_le_bytes());
        AuthoritySession::from_session_info(session, validation_id)
    }

    fn require_session(
        &self,
        current: Option<SessionInfo>,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        let current = current.ok_or(AuthorityError::Disconnected)?;
        if self.session(&current).validation_id != session.validation_id {
            return Err(AuthorityError::Disconnected);
        }
        Ok(current)
    }
}

struct WalletPersonhoodSigner<'a> {
    wallet: &'a WalletAccountHolder,
    session: &'a AuthoritySession,
    vrf: Vrf,
}

impl PersonhoodSigner for WalletPersonhoodSigner<'_> {
    fn member(
        &self,
        collection: PersonhoodCollection,
    ) -> Result<[u8; 32], StatementAllowanceError> {
        self.wallet.with_keys(self.session, |keys| {
            Ok(self
                .vrf
                .member(&keys.personhood_entropy(collection))
                .map_err(proof::vrf_error)?)
        })
    }

    fn alias(
        &self,
        collection: PersonhoodCollection,
        context: &[u8],
    ) -> Result<[u8; 32], StatementAllowanceError> {
        self.wallet.with_keys(self.session, |keys| {
            Ok(self
                .vrf
                .alias(&keys.personhood_entropy(collection), context)
                .map_err(proof::vrf_error)?)
        })
    }

    fn prove(
        &self,
        ring: &RingParams,
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, StatementAllowanceError> {
        self.wallet.with_keys(self.session, |keys| {
            proof::ring_vrf_proof(
                &self.vrf,
                proof::domain_for_ring_exponent(ring.exponent)?,
                &keys.personhood_entropy(ring.collection),
                &ring.members,
                context,
                message,
            )
        })
    }
}

/// Secrets exported only while preparing an encrypted pairing answer.
pub struct PairingMaterial {
    /// Identity retained by the authenticated transport.
    pub identity: ResponderIdentity,
    /// Chat identity shared with the paired host.
    pub chat_private_key: Zeroizing<[u8; 32]>,
    /// Product entropy shared with the paired host.
    pub product_entropy_source: Zeroizing<[u8; 32]>,
}

/// Validated material bound to the wallet revision before its protected read.
pub struct PreparedWalletActivation {
    expected_activation: u64,
    keys: WalletKeys,
    session: SessionInfo,
}

impl WalletAccountHolder {
    /// Start locked, with no wallet secrets.
    pub fn new(
        services: Arc<crate::runtime::RuntimeServices>,
        network_suffix: String,
        ring_vrf_registry: Arc<crate::runtime::ring_vrf_registry::RingVrfRegistryStore>,
    ) -> Self {
        Self {
            ring_resolver: super::ring_vrf::ChainRingResolver::new(services.chain.clone()),
            ring_vrf_registry,
            services,
            renewal: allowance_renewal::RenewalState::default(),
            #[cfg(feature = "test-host")]
            resource_controls: Arc::new(
                crate::runtime::test_resource_controls::TestResourceControls::default(),
            ),
            network_suffix,
            lifecycle: Mutex::new(WalletState::default()),
            session_state: SessionState::new(),
        }
    }

    /// Inject a ring resolver for account-operation tests.
    #[cfg(test)]
    pub fn new_with_ring_resolver(
        services: Arc<crate::runtime::RuntimeServices>,
        network_suffix: String,
        ring_resolver: Arc<dyn super::ring_vrf::RingResolver>,
        ring_vrf_registry: Arc<crate::runtime::ring_vrf_registry::RingVrfRegistryStore>,
    ) -> Self {
        Self {
            ring_resolver,
            ..Self::new(services, network_suffix, ring_vrf_registry)
        }
    }

    fn with_keys<T, E: From<AuthorityError>>(
        &self,
        session: &AuthoritySession,
        use_keys: impl FnOnce(&WalletKeys) -> Result<T, E>,
    ) -> Result<T, E> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.require_session(self.session_state.current(), session)?;
        use_keys(state.keys.as_ref().ok_or(AuthorityError::Disconnected)?)
    }

    async fn personhood_signer<'a>(
        &'a self,
        session: &'a AuthoritySession,
    ) -> Result<WalletPersonhoodSigner<'a>, StatementAllowanceError> {
        let vrf = vrf::load().await.map_err(proof::vrf_error)?;
        self.require_current_session(session)?;
        Ok(WalletPersonhoodSigner {
            wallet: self,
            session,
            vrf,
        })
    }

    /// Shared synthetic resource controls used by this test wallet and its host.
    #[cfg(feature = "test-host")]
    pub fn resource_controls(
        &self,
    ) -> &Arc<crate::runtime::test_resource_controls::TestResourceControls> {
        &self.resource_controls
    }

    fn authorization_matches(
        &self,
        authorization: &WalletAuthorization,
        session: &AuthoritySession,
        product_id: &str,
    ) -> bool {
        authorization
            .issuer
            .ptr_eq(&Arc::downgrade(&self.session_state))
            && authorization.validation_id == session.validation_id
            && authorization.product_id == product_id
    }

    /// Reject a receipt issued for a different wallet, activation or product.
    pub fn validate_authorization(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        authorization: &WalletAuthorization,
    ) -> Result<(), AuthorityError> {
        self.require_current_session(session)?;
        if !self.authorization_matches(authorization, session, product_id) {
            return Err(AuthorityError::Rejected);
        }
        Ok(())
    }

    /// Validate retained permission without accessing the host's grant cache.
    pub fn auto_signing_status(
        &self,
        session: &AuthoritySession,
        calling_product_id: &str,
        account: &ProductAccountId,
        authorization: Option<&WalletAuthorization>,
    ) -> Result<AutoSigningGrant, AuthorityError> {
        self.require_current_session(session)?;
        if crate::runtime::authority::is_blessed_owner(
            calling_product_id,
            &account.dot_ns_identifier,
        ) {
            return Ok(AutoSigningGrant::Active);
        }
        let (Ok(caller), Ok(owner)) = (
            normalize_product_identifier(calling_product_id),
            normalize_product_identifier(&account.dot_ns_identifier),
        ) else {
            return Ok(AutoSigningGrant::Absent);
        };
        Ok(
            if caller == owner
                && authorization
                    .is_some_and(|grant| self.authorization_matches(grant, session, &caller))
            {
                AutoSigningGrant::Active
            } else {
                AutoSigningGrant::Absent
            },
        )
    }

    /// Network suffix used for reserved wallet identities.
    pub fn network_suffix(&self) -> &str {
        &self.network_suffix
    }

    /// Connection-status subscriptions for the active wallet.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Select the current wallet activation.
    pub fn current_session(&self) -> Option<AuthoritySession> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        self.session_state
            .current()
            .map(|session| state.session(&session))
    }

    /// Require the same activation that was selected before an asynchronous operation.
    pub fn require_current_session(
        &self,
        session: &AuthoritySession,
    ) -> Result<SessionInfo, AuthorityError> {
        self.lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned")
            .require_session(self.session_state.current(), session)
    }

    /// Verify both keys of an externally owned SSO transport.
    pub fn require_sso_identity(
        &self,
        session: &AuthoritySession,
        statement_public_key: [u8; 32],
        encryption_public_key: [u8; 32],
    ) -> Result<(), AuthorityError> {
        self.with_keys(session, |keys| {
            let (identity, _) = keys.responder_identity().map_err(product_authority_error)?;
            if identity.statement_public_key != statement_public_key
                || identity.encryption_public_key != encryption_public_key
            {
                return Err(AuthorityError::Unavailable {
                    reason: "SSO transport identity does not match the active wallet".to_string(),
                });
            }
            Ok(())
        })
    }

    /// Export the selected wallet's SSO transport identity.
    pub fn responder_identity(
        &self,
        session: &AuthoritySession,
    ) -> Result<ResponderIdentity, AuthorityError> {
        self.with_keys(session, |keys| {
            Ok(keys
                .responder_identity()
                .map_err(product_authority_error)?
                .0)
        })
    }

    /// Export the selected wallet's material for an encrypted pairing answer.
    pub fn pairing_material(
        &self,
        session: &AuthoritySession,
    ) -> Result<PairingMaterial, AuthorityError> {
        self.with_keys(session, |keys| {
            let (identity, chat_private_key) =
                keys.responder_identity().map_err(product_authority_error)?;
            Ok(PairingMaterial {
                identity,
                chat_private_key: Zeroizing::new(chat_private_key),
                product_entropy_source: Zeroizing::new(keys.root_entropy_source()),
            })
        })
    }

    /// Prepare a replacement without changing the active wallet or its grants.
    pub async fn prepare_wallet_activation(
        &self,
        provider: &dyn crate::runtime::WalletSecretProvider,
        wallet_id: String,
        lite_username: Option<String>,
    ) -> Result<PreparedWalletActivation, AuthorityError> {
        let expected_activation = {
            let state = self
                .lifecycle
                .lock()
                .expect("wallet lifecycle mutex poisoned");
            if state.retired {
                return Err(AuthorityError::Disconnected);
            }
            state.activation
        };
        let entropy = Zeroizing::new(provider.read_wallet_root_entropy(wallet_id).await.map_err(
            |error| AuthorityError::Unavailable {
                reason: error.reason,
            },
        )?);
        self.prepare_activation_at(entropy, lite_username, expected_activation)
    }

    /// Prepare raw material only for test-host activation.
    #[cfg(any(test, feature = "test-host"))]
    pub fn prepare_activation(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<PreparedWalletActivation, AuthorityError> {
        let expected_activation = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned")
            .activation;
        self.prepare_activation_at(Zeroizing::new(secret), lite_username, expected_activation)
    }

    fn prepare_activation_at(
        &self,
        entropy: Zeroizing<Vec<u8>>,
        lite_username: Option<String>,
        expected_activation: u64,
    ) -> Result<PreparedWalletActivation, AuthorityError> {
        let keys = WalletKeys::new(entropy, self.network_suffix.clone());
        let public_key = keys.root_public_key().map_err(product_authority_error)?;
        let identity_account_id = keys.identity_keypair()?.public.to_bytes();
        let identity_chat_private_key = derive_identity_chat_private_key(&keys.entropy);
        Ok(PreparedWalletActivation {
            expected_activation,
            keys,
            session: SessionInfo {
                public_key,
                sso: None,
                root_entropy_source: None,
                identity_account_id: Some(identity_account_id),
                identity_chat_private_key: Some(identity_chat_private_key),
                device_enc_public_key: None,
                lite_username,
                full_username: None,
            },
        })
    }

    /// Install under the host's grant lock so session and grant changes are atomic.
    pub fn install(
        &self,
        activation: PreparedWalletActivation,
    ) -> Result<SessionInfo, AuthorityError> {
        let mut state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        if state.retired || state.activation != activation.expected_activation {
            return Err(AuthorityError::Disconnected);
        }
        state.advance();
        state.keys = Some(activation.keys);
        self.session_state.set_session(activation.session.clone());
        Ok(activation.session)
    }

    /// Clear under the host's grant lock, dropping the active wallet secrets.
    pub fn clear(&self) {
        let mut state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.advance();
        state.keys.take();
        self.session_state.clear_session();
    }

    /// Permanently refuse activation under the host's grant lifecycle lock.
    pub fn retire(&self) {
        let mut state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.retired = true;
        state.advance();
        state.keys.take();
        self.session_state.clear_session();
    }
}

/// Active wallet entropy and purpose-specific key derivation.
pub struct WalletKeys {
    entropy: Zeroizing<Vec<u8>>,
    network_suffix: String,
}

impl WalletKeys {
    /// Keep entropy zeroizable without caching expanded secret keys.
    pub fn new(entropy: Zeroizing<Vec<u8>>, network_suffix: String) -> Self {
        Self {
            entropy,
            network_suffix,
        }
    }

    /// Root public key used to bind grants and renewal records to their owner.
    pub fn root_public_key(&self) -> Result<[u8; 32], ProductAccountError> {
        derive_root_keypair_from_entropy(&self.entropy).map(|root| root.public.to_bytes())
    }

    /// Public product subtree for account discovery.
    pub fn product_subtree_public_key(&self, product_id: &str) -> Result<[u8; 32], AuthorityError> {
        let root =
            derive_root_keypair_from_entropy(&self.entropy).map_err(product_authority_error)?;
        derive_product_subtree_keypair(&root, product_id)
            .map(|keypair| keypair.public.to_bytes())
            .map_err(product_authority_error)
    }

    /// Product subtree secret exported for delegated signing.
    pub fn product_subtree_secret(&self, product_id: &str) -> Result<[u8; 64], AuthorityError> {
        let root =
            derive_root_keypair_from_entropy(&self.entropy).map_err(product_authority_error)?;
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id)
            .map(|keypair| keypair.secret.to_bytes())
            .map_err(product_authority_error)
    }

    /// Product account used for local signing.
    pub fn product_keypair(
        &self,
        account: &ProductAccountId,
    ) -> Result<schnorrkel::Keypair, AuthorityError> {
        let root =
            derive_root_keypair_from_entropy(&self.entropy).map_err(product_authority_error)?;
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
        .map_err(product_authority_error)
    }

    /// Reserved identity account for this wallet's network.
    pub fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
        derive_identity_keypair(&self.entropy, &self.network_suffix)
            .map_err(product_authority_error)
    }

    /// Entropy for a registered product ring-VRF key.
    pub fn ring_vrf_entropy(
        &self,
        handle: &ProductAccountId,
    ) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
        derive_ring_vrf_entropy(
            &self.entropy,
            &handle.dot_ns_identifier,
            &handle.derivation_index,
        )
        .map(Zeroizing::new)
        .map_err(|err| RingVrfError::Unknown {
            reason: err.to_string(),
        })
    }

    /// Product domain entropy exported alongside an AutoSigning subtree.
    pub fn ring_vrf_domain_entropy(
        &self,
        product_id: &str,
    ) -> Result<[u8; 32], ProductAccountError> {
        derive_ring_vrf_domain_entropy(&self.entropy, product_id)
    }

    fn personhood_entropy(&self, collection: PersonhoodCollection) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(match collection {
            PersonhoodCollection::People => {
                derive_full_person_ring_vrf_entropy(&self.entropy, &self.network_suffix)
            }
            PersonhoodCollection::LitePeople => {
                derive_lite_person_ring_vrf_entropy(&self.entropy, &self.network_suffix)
            }
        })
    }

    /// RFC-0007 product-scoped entropy.
    pub fn derive_entropy(
        &self,
        product_id: &str,
        context: &[u8],
    ) -> Result<[u8; 32], AuthorityError> {
        derive_product_entropy(&self.entropy, product_id, context).map_err(|err| {
            AuthorityError::Unknown {
                reason: err.to_string(),
            }
        })
    }

    /// Product-independent secret for contact handles.
    pub fn contacts_handle_key(&self) -> [u8; 32] {
        crate::runtime::contacts::handle_key_from_root_source(&self.root_entropy_source())
    }

    /// Purpose-limited entropy shared with a paired host.
    pub fn root_entropy_source(&self) -> [u8; 32] {
        root_entropy_source(&self.entropy)
    }

    /// Statement-store allowance account for a product.
    pub fn statement_allowance_key(
        &self,
        product_id: &str,
    ) -> Result<schnorrkel::Keypair, ProductAccountError> {
        derive_sr25519_hard_path(&self.entropy, &["allowance", "statement-store", product_id])
    }

    /// Bulletin allowance account for a product.
    pub fn bulletin_allowance_key(
        &self,
        product_id: &str,
    ) -> Result<schnorrkel::Keypair, ProductAccountError> {
        derive_sr25519_hard_path(&self.entropy, &["allowance", "bulletin", product_id])
    }

    /// Statement, encryption and chat keys from one wallet snapshot.
    pub fn responder_identity(&self) -> Result<(ResponderIdentity, [u8; 32]), ProductAccountError> {
        let statement = derive_identity_keypair(&self.entropy, &self.network_suffix)?;
        let (encryption_secret_key, encryption_public_key) =
            derive_x25519_keypair_from_entropy(&self.entropy, SSO_ENCRYPTION_DOMAIN);
        let identity_chat_private_key = derive_identity_chat_private_key(&self.entropy);
        Ok((
            ResponderIdentity {
                statement_secret: statement.secret.to_bytes(),
                statement_public_key: statement.public.to_bytes(),
                encryption_secret_key,
                encryption_public_key,
            },
            identity_chat_private_key,
        ))
    }
}

/// Map unavailable wallet derivations to the account-operation error contract.
pub fn product_authority_error(err: ProductAccountError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: err.to_string(),
    }
}
