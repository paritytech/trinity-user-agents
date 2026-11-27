//! RFC-0022 account and personhood derivation shared by all hosts.
//!
//! Product subtrees use hard HDKD at `//product//{product_id}`. Individual
//! accounts use one soft junction carrying the RFC-0022 32-byte derivation
//! index, so a paired host can derive children from the subtree public key.
//! Reserved built-ins additionally pin the RFC-0028 `peopl.<suffix>` identity
//! account and the `peopl.<suffix>` full/lite ring-VRF keyed-hash paths used
//! by pairing attestation, where `<suffix>` is the network's dotNS TLD (`dot`, `paseo`,
//! `testnet`) from [`crate::platform::Stack::tld`]. The
//! People chain scopes its proof contexts with the same suffix, so one
//! network has one person per seed. RFC-0024 operational key selection comes
//! from the registry.
//! Host-spec C.5-C.7 define the product-account derivation, SS58 address, and
//! `ProductAccountId` shape:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L66-L128>

use parity_scale_codec::Encode;
use schnorrkel::{
	ExpansionMode, Keypair, PublicKey, SecretKey,
	derive::{ChainCode, Derivation},
};
use std::str::FromStr;
use thiserror::Error;

use crate::platform::{PrivilegedProduct, ProductId};

const JUNCTION_ID_LEN: usize = 32;
/// The reserved personhood product id on the network with `network_suffix`:
/// `peopl.dot` on Polkadot, `peopl.paseo` on paseo-next-v2, `peopl.testnet` on
/// previewnet. It is the product id a personhood app is opened under on that
/// network, so the keys derived here are the ones such an app owns through
/// the RFC-0024 registry. Requires the TLD of a validated stack.
pub fn personhood_product_id(network_suffix: &str) -> ProductId {
	PrivilegedProduct::Personhood.product_id(network_suffix).expect(
		"reserved personhood label and validated network suffix form a valid product id; qed",
	)
}

/// Substrate sr25519 signing-context string. Shared by every sr25519 signature
/// the core produces: statement store, product raw signing, dotNS gateway.
pub const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

/// Error deriving product accounts or keys.
#[derive(Debug, Error)]
pub enum ProductAccountError {
	/// Root public key bytes are not a valid sr25519 point.
	#[error("invalid sr25519 root public key")]
	InvalidRootPublicKey,
	/// Product subtree secret bytes are not a valid expanded sr25519 secret.
	#[error("invalid sr25519 product subtree secret")]
	InvalidProductSubtreeSecret,
	/// All-digit junction strings encode as `u64`, and this one overflows it.
	#[error("numeric derivation junction is outside u64 range")]
	NumericJunctionOutOfRange,
	/// Entropy bytes could not be expanded into a mini secret.
	#[error("invalid BIP-39 entropy: {0}")]
	InvalidEntropy(String),
}

/// 28-byte magic separating plain-index space from raw 32-byte indexes:
/// `blake2b256("product-account-index")[..28]`.
fn index_magic() -> [u8; 28] {
	let digest = sp_crypto_hashing::blake2_256(b"product-account-index");
	let mut magic = [0u8; 28];
	magic.copy_from_slice(&digest[..28]);
	magic
}

/// 32-byte derivation index for a plain `u32` index: the index little-endian
/// followed by the index magic.
pub fn index_bytes(index: u32) -> [u8; 32] {
	let mut bytes = [0u8; 32];
	bytes[..4].copy_from_slice(&index.to_le_bytes());
	bytes[4..].copy_from_slice(&index_magic());
	bytes
}

/// Internal 32-byte derivation index for a wire-level account selector.
pub fn derivation_index_bytes(index: &truapi::v01::DerivationIndex) -> [u8; 32] {
	match index {
		truapi::v01::DerivationIndex::Index(index) => index_bytes(*index),
		truapi::v01::DerivationIndex::Raw(bytes) => *bytes,
	}
}

