//! The user's root BIP-39 entropy.
//!
//! [`RootEntropy`] owns the seed a host hands over and wipes it when dropped.
//! Only this module reads the bytes, and every derivation the hosts share is a
//! method here. A derivation that returns its own module's types is a method
//! that module adds: NFT item keys in `nft/keys.rs`, the data store account in
//! `runtime/signing_host/data_store.rs`.

use parity_scale_codec::Encode;
use schnorrkel::{
	ExpansionMode, Keypair,
	derive::{ChainCode, Derivation},
};
use zeroize::Zeroizing;

use super::{
	entropy::{ProductEntropyError, derive_product_entropy_from_source},
	keyed_hash::blake2b_256_keyed,
	product_account::{
		ProductAccountError, create_chain_code, derive_hard_path_from_keypair,
		derive_ring_vrf_entropy_from_domain, index_bytes, normalize_chain_code,
		personhood_product_id,
	},
	sso::pairing::CHAT_ENCRYPTION_DOMAIN,
	x25519_seal::X25519PrivateKey,
};
use crate::platform::ProductId;

const PRODUCT_JUNCTION: &str = "product";
const RING_VRF_ROOT_KEY: &[u8] = b"ring-vrf";
const PRODUCT_ENTROPY_DOMAIN_SEPARATOR: &[u8] = b"product-entropy-derivation";

/// The user's root BIP-39 entropy, wiped from memory when dropped.
pub struct RootEntropy(Zeroizing<Vec<u8>>);

impl RootEntropy {
	/// Wraps the raw entropy a host hands over.
	///
	/// An owned `Vec` is moved in. A slice or an array is copied.
	pub fn new(entropy: impl Into<Vec<u8>>) -> Self {
		Self(Zeroizing::new(entropy.into()))
	}

	/// Derive the root sr25519 keypair.
	///
	/// Host-spec C.1 defines the BIP-39 entropy to sr25519 mini-secret path:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L24-L41>
	///
	/// Matches the Substrate mini-secret scheme (`sp_core::sr25519::Pair::from_entropy`)
	/// used by polkadot-app-ios-v2: PBKDF2 over the entropy to a 32-byte mini
	/// secret, then Ed25519-mode expansion. The public key of this keypair is the
	/// `rootAccountId` shared with paired hosts.
	pub fn derive_root_keypair(&self) -> Result<Keypair, ProductAccountError> {
		let mini_secret = substrate_bip39::mini_secret_from_entropy(&self.0, "")
			.map_err(|err| ProductAccountError::InvalidEntropy(format!("{err:?}")))?;
		Ok(mini_secret.expand_to_keypair(ExpansionMode::Ed25519))
	}

	/// Derive an sr25519 keypair down a path of hard string junctions from the
	/// canonical BIP-39 root key.
	pub fn derive_sr25519_hard_path(
		&self,
		junctions: &[&str],
	) -> Result<Keypair, ProductAccountError> {
		derive_hard_path_from_keypair(self.derive_root_keypair()?, junctions)
	}

	/// Derive the product-subtree keypair at `//product//{product_id}`.
	///
	/// The hard junctions are the security boundary: exposing this keypair grants
	/// access to one product subtree without exposing the root account.
	pub fn derive_product_subtree_keypair(
		&self,
		product_id: &ProductId,
	) -> Result<Keypair, ProductAccountError> {
		self.derive_sr25519_hard_path(&[PRODUCT_JUNCTION, product_id.as_str()])
	}

	/// Derive a product-account keypair.
	///
	/// First derives the hard product subtree, then applies the account's 32-byte
	/// derivation index as one soft junction.
	pub fn derive_product_keypair(
		&self,
		product_id: &ProductId,
		derivation_index: [u8; 32],
	) -> Result<Keypair, ProductAccountError> {
		let subtree = self.derive_product_subtree_keypair(product_id)?;
		Ok(subtree.derived_key_simple(ChainCode(derivation_index), []).0)
	}

	/// Derive the RFC-0028 identity account on the network with `network_suffix`:
	/// `//product//peopl.<network_suffix>/index_bytes(0)`, the personhood
	/// product's account 0.
	pub fn derive_identity_keypair(
		&self,
		network_suffix: &str,
	) -> Result<Keypair, ProductAccountError> {
		self.derive_product_keypair(&personhood_product_id(network_suffix), index_bytes(0))
	}

