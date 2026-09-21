// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use parity_scale_codec::Encode;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// The current iOS main purse; persisted indices must belong to this layout.
pub const MAIN_PURSE: u32 = u32::MAX;
/// Coinage currently derives all main-purse keys on page zero.
pub const PAGE: u32 = 0;

pub const RECYCLER_ALIAS_CONTEXT: &[u8; 32] = b"pop:polkadot.network/coinrecyclr";

/// Derives sr25519 coins at `//coinage//4294967295//0/<index>`: three hard
/// parent junctions and a soft item junction. The cached parent is zeroized on drop.
/// Callers must not reuse indices persisted under a different purse layout.
pub struct CoinKeypairFactory {
    parent: Result<schnorrkel::Keypair, String>,
}

impl Zeroize for CoinKeypairFactory {
    fn zeroize(&mut self) {
        // Dropping the cached Schnorrkel keypair zeroizes its secret. Leave no
        // usable parent after explicit clearing, and do not allocate on drop.
        self.parent = Err(String::new());
    }
}

impl Drop for CoinKeypairFactory {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for CoinKeypairFactory {}

impl CoinKeypairFactory {
    pub fn new(entropy: &[u8]) -> Self {
        let parent = derive_sr25519_hard_path(
            entropy,
            &[
                junction_chain_code("coinage"),
                junction_chain_code(u64::from(MAIN_PURSE)),
                junction_chain_code(u64::from(PAGE)),
            ],
        )
        .map_err(|error| format!("coin key derivation: {error}"));
        Self { parent }
    }

    /// The full keypair for one coin index.
    pub fn keypair(&self, index: u32) -> Result<schnorrkel::Keypair, String> {
        use rand::SeedableRng;
        use schnorrkel::derive::{ChainCode, Derivation};

        // Recovery derives thousands of children; expand BIP-39 and the three
        // hard parents once per factory, not once per scanned index.
        let parent = self.parent.as_ref().map_err(Clone::clone)?;
        // Only the HDKD auxiliary randomness is fixed. Schnorrkel mixes the
        // parent secret and nonce into its witness RNG; the scalar/public key
        // retain standard soft derivation. This makes the complete exported
        // 64-byte secret stable so a durable handoff replays the same memo.
        // Signing continues to use Schnorrkel's ordinary randomized path.
        Ok(parent
            .derived_key_simple_rng(
                ChainCode(junction_chain_code(u64::from(index))),
                [],
                rand_chacha::ChaCha20Rng::from_seed([0; 32]),
            )
            .0)
    }

    /// The coin's on-chain identity (`CoinsByOwner` storage key part).
    pub fn public_key(&self, index: u32) -> Result<[u8; 32], String> {
        Ok(self.keypair(index)?.public.to_bytes())
    }

    /// The raw 64-byte expanded secret — the exact layout
    /// `schnorrkel::SecretKey::from_bytes` reconstructs a signer from.
    pub fn secret_bytes(&self, index: u32) -> Result<[u8; 64], String> {
        Ok(self.keypair(index)?.secret.to_bytes())
    }
}

/// A derived voucher seed (32 bytes). Secret material: the Bandersnatch
/// secret key is expanded from it. Zeroized on drop.
#[derive(Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct VoucherSeed(pub [u8; 32]);

impl std::fmt::Debug for VoucherSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print seed bytes.
        f.write_str("VoucherSeed(..)")
    }
}

/// Derives vouchers at `//coinage-ring-vrf//4294967295//0//<index>`.
/// All four junctions are hard and fold directly over root entropy, without
/// BIP-39 expansion. Callers must keep persisted indices scoped to this layout.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct VoucherKeypairFactory {
    entropy: Vec<u8>,
}

impl VoucherKeypairFactory {
    pub fn new(entropy: &[u8]) -> Self {
        Self {
            entropy: entropy.to_vec(),
        }
    }

    /// The chained seed for one voucher index.
    pub fn seed(&self, index: u32) -> VoucherSeed {
        let mut seed = VoucherSeed(keyed_blake2b_256(
            &self.entropy,
            &junction_chain_code("coinage-ring-vrf"),
        ));
        for junction in [MAIN_PURSE, PAGE, index] {
            let chain_code = junction_chain_code(u64::from(junction));
            seed.0 = keyed_blake2b_256(&seed.0, &chain_code);
        }
        seed
    }

    /// The member key used by Coinage and Members storage.
    pub fn public_key(
        &self,
        index: u32,
        crypto: &dyn VoucherCryptography,
    ) -> Result<[u8; 32], String> {
        crypto.member_key(&self.seed(index))
    }

    /// Ownership signature binding the member key to an input coin.
    pub fn proof_of_ownership(
        &self,
        index: u32,
        message: &[u8],
        crypto: &dyn VoucherCryptography,
    ) -> Result<[u8; 64], String> {
        crypto.sign(&self.seed(index), message)
    }

    /// The context-specific recycler alias, independent of the implication.
    pub fn alias(
        &self,
        index: u32,
        context: &[u8],
        crypto: &dyn VoucherCryptography,
    ) -> Result<[u8; 32], String> {
        crypto.alias(&self.seed(index), context)
    }