/// Derive one registered member key from an RFC-0024 product-domain entropy.
pub fn derive_ring_vrf_entropy_from_domain(
	domain_entropy: &[u8; 32],
	derivation_index: &truapi::v01::DerivationIndex,
) -> [u8; 32] {
	blake2b256_keyed(domain_entropy, &derivation_index_bytes(derivation_index))
}

fn blake2b256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
	blake2b_simd::Params::new()
		.hash_length(32)
		.key(key)
		.hash(message)
		.as_bytes()
		.try_into()
		.expect("hash_length(32) configures BLAKE2b output to exactly 32 bytes; qed")
}

/// Soft-derive a product account from an allocated 64-byte subtree secret.
pub fn derive_product_keypair_from_subtree_secret(
	product_subtree_secret: [u8; 64],
	derivation_index: [u8; 32],
) -> Result<Keypair, ProductAccountError> {
	let secret = SecretKey::from_bytes(&product_subtree_secret)
		.map_err(|_| ProductAccountError::InvalidProductSubtreeSecret)?;
	let subtree = Keypair { public: secret.to_public(), secret };
	Ok(subtree.derived_key_simple(ChainCode(derivation_index), []).0)
}

/// Soft-derive a product account from `//product//{product_id}`'s public key.
///
/// A root public key cannot cross the two hard product junctions. Pairing hosts
/// must obtain this subtree public key from the Account Holder once per product.
pub fn derive_product_public_key(
	product_subtree_public_key: [u8; 32],
	derivation_index: [u8; 32],
) -> Result<[u8; 32], ProductAccountError> {
	let public_key = PublicKey::from_bytes(&product_subtree_public_key)
		.map_err(|_| ProductAccountError::InvalidRootPublicKey)?;
	let (derived, _) = public_key.derived_key_simple(ChainCode(derivation_index), []);
	Ok(derived.to_bytes())
}

/// Encode a product account public key as a generic Substrate SS58 address.
///
/// Delegates to subxt's `AccountId32` Display, which is the generic-substrate
/// prefix-42 SS58-check encoding host-spec C.6 mandates; the test vector
/// below pins the format against drift.
pub fn product_public_key_to_address(public_key: [u8; 32]) -> String {
	subxt::utils::AccountId32(public_key).to_string()
}

/// Decode a Substrate SS58 account address into its raw public key.
pub fn public_key_from_address(address: &str) -> Option<[u8; 32]> {
	Some(subxt::utils::AccountId32::from_str(address).ok()?.0)
}

/// Derive an sr25519 keypair down a path of hard string junctions from
/// `keypair`.
pub fn derive_hard_path_from_keypair(
	mut keypair: Keypair,
	junctions: &[&str],
) -> Result<Keypair, ProductAccountError> {
	for junction in junctions {
		let chain_code = ChainCode(create_chain_code(junction)?);
		let (mini_secret, _) = keypair.secret.hard_derive_mini_secret_key(Some(chain_code), b"");
		keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
	}
	Ok(keypair)
}

/// Create a Substrate soft-derivation chain code for one junction.
pub fn create_chain_code(code: &str) -> Result<[u8; 32], ProductAccountError> {
	let encoded = if !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_digit()) {
		code.parse::<u64>()
			.map_err(|_| ProductAccountError::NumericJunctionOutOfRange)?
			.encode()
	} else {
		code.encode()
	};
	Ok(normalize_chain_code(encoded))
}

/// Normalize a SCALE-encoded junction to a 32-byte chain code.
pub fn normalize_chain_code(encoded: Vec<u8>) -> [u8; 32] {
	let mut chain_code = [0u8; JUNCTION_ID_LEN];
	if encoded.len() > JUNCTION_ID_LEN {
		chain_code = sp_crypto_hashing::blake2_256(&encoded);
	} else {
		chain_code[..encoded.len()].copy_from_slice(&encoded);
	}
	chain_code
}

#[cfg(test)]
mod tests;
