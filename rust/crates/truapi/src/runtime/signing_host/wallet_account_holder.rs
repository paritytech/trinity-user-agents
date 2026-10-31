//! Wallet activation, authorization and resource issuance.

mod account;
mod allowance;
mod allowance_renewal;
mod pairing;
#[cfg(test)]
mod tests;

pub use allowance::{AllowanceAllocationError, NotificationClaim};
pub use allowance_renewal::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use allowance_renewal::{StatementRenewalError, TrackedStatementRenewalTarget};

use std::sync::{Arc, Mutex};
use truapi::latest::ProductAccountId;
use zeroize::Zeroizing;

use super::ring_vrf::RingResolver;
use crate::{
	host_internal::sso_messages::RingVrfError,
	host_logic::{
		entropy::{derive_product_entropy, root_entropy_source},
		product_account::{
			ProductAccountError, derivation_index_bytes, derive_full_person_ring_vrf_entropy,
			derive_identity_keypair, derive_lite_person_ring_vrf_entropy, derive_product_keypair,
			derive_product_subtree_keypair, derive_ring_vrf_domain_entropy,
			derive_ring_vrf_entropy, derive_root_keypair_from_entropy, derive_sr25519_hard_path,
		},
		session::{SessionInfo, SessionState},
		sso::pairing::{
			ResponderIdentity, derive_identity_chat_private_key, derive_x25519_keypair_from_entropy,
		},
	},
	platform::ProductId,
	runtime::{
		RuntimeServices,
		authority::{
			AccountHolder, AuthorityError, AuthoritySession, authority_session_validation_id,
		},
		contacts::ContactHandles,
		product_consent::ProductConsent,
		ring_vrf_registry::RingVrfRegistryStore,
		statement_allowance::{
			PersonhoodSigner, StatementAllowanceError, collection::PersonhoodCollection, proof,
			ring::RingParams,
		},
		vrf::{self, Vrf},
	},
};

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Owns the active wallet session and its zeroizable entropy.
pub struct WalletAccountHolder {
	services: Arc<RuntimeServices>,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	renewal: allowance_renewal::RenewalState,
	/// Held through each notification-account claim, from the target's lookup
	/// to its confirmation.
	notification_claims: futures::lock::Mutex<()>,
	/// Unchecked grants and withheld resources for test-host suites. Compiled
	/// only into a build carrying `test-host`, which is off by default and which
	/// neither the production browser bundle nor a released native host
	/// enables, so a shipping host has no way to set it.
	#[cfg(feature = "test-host")]
	resource_controls: crate::runtime::test_resource_controls::TestResourceControls,
	/// The dotNS TLD of the network this wallet serves, from
	/// [`crate::platform::SigningHostConfig::network_suffix`]. Every reserved
	/// RFC-0022 derivation (`uid.<suffix>`, `peopl.<suffix>`) ends in it.
	network_suffix: String,
	consent: Arc<ProductConsent>,
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
		self.activation = self.activation.checked_add(1).expect("wallet activation exhausted");
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

/// Proves personhood with the reserved `peopl.<suffix>` keys of the wallet a
/// call began under.
///
/// Both collections' keys are always offered; which ones the person belongs to
/// is settled on chain, not by local state, so the two hosts never disagree.
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
		// The proof takes long enough that holding the wallet lock through it
		// would stall every signing call, lock and wallet switch.
		let entropy = self.wallet.with_keys::<_, AuthorityError>(self.session, |keys| {
			Ok(keys.personhood_entropy(ring.collection))
		})?;
		let proof = proof::ring_vrf_proof(
			&self.vrf,
			proof::domain_for_ring_exponent(ring.exponent)?,
			&entropy,
			&ring.members,
			context,
			message,
		)?;
		self.wallet.require_current_session(self.session)?;
		Ok(proof)
	}
}

/// Validated activation material, installed only after host grants are invalidated.
pub struct PreparedWalletActivation {
	keys: WalletKeys,
	session: SessionInfo,
}

