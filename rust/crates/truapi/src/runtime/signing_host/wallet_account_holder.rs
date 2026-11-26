//! Wallet activation, authorization and resource issuance.

mod account;
mod allowance;
mod allowance_renewal;
#[cfg(not(target_arch = "wasm32"))]
mod data_store;
#[cfg(not(target_arch = "wasm32"))]
mod item_key_range;
mod pairing;
#[cfg(test)]
mod tests;

pub use allowance_renewal::StatementRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
pub use allowance_renewal::{StatementRenewalError, TrackedStatementRenewalTarget};

use std::sync::{Arc, Mutex};
use truapi::latest::ProductAccountId;
use zeroize::Zeroizing;

use super::ring_vrf::RingResolver;
use crate::{
	host_internal::{extrinsic::Sr25519Signer, sso_messages::RingVrfError},
	host_logic::{
		device_key::{read_or_create_device_encryption_secret, store_device_encryption_secret},
		product_account::{ProductAccountError, derivation_index_bytes},
		root_entropy::RootEntropy,
		session::{SessionInfo, SessionState},
		sso::pairing::ResponderIdentity,
	},
	platform::ProductId,
	runtime::{
		RuntimeServices,
		authority::{
			AccountHolder, AuthorityError, AuthoritySession, authority_session_validation_id,
		},
		contacts::ContactHandleKey,
		product_consent::ProductConsent,
		ring_vrf_registry::RingVrfRegistryStore,
		served::{NamesProducts, Served, serve},
		statement_allowance::{
			PersonhoodSigner, StatementAllowanceError, collection::PersonhoodCollection, proof,
			ring::RingParams,
		},
		vrf::{self, Vrf},
	},
	stack::Stack,
};

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Owns the active wallet session and its zeroizable entropy.
pub struct WalletAccountHolder {
	services: Arc<RuntimeServices>,
	ring_resolver: Arc<dyn RingResolver>,
	ring_vrf_registry: Arc<RingVrfRegistryStore>,
	renewal: allowance_renewal::RenewalState,
	/// Unchecked grants and withheld resources for test-host suites. Compiled
	/// only into a build carrying `test-host`, which is off by default and which
	/// neither the production browser bundle nor a released native host
	/// enables, so a shipping host has no way to set it.
	#[cfg(feature = "test-host")]
	resource_controls: Arc<crate::runtime::test_resource_controls::TestResourceControls>,
	consent: Arc<ProductConsent>,
	lifecycle: Mutex<WalletState>,
	session_state: Arc<SessionState>,
	/// This install's encryption secret once read or adopted. The lock also
	/// serializes the first read-or-create: concurrent first-time readers would
	/// otherwise each generate a secret and persist it, leaving peers
	/// addressing an overwritten key.
	device_encryption_secret: futures::lock::Mutex<Option<[u8; 32]>>,
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
		consent: Arc<ProductConsent>,
		ring_resolver: Arc<dyn RingResolver>,
		ring_vrf_registry: Arc<RingVrfRegistryStore>,
	) -> Self {
		Self {
			services,
			ring_resolver,
			ring_vrf_registry,
			renewal: allowance_renewal::RenewalState::default(),
			#[cfg(feature = "test-host")]
			resource_controls: Default::default(),
			consent,
			lifecycle: Mutex::new(WalletState::default()),
			session_state: SessionState::new(),
			device_encryption_secret: futures::lock::Mutex::new(None),
		}
	}

	/// `value` as [`Served`], naming each product by the id the stack serves
	/// it under. For a paired host's request, which does not pass through a
	/// product connection. The peer may be a host that is not built on this
	/// core and names a product under `dot`, as a product does.
	pub fn serve<T: NamesProducts>(&self, value: T) -> Result<Served<T>, AuthorityError> {
		Ok(serve(self.services.stack(), value)?)
	}

	/// This install's X25519 encryption secret, which contacts encrypt to and
	/// pairing answers advertise.
	///
	/// Created and persisted on first read unless one was adopted.
	pub async fn encryption_secret(&self) -> Result<[u8; 32], String> {
		let mut cached = self.device_encryption_secret.lock().await;
		if let Some(secret) = *cached {
			return Ok(secret);
		}
		let secret =
			read_or_create_device_encryption_secret(self.services.platform.as_ref()).await?;
		*cached = Some(secret);
		Ok(secret)
	}

	/// Make `secret` this install's X25519 encryption secret, replacing the
	/// stored one.
	///
	/// For a host that announced a key of its own before the core kept one,
	/// so peers addressing that key stay reachable.
	pub async fn adopt_encryption_secret(&self, secret: [u8; 32]) -> Result<(), String> {
		let mut cached = self.device_encryption_secret.lock().await;
		store_device_encryption_secret(self.services.platform.as_ref(), secret).await?;
		*cached = Some(secret);
		Ok(())
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
	) -> &Arc<crate::runtime::test_resource_controls::TestResourceControls> {
		&self.resource_controls
	}

	/// Connection-status subscriptions for the active wallet.
	pub fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	/// Validate and derive activation material without changing the active wallet.
	pub fn prepare_activation(
		&self,
		entropy: RootEntropy,
		lite_username: Option<String>,
	) -> Result<PreparedWalletActivation, AuthorityError> {
		let keys = WalletKeys::new(entropy, self.services.stack().clone());
		let public_key = keys.root_public_key()?;
		let identity_account_id = keys.identity_keypair()?.public.to_bytes();
		let identity_chat_private_key = keys.entropy.derive_identity_chat_private_key();
		Ok(PreparedWalletActivation {
			keys,
			session: SessionInfo {
				public_key,
				sso: None,
				root_entropy_source: None,
				identity_account_id: Some(identity_account_id),
				identity_chat_private_key: Some(identity_chat_private_key),
				// A local session has no answering remote device to address.
				peer_device_enc_public_key: None,
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

	/// A signer for the identity account of the active session.
	///
	/// Fails with `Disconnected` without an active session.
	pub fn identity_signer(&self) -> Result<Sr25519Signer, AuthorityError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.with_keys(&session, |keys| Ok(Sr25519Signer::from_keypair(&keys.identity_keypair()?)))
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
		self.with_installed_keys(|keys| keys.entropy.item_keys(slots).map_err(AuthorityError::from))
	}

	/// The signer for one item slot.
	pub fn item_signer(&self, slot: crate::nft::ItemSlot) -> Result<Sr25519Signer, AuthorityError> {
		self.with_installed_keys(|keys| {
			keys.entropy.item_signer(slot).map_err(AuthorityError::from)
		})
	}

	/// Derive the item secrets for `slots`.
	pub fn item_secrets(
		&self,
		slots: &[crate::nft::ItemSlot],
	) -> Result<Vec<crate::nft::ItemSecret>, AuthorityError> {
		self.with_installed_keys(|keys| {
			keys.entropy.item_secrets(slots).map_err(AuthorityError::from)
		})
	}

	/// The identity chat key.
	pub fn identity_chat_private_key(&self) -> Result<[u8; 32], AuthorityError> {
		self.with_installed_keys(|keys| Ok(keys.entropy.derive_identity_chat_private_key()))
	}
}

struct WalletKeys {
	entropy: RootEntropy,
	stack: Stack,
}

impl WalletKeys {
	fn new(entropy: RootEntropy, stack: Stack) -> Self {
		Self { entropy, stack }
	}

	fn root_public_key(&self) -> Result<[u8; 32], ProductAccountError> {
		self.entropy.derive_root_keypair().map(|root| root.public.to_bytes())
	}

	/// Refuses an id the stack serves no product under, a `dot` name on a
	/// stack with another TLD included: the stack serves that product under
	/// its own TLD, and no key may derive under the Polkadot id. Every key
	/// derivation keyed by a product id calls it first, so a paired host's
	/// request and a stored renewal target, which reaches the wallet without
	/// a request, are checked as well. The contact handle key does not, since
	/// only a connection's own product reaches it.
	fn require_served(&self, product_id: &ProductId) -> Result<(), AuthorityError> {
		Ok(self.stack.require_served(product_id)?)
	}

	fn product_subtree_keypair(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.require_served(product_id)?;
		Ok(self.entropy.derive_product_subtree_keypair(product_id)?)
	}

	/// The root keypair is recomputed per call (PBKDF2, 2048 rounds, via
	/// `substrate-bip39`) rather than cached: the wallet holds only the raw,
	/// zeroizable entropy, never an expanded secret key.
	fn product_keypair(
		&self,
		account: &ProductAccountId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.require_served(&account.dot_ns_identifier)?;
		Ok(self.entropy.derive_product_keypair(
			&account.dot_ns_identifier,
			derivation_index_bytes(&account.derivation_index),
		)?)
	}

	fn identity_keypair(&self) -> Result<schnorrkel::Keypair, AuthorityError> {
		Ok(self.entropy.derive_identity_keypair(self.stack.tld())?)
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
		self.require_served(&handle.dot_ns_identifier)?;
		self.entropy
			.derive_ring_vrf_entropy(&handle.dot_ns_identifier, &handle.derivation_index)
			.map(Zeroizing::new)
			.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })
	}

	fn ring_vrf_domain_entropy(&self, product_id: &ProductId) -> Result<[u8; 32], AuthorityError> {
		self.require_served(product_id)?;
		Ok(self.entropy.derive_ring_vrf_domain_entropy(product_id)?)
	}

	fn personhood_entropy(&self, collection: PersonhoodCollection) -> Zeroizing<[u8; 32]> {
		Zeroizing::new(match collection {
			PersonhoodCollection::People => {
				self.entropy.derive_full_person_ring_vrf_entropy(self.stack.tld())
			},
			PersonhoodCollection::LitePeople => {
				self.entropy.derive_lite_person_ring_vrf_entropy(self.stack.tld())
			},
		})
	}

	/// RFC-0007 product-scoped entropy.
	fn derive_entropy(
		&self,
		product_id: &ProductId,
		context: &[u8],
	) -> Result<[u8; 32], AuthorityError> {
		self.require_served(product_id)?;
		self.entropy
			.derive_product_entropy(product_id, context)
			.map_err(|err| AuthorityError::Unknown { reason: err.to_string() })
	}

	/// The same key a pairing host derives from the root entropy source it
	/// receives, so one contact hashes alike whichever role the user is running.
	fn contact_handle_key(&self, product_id: &ProductId) -> ContactHandleKey {
		ContactHandleKey::from_root_entropy_source(&self.root_entropy_source(), product_id)
	}

	/// Purpose-limited entropy shared with a paired host.
	fn root_entropy_source(&self) -> [u8; 32] {
		self.entropy.root_entropy_source()
	}

	fn statement_allowance_key(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.require_served(product_id)?;
		Ok(self.entropy.derive_sr25519_hard_path(&[
			"allowance",
			"statement-store",
			product_id.as_str(),
		])?)
	}

	fn bulletin_allowance_key(
		&self,
		product_id: &ProductId,
	) -> Result<schnorrkel::Keypair, AuthorityError> {
		self.require_served(product_id)?;
		Ok(self.entropy.derive_sr25519_hard_path(&[
			"allowance",
			"bulletin",
			product_id.as_str(),
		])?)
	}

	/// The network's RFC-0028 `peopl.<suffix>` statement identity with the SSO
	/// encryption key, and the identity chat key.
	fn responder_identity(&self) -> Result<(ResponderIdentity, [u8; 32]), ProductAccountError> {
		let statement = self.entropy.derive_identity_keypair(self.stack.tld())?;
		let (encryption_secret_key, encryption_public_key) =
			self.entropy.derive_x25519_keypair(SSO_ENCRYPTION_DOMAIN);
		let identity_chat_private_key = self.entropy.derive_identity_chat_private_key();
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