    /// Transaction-bound Bandersnatch membership proof at a finalized ring.
    pub fn ring_vrf_proof(
        &self,
        index: u32,
        ring_exponent: u8,
        ring_members: &[[u8; 32]],
        context: &[u8],
        message: &[u8],
        crypto: &dyn VoucherCryptography,
    ) -> Result<Vec<u8>, String> {
        crypto.ring_vrf_proof(
            &self.seed(index),
            ring_exponent,
            ring_members,
            context,
            message,
        )
    }
}

/// SCALE-encoded Substrate junction → 32-byte chain code. Numeric path
/// components must be passed as `u64`, even though purse/page/item are `u32`.
/// Oversized encodings hash through BLAKE2b-256; short ones zero-pad.
fn junction_chain_code(junction: impl Encode) -> [u8; 32] {
    let mut chain_code = [0u8; 32];
    if junction.encoded_size() > chain_code.len() {
        chain_code = blake2b_256(&junction.encode());
    } else {
        junction.encode_to(&mut &mut chain_code[..]);
    }
    chain_code
}

fn keyed_blake2b_256(message: &[u8], key: &[u8]) -> [u8; 32] {
    let mut params = blake2b_simd::Params::new();
    params.hash_length(32).key(key);
    params
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

fn blake2b_256(message: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinDerivedWalletError {
    /// The requesting signer's account id is not this coin's public key.
    UnexpectedAccount,
}

impl std::fmt::Display for CoinDerivedWalletError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "signer account does not match the coin key")
    }
}

impl std::error::Error for CoinDerivedWalletError {}

pub struct CoinDerivedWallet {
    keypair: schnorrkel::Keypair,
}

impl CoinDerivedWallet {
    pub fn new(keypair: schnorrkel::Keypair) -> Self {
        Self { keypair }
    }

    /// The wallet for one coin index.
    pub fn for_index(factory: &CoinKeypairFactory, index: u32) -> Result<Self, String> {
        Ok(Self::new(factory.keypair(index)?))
    }

    /// The coin's raw public key (== its sr25519 account id).
    pub fn public_key(&self) -> [u8; 32] {
        self.keypair.public.to_bytes()
    }