impl WalletAccountHolder {
	/// Start locked, with no wallet secrets, asking `consent` before a product uses another
	/// product's ring-VRF keys.
	pub fn new(
		services: Arc<RuntimeServices>,
		network_suffix: String,
		consent: Arc<ProductConsent>,
		ring_resolver: Arc<dyn RingResolver>,
		ring_vrf_registry: Arc<RingVrfRegistryStore>,
	) -> Self {
		Self {
			services,
			ring_resolver,
			ring_vrf_registry,
			renewal: allowance_renewal::RenewalState::default(),
			notification_claims: Default::default(),
			#[cfg(feature = "test-host")]
			resource_controls: Default::default(),
			network_suffix,
			consent,
			lifecycle: Mutex::new(WalletState::default()),
			session_state: SessionState::new(),
		}
	}

	fn with_keys<T, E: From<AuthorityError>>(
		&self,
		session: &AuthoritySession,
		use_keys: impl FnOnce(&WalletKeys) -> Result<T, E>,
	) -> Result<T, E> {
		let state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		state.require_session(self.session_state.current(), session)?;
		use_keys(state.keys.as_ref().ok_or(AuthorityError::Disconnected)?)
	}

	async fn personhood_signer<'a>(
		&'a self,
		session: &'a AuthoritySession,
	) -> Result<WalletPersonhoodSigner<'a>, StatementAllowanceError> {
		let vrf = vrf::load().await.map_err(proof::vrf_error)?;
		self.require_current_session(session)?;
		Ok(WalletPersonhoodSigner { wallet: self, session, vrf })
	}

	/// Shared synthetic controls for the wallet and its native host.
	#[cfg(feature = "test-host")]
	pub fn resource_controls(
		&self,
	) -> &crate::runtime::test_resource_controls::TestResourceControls {
		&self.resource_controls
	}

	/// Connection-status subscriptions for the active wallet.
	pub fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	/// Validate and derive activation material without changing the active wallet.
	pub fn prepare_activation(
		&self,
		secret: Vec<u8>,
		lite_username: Option<String>,
	) -> Result<PreparedWalletActivation, AuthorityError> {
		let keys = WalletKeys::new(secret, self.network_suffix.clone());
		let public_key = keys.root_public_key()?;
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
				// A local session has no answering remote device to address.
				device_enc_public_key: None,
				lite_username,
				full_username: None,
			},
		})
	}

	/// Install under the host's grant lock so session and grant changes are atomic.
	pub fn install(&self, activation: PreparedWalletActivation) -> SessionInfo {
		let mut state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		state.advance();
		state.keys = Some(activation.keys);
		self.session_state.set_session(activation.session.clone());
		activation.session
	}

	/// Clear under the host's grant lock, dropping the active wallet secrets.
	pub fn clear(&self) {
		let mut state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		state.advance();
		state.keys.take();
		self.session_state.clear_session();
	}

	/// Check that `account` is this wallet's identity account.
	///
	/// Fails with `Unavailable` for any other account, so a paired host's legacy
	/// request is refused before the user reviews it.
	pub fn require_identity_account(
		&self,
		session: &AuthoritySession,
		account: [u8; 32],
	) -> Result<(), AuthorityError> {
		self.with_keys(session, |keys| keys.identity_keypair_for(account).map(drop))
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl WalletAccountHolder {
	/// Run `use_keys` against the wallet installed now.
	fn with_installed_keys<T>(
		&self,
		use_keys: impl FnOnce(&WalletKeys) -> Result<T, AuthorityError>,
	) -> Result<T, AuthorityError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.with_keys(&session, use_keys)
	}

	/// Derive the item keys for `slots`.
	pub fn item_keys(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemKey>, AuthorityError> {
		self.with_installed_keys(|keys| {
			crate::nft::item_keys_from_entropy(&keys.entropy, slots).map_err(AuthorityError::from)
		})
	}

	/// The signer for one item slot.
	pub fn item_signer(
		&self,
		slot: crate::nft::ItemSlot,
	) -> Result<crate::host_internal::extrinsic::Sr25519Signer, AuthorityError> {
		self.with_installed_keys(|keys| {
			crate::nft::item_signer_from_entropy(&keys.entropy, slot).map_err(AuthorityError::from)
		})
	}

	/// Derive the item secrets for `slots`.
	pub fn item_secrets(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemSecret>, AuthorityError> {
		self.with_installed_keys(|keys| {
			crate::nft::item_secrets_from_entropy(&keys.entropy, slots)
				.map_err(AuthorityError::from)
		})
	}

	/// The identity chat key.
	pub fn identity_chat_private_key(&self) -> Result<[u8; 32], AuthorityError> {
		self.with_installed_keys(|keys| Ok(derive_identity_chat_private_key(&keys.entropy)))
	}
}

struct WalletKeys {
	entropy: Zeroizing<Vec<u8>>,
	network_suffix: String,
}

impl WalletKeys {
	fn new(entropy: Vec<u8>, network_suffix: String) -> Self {
		Self { entropy: Zeroizing::new(entropy), network_suffix }
	}

	fn root_public_key(&self) -> Result<[u8; 32], ProductAccountError> {
		derive_root_keypair_from_entropy(&self.entropy).map(|root| root.public.to_bytes())
	}

	fn product_subtree_keypair(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		let root = derive_root_keypair_from_entropy(&self.entropy)?;
		Ok(derive_product_subtree_keypair(&root, product_id)?)
	}

	/// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
	/// `substrate-bip39`) rather than cached: the wallet holds only the raw,
	/// zeroizable entropy, never an expanded secret key.
	fn product_keypair(
		&self,
		account: &ProductAccountId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		let root = derive_root_keypair_from_entropy(&self.entropy)?;
		Ok(derive_product_keypair(
			&root,
			&account.dot_ns_identifier,
			derivation_index_bytes(&account.derivation_index),
		)?)
	}

	fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
		Ok(derive_identity_keypair(&self.entropy, &self.network_suffix)?)
	}

	fn identity_keypair_for(
		&self,
		account: [u8; 32],
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		let keypair = self.identity_keypair()?;
		if keypair.public.to_bytes() != account {
			return Err(AuthorityError::Unavailable {
				reason: "signing host: the requested identity account is not available in this \
				         CLI wallet"
					.to_string(),
			});
		}
		Ok(keypair)
	}

	fn ring_vrf_entropy(
		&self,
		handle: &ProductAccountId,
	) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
		derive_ring_vrf_entropy(&self.entropy, &handle.dot_ns_identifier, &handle.derivation_index)
			.map(Zeroizing::new)
			.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })
	}

	fn ring_vrf_domain_entropy(&self, product_id: &ProductId) -> Result<[u8; 32], AuthorityError> {
		Ok(derive_ring_vrf_domain_entropy(&self.entropy, product_id)?)
	}

	fn personhood_entropy(&self, collection: PersonhoodCollection) -> Zeroizing<[u8; 32]> {
		Zeroizing::new(match collection {
			PersonhoodCollection::People => {
				derive_full_person_ring_vrf_entropy(&self.entropy, &self.network_suffix)
			},
			PersonhoodCollection::LitePeople => {
				derive_lite_person_ring_vrf_entropy(&self.entropy, &self.network_suffix)
			},
		})
	}

	/// RFC-0007 product-scoped entropy.
	fn derive_entropy(
		&self,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		derive_product_entropy(&self.entropy, product_id, context)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// The same handles a pairing host derives from the root entropy source it
	/// receives, so one contact hashes alike whichever role the user is running.
	fn contact_handles(&self) -> ContactHandles {
		ContactHandles::from_root_entropy_source(&self.root_entropy_source())
	}

	/// Purpose-limited entropy shared with a paired host.
	fn root_entropy_source(&self) -> [u8; 32] {
		root_entropy_source(&self.entropy)
	}

	fn statement_allowance_key(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, ProductAccountError> {
		derive_sr25519_hard_path(
			&self.entropy,
			&["allowance", "statement-store", product_id.as_str()],
		)
	}

	fn bulletin_allowance_key(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, ProductAccountError> {
		derive_sr25519_hard_path(&self.entropy, &["allowance", "bulletin", product_id.as_str()])
	}

	/// The network's RFC-0022 `uid.<suffix>` statement identity with the SSO
	/// encryption key, and the identity chat key.
	fn responder_identity(&self) -> Result<(ResponderIdentity, [u8; 32]), ProductAccountError> {
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
