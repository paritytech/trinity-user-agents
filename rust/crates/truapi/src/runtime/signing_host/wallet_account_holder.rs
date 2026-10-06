//! Active wallet secrets and the keys derived from them.

use std::sync::Mutex;
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
use crate::host_logic::session::SessionInfo;
use crate::host_logic::sso::pairing::{
    ResponderIdentity, derive_identity_chat_private_key, derive_x25519_keypair_from_entropy,
};
use crate::platform::normalize_product_identifier;
use crate::runtime::authority::AuthorityError;
use crate::runtime::statement_allowance::CollectionCandidate;
use crate::runtime::statement_allowance::collection::PersonhoodCollection;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
pub const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Owns wallet entropy for the active local session.
pub struct WalletAccountHolder {
    network_suffix: String,
    keys: Mutex<Option<WalletKeys>>,
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
            keys: Mutex::new(None),
        }
    }

    /// Network suffix used for reserved wallet identities.
    pub fn network_suffix(&self) -> &str {
        &self.network_suffix
    }

    /// Capture one wallet for derivations that must remain consistent across awaits.
    pub fn keys(&self) -> Result<WalletKeys, AuthorityError> {
        self.keys
            .lock()
            .expect("wallet keys mutex poisoned")
            .clone()
            .ok_or(AuthorityError::Disconnected)
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
        *self.keys.lock().expect("wallet keys mutex poisoned") = Some(activation.keys);
        activation.session
    }

    /// Clear under the host's grant lock, dropping the active wallet secrets.
    pub fn clear(&self) {
        self.keys.lock().expect("wallet keys mutex poisoned").take();
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
