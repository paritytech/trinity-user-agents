//! Wallet activation, session validation and derived keys.

use std::sync::{Arc, Mutex};
use truapi::latest::{HostAccountSignVrfRequest, ProductAccountId, VrfSignature};
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
    AuthorityError, AuthoritySession, authority_session_validation_id,
};
use crate::runtime::statement_allowance::CollectionCandidate;
use crate::runtime::statement_allowance::collection::PersonhoodCollection;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
pub const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Owns the active wallet session and its zeroizable entropy.
pub struct WalletAccountHolder {
    network_suffix: String,
    lifecycle: Mutex<WalletState>,
    session_state: Arc<SessionState>,
}

#[derive(Default)]
struct WalletState {
    activation: u64,
    keys: Option<WalletKeys>,
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

/// Validated activation material, installed only after host grants are invalidated.
pub struct PreparedWalletActivation {
    keys: WalletKeys,
    session: SessionInfo,
}

impl WalletAccountHolder {
    /// Start locked, with no wallet secrets.
    pub fn new(network_suffix: String) -> Self {
        Self {
            network_suffix,
            lifecycle: Mutex::new(WalletState::default()),
            session_state: SessionState::new(),
        }
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

    /// Capture one wallet for grouped derivations across asynchronous work.
    pub fn current_keys(&self) -> Result<(AuthoritySession, WalletKeys), AuthorityError> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        let session = self
            .session_state
            .current()
            .ok_or(AuthorityError::Disconnected)?;
        let keys = state.keys.clone().ok_or(AuthorityError::Disconnected)?;
        Ok((state.session(&session), keys))
    }

    /// Capture keys only for the selected wallet activation.
    pub fn keys(&self, session: &AuthoritySession) -> Result<WalletKeys, AuthorityError> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.require_session(self.session_state.current(), session)?;
        state.keys.clone().ok_or(AuthorityError::Disconnected)
    }

    /// Sign only while the approved wallet activation remains installed.
    pub fn sign_vrf(
        &self,
        session: &AuthoritySession,
        request: &HostAccountSignVrfRequest,
    ) -> Result<VrfSignature, AuthorityError> {
        let state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.require_session(self.session_state.current(), session)?;
        let keypair = state
            .keys
            .as_ref()
            .ok_or(AuthorityError::Disconnected)?
            .product_keypair(&request.account)?;
        let (pre_output, proof) = crate::dynamic_vrf::sign_dynamic_vrf(
            &keypair,
            &request.transcript_label,
            request
                .items
                .iter()
                .map(|item| (item.label.as_slice(), item.value.as_slice())),
        );
        Ok(VrfSignature { pre_output, proof })
    }

    /// Validate and derive activation material without changing the active wallet.
    pub fn prepare_activation(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<PreparedWalletActivation, AuthorityError> {
        let keys = WalletKeys::new(secret, self.network_suffix.clone());
        let public_key = keys.root_public_key().map_err(product_authority_error)?;
        let identity_account_id = keys.identity_keypair()?.public.to_bytes();
        let identity_chat_private_key = derive_identity_chat_private_key(&keys.entropy);
        Ok(PreparedWalletActivation {
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
    pub fn install(&self, activation: PreparedWalletActivation) -> SessionInfo {
        let mut state = self
            .lifecycle
            .lock()
            .expect("wallet lifecycle mutex poisoned");
        state.advance();
        state.keys = Some(activation.keys);
        self.session_state.set_session(activation.session.clone());
        activation.session
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
}

/// Opaque wallet snapshot; derived keys share one root even if activation changes.
#[derive(Clone)]
pub struct WalletKeys {
    entropy: Zeroizing<Vec<u8>>,
    network_suffix: String,
}

impl WalletKeys {
    /// Keep entropy zeroizable without caching expanded secret keys.
    pub fn new(entropy: Vec<u8>, network_suffix: String) -> Self {
        Self {
            entropy: Zeroizing::new(entropy),
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

    /// Validate the owner before recording a product's AutoSigning grant.
    pub fn validate_product_owner(
        &self,
        owner: [u8; 32],
        product_id: &str,
    ) -> Result<String, AuthorityError> {
        let root =
            derive_root_keypair_from_entropy(&self.entropy).map_err(product_authority_error)?;
        if root.public.to_bytes() != owner {
            return Err(AuthorityError::Disconnected);
        }
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        derive_product_subtree_keypair(&root, &product_id).map_err(product_authority_error)?;
        Ok(product_id)
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

    /// Reserved personhood candidates, widest slot budget first; membership is checked on chain.
    pub fn reserved_person_collection_candidates(&self) -> Vec<CollectionCandidate> {
        vec![
            CollectionCandidate {
                collection: PersonhoodCollection::People,
                entropy: derive_full_person_ring_vrf_entropy(&self.entropy, &self.network_suffix),
            },
            CollectionCandidate {
                collection: PersonhoodCollection::LitePeople,
                entropy: derive_lite_person_ring_vrf_entropy(&self.entropy, &self.network_suffix),
            },
        ]
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