	/// Derive the RFC-0022 full-person ring-VRF entropy on the network with
	/// `network_suffix`:
	/// `hash(root_entropy, "ring-vrf")//peopl.<network_suffix>//index_bytes(0)`.
	pub fn derive_full_person_ring_vrf_entropy(&self, network_suffix: &str) -> [u8; 32] {
		self.derive_person_ring_vrf_entropy(network_suffix, 0)
	}

	/// Derive the RFC-0022 light-person ring-VRF entropy on the network with
	/// `network_suffix`:
	/// `hash(root_entropy, "ring-vrf")//peopl.<network_suffix>//index_bytes(1)`.
	pub fn derive_lite_person_ring_vrf_entropy(&self, network_suffix: &str) -> [u8; 32] {
		self.derive_person_ring_vrf_entropy(network_suffix, 1)
	}

	fn derive_person_ring_vrf_entropy(&self, network_suffix: &str, index: u32) -> [u8; 32] {
		self.derive_ring_vrf_entropy(
			&personhood_product_id(network_suffix),
			&truapi::v01::DerivationIndex::Index(index),
		)
		.expect("a product id cannot be an overflowing numeric derivation junction; qed")
	}

	/// Derive arbitrary RFC-0022 ring-VRF entropy:
	/// `hash(root_entropy, "ring-vrf")//{product_id}//{derivation_index}`.
	pub fn derive_ring_vrf_entropy(
		&self,
		product_id: &ProductId,
		derivation_index: &truapi::v01::DerivationIndex,
	) -> Result<[u8; 32], ProductAccountError> {
		let domain = self.derive_ring_vrf_domain_entropy(product_id)?;
		Ok(derive_ring_vrf_entropy_from_domain(&domain, derivation_index))
	}

	/// Derive the RFC-0024 AutoSigning entropy for a product in the ring-VRF tree.
	pub fn derive_ring_vrf_domain_entropy(
		&self,
		product_id: &ProductId,
	) -> Result<[u8; 32], ProductAccountError> {
		let root = self.keyed_hash(RING_VRF_ROOT_KEY);
		Ok(blake2b_256_keyed(&root, &create_chain_code(product_id.as_str())?))
	}

	/// Derive product-scoped entropy from the root entropy.
	pub fn derive_product_entropy(
		&self,
		product_id: &ProductId,
		key: &[u8],
	) -> Result<[u8; 32], ProductEntropyError> {
		derive_product_entropy_from_source(&self.root_entropy_source(), product_id, key)
	}

	/// Pre-hashed root entropy source (RFC-0007 layer 1). Signing hosts share this
	/// value with paired hosts during the SSO handshake so both sides derive the
	/// same product entropy.
	pub fn root_entropy_source(&self) -> [u8; 32] {
		self.keyed_hash(PRODUCT_ENTROPY_DOMAIN_SEPARATOR)
	}

	/// Derive an RFC-0022 X25519 keypair for an ECDH domain.
	///
	/// `root = blake2b256(entropy, key = "ecdh")`, then one hard keyed-hash
	/// junction `//{domain}`. X25519 clamps the resulting private bytes.
	pub fn derive_x25519_keypair(&self, domain: &[u8]) -> ([u8; 32], [u8; 32]) {
		let root = self.keyed_hash(b"ecdh");
		let chain_code = normalize_chain_code(domain.encode());
		let secret_bytes = blake2b_256_keyed(&root, &chain_code);
		(secret_bytes, X25519PrivateKey::from(secret_bytes).public_key().0)
	}

	/// Derive the X25519 private key addressing an identity in chat.
	///
	/// Every host holding the identity entropy derives the same key, which is what
	/// lets a wallet share it with a paired host that has no entropy of its own.
	pub fn derive_identity_chat_private_key(&self) -> [u8; 32] {
		self.derive_x25519_keypair(CHAT_ENCRYPTION_DOMAIN).0
	}

	fn keyed_hash(&self, key: &[u8]) -> [u8; 32] {
		blake2b_256_keyed(&self.0, key)
	}
}