    pub fn fetch_signer_secret(
        &self,
        signer_account_id: &[u8; 32],
    ) -> Result<[u8; 64], CoinDerivedWalletError> {
        if *signer_account_id != self.public_key() {
            return Err(CoinDerivedWalletError::UnexpectedAccount);
        }
        Ok(self.keypair.secret.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTROPY: [u8; 16] = [7u8; 16];

    #[test]
    fn coin_keys_are_distinct_per_index_and_deterministic() {
        let factory = CoinKeypairFactory::new(&ENTROPY);
        let a0 = factory.public_key(0).unwrap();
        let a1 = factory.public_key(1).unwrap();
        assert_ne!(a0, a1, "indices must never collide");
        assert_eq!(a0, CoinKeypairFactory::new(&ENTROPY).public_key(0).unwrap());
    }

    #[test]
    fn clearing_cached_coin_parent_prevents_further_derivation() {
        let mut factory = CoinKeypairFactory::new(&ENTROPY);
        factory.public_key(0).unwrap();
        factory.zeroize();
        assert!(factory.public_key(0).is_err());
        assert!(factory.secret_bytes(1).is_err());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn coin_keys_match_canonical_main_purse_with_soft_items() {
        use subxt_signer::{SecretUri, bip39::Mnemonic, sr25519};

        let mnemonic = Mnemonic::from_entropy(&ENTROPY).unwrap();
        for index in [0, u32::MAX] {
            let soft_uri: SecretUri = format!("{mnemonic}//coinage//4294967295//0/{index}")
                .parse()
                .unwrap();
            let hard_uri: SecretUri = format!("{mnemonic}//coinage//4294967295//0//{index}")
                .parse()
                .unwrap();
            let actual = CoinKeypairFactory::new(&ENTROPY).keypair(index).unwrap();
            let expected = sr25519::Keypair::from_uri(&soft_uri).unwrap();
            assert_eq!(actual.public.to_bytes(), expected.public_key().0);
            assert_ne!(
                actual.public.to_bytes(),
                sr25519::Keypair::from_uri(&hard_uri)
                    .unwrap()
                    .public_key()
                    .0,
                "coin items must be soft, unlike voucher items"
            );
            let message = b"coin main-purse signing regression";
            let signature = actual.sign_simple(b"substrate", message);
            assert!(sr25519::verify(
                &sr25519::Signature(signature.to_bytes()),
                message,
                &expected.public_key(),
            ));
        }
    }

    #[test]
    fn coin_secret_bytes_are_64_and_rebuild_the_keypair() {
        let factory = CoinKeypairFactory::new(&ENTROPY);
        let secret = factory.secret_bytes(3).unwrap();
        let rebuilt = schnorrkel::SecretKey::from_bytes(&secret).unwrap();
        assert_eq!(
            rebuilt.to_public().to_bytes(),
            factory.public_key(3).unwrap(),
            "memo secret bytes must reconstruct the coin's public identity"
        );
    }

    #[test]
    fn voucher_seeds_are_deterministic_and_distinct() {
        let factory = VoucherKeypairFactory::new(&ENTROPY);
        assert_eq!(
            factory.seed(0),
            VoucherKeypairFactory::new(&ENTROPY).seed(0)
        );
        assert_ne!(factory.seed(0), factory.seed(1));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn voucher_entropy_matches_canonical_full_path_fold() {
        use blake2::{
            Blake2bMac,
            digest::{KeyInit, Mac, consts::U32},
        };
        use subxt_signer::{DeriveJunction, SecretUri};

        // Independent URI parser and BLAKE2 implementation: no production
        // chain-code helper, path constants, or keyed-hash helper is reused.
        let uri: SecretUri = "//coinage-ring-vrf//4294967295//0//5".parse().unwrap();
        let mut expected = ENTROPY.to_vec();
        for junction in uri.junctions {
            let DeriveJunction::Hard(chain_code) = junction else {
                panic!("voucher reference path must be entirely hard");
            };
            let mut hash = <Blake2bMac<U32> as KeyInit>::new_from_slice(&chain_code).unwrap();
            Mac::update(&mut hash, &expected);
            expected = hash.finalize().into_bytes().to_vec();
        }
        assert_eq!(
            VoucherKeypairFactory::new(&ENTROPY).seed(5).0.as_slice(),
            expected
        );
    }

    /// The voucher path must not shadow the coin path from the same
    /// entropy — different junction lists, different key material.
    #[test]
    fn voucher_path_diverges_from_coin_path() {
        let coins = CoinKeypairFactory::new(&ENTROPY);
        let vouchers = VoucherKeypairFactory::new(&ENTROPY);
        assert_ne!(vouchers.seed(0).0, coins.secret_bytes(0).unwrap()[..32]);
    }

    #[test]
    fn coin_derived_wallet_guards_the_signer_secret() {
        let factory = CoinKeypairFactory::new(&ENTROPY);
        let wallet = CoinDerivedWallet::for_index(&factory, 3).unwrap();
        let account_id = wallet.public_key();

        let secret = wallet.fetch_signer_secret(&account_id).unwrap();
        let rebuilt = schnorrkel::SecretKey::from_bytes(&secret).unwrap();
        assert_eq!(rebuilt.to_public().to_bytes(), account_id);

        let mut wrong = account_id;
        wrong[0] ^= 1;
        assert_eq!(
            wallet.fetch_signer_secret(&wrong),
            Err(CoinDerivedWalletError::UnexpectedAccount)
        );
    }
}

/// Required exact Bandersnatch primitive implementation. The ring exponent is
/// the chain's member-count exponent (9/10/14), not the PCS exponent.
/// Adapters must use the deployed Bandersnatch suite; a different curve or
/// synthetic proof is not a valid implementation. Seeds must not be retained.
pub trait VoucherCryptography: Send + Sync {
    /// Derive the Bandersnatch public member key.
    fn member_key(&self, seed: &VoucherSeed) -> Result<[u8; 32], String>;
    /// Sign an ownership message with the derived Bandersnatch key.
    fn sign(&self, seed: &VoucherSeed, message: &[u8]) -> Result<[u8; 64], String>;
    /// Derive the context-bound alias for this voucher.
    fn alias(&self, seed: &VoucherSeed, context: &[u8]) -> Result<[u8; 32], String>;
    /// Generate the canonical 785-byte ring-VRF proof.
    fn ring_vrf_proof(
        &self,
        seed: &VoucherSeed,
        ring_exponent: u8,
        ring_members: &[[u8; 32]],
        context: &[u8],
        message: &[u8],
    ) -> Result<Vec<u8>, String>;
}

// Derived from MIT-licensed host-rust-core product_account.rs. Copyright
// (c) 2026 Parity Technologies. The complete MIT notice is in LICENSE-MIT.
fn derive_sr25519_hard_path(
    entropy: &[u8],
    junctions: &[[u8; 32]],
) -> Result<schnorrkel::Keypair, String> {
    use schnorrkel::{ExpansionMode, derive::ChainCode};
    let mini_secret = substrate_bip39::mini_secret_from_entropy(entropy, "")
        .map_err(|error| format!("invalid BIP-39 entropy: {error:?}"))?;
    let mut keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    for junction in junctions {
        let chain_code = ChainCode(*junction);
        let (mini_secret, _) = keypair
            .secret
            .hard_derive_mini_secret_key(Some(chain_code), b"");
        keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    }
    Ok(keypair)
}

#[cfg(test)]
mod root_derivation_tests {
    #[test]
    fn entropy_expansion_matches_deployed_host() {
        let root = super::derive_sr25519_hard_path(&[0xAB; 16], &[]).unwrap();
        assert_eq!(
            hex::encode(root.public.to_bytes()),
            "0062ba8ae929ea64bc2ad6f21359e96a29e236a41d376d1c5ba76491da94fc72"
        );
    }
}
