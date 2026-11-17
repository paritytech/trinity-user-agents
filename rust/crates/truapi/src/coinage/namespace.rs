//! A2 · Namespace: the derivation paths of coin and entry keys, and the keys
//! computed from them.
//!
//! ```text
//! coins      //coinage //<purse> //<installation> /<item>             sr25519; the item junction is soft
//! entries    //coinage-ring-vrf //<purse> //<installation> //<item>   bandersnatch; every junction is hard
//! registry   //datastore                                              sr25519
//! ```
//!
//! Chain codes are built from the junction values as the design defines them,
//! never by parsing a path string: `sp-core`'s parser would read the
//! installation's hex as text and derive different keys.

use parity_scale_codec::{Decode, Encode};
use schnorrkel::{
	ExpansionMode, Keypair,
	derive::{ChainCode, Derivation},
};

use crate::{host_internal::sso_messages::RingVrfError, installation::InstallationId, runtime::Vrf};

/// One isolated balance. The main purse is [`PurseId::MAIN`]; other purses
/// count up from zero, so none can equal it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct PurseId(pub u32);

uniffi::custom_type!(PurseId, u32, {
	lower: |purse| purse.0,
	try_lift: |id| Ok(PurseId(id)),
});

impl PurseId {
	/// The main purse, which every wallet has.
	pub const MAIN: Self = Self(u32::MAX);
}

/// The sr25519 secret controlling one coin account: the secret scalar, then
/// the nonce, 32 bytes each, in schnorrkel's canonical layout.
///
/// Its `Debug` output never shows the secret.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinSecret(pub [u8; 64]);

uniffi::custom_type!(CoinSecret, Vec<u8>, {
	lower: |secret| secret.0.to_vec(),
	try_lift: |bytes| Ok(CoinSecret(bytes.as_slice().try_into()?)),
});

impl core::fmt::Debug for CoinSecret {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter.write_str("CoinSecret(..)")
	}
}

/// The seed of an entry's Bandersnatch secret.
///
/// Its `Debug` output never shows the seed.
#[derive(Clone, PartialEq, Eq)]
pub struct EntrySeed(pub [u8; 32]);

impl core::fmt::Debug for EntrySeed {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter.write_str("EntrySeed(..)")
	}
}

/// The BIP-39 entropy is not 16 or 32 bytes long.
#[derive(Clone, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("the recovery phrase's entropy is not 16 or 32 bytes")]
pub struct InvalidEntropy;

/// One junction of a derivation path.
enum Junction<'a> {
	Name(&'a str),
	Number(u64),
	Installation(InstallationId),
}

impl Junction<'_> {
	/// The junction's 32-byte chain code: a name or a number SCALE-encoded
	/// and padded with zeros, an installation's own bytes.
	fn chain_code(&self) -> [u8; 32] {
		let encoded = match self {
			Self::Name(name) => name.encode(),
			Self::Number(number) => number.encode(),
			Self::Installation(installation) => return installation.0,
		};
		let mut code = [0u8; 32];
		code[..encoded.len()].copy_from_slice(&encoded);
		code
	}
}

const COINS: &str = "coinage";
const ENTRIES: &str = "coinage-ring-vrf";
const DATASTORE: &str = "datastore";

/// The secret and the account of the coin at `item` of `purse` under
/// `installation`.
///
/// The secret's scalar half and the account are the same on every call; the
/// nonce half is not, because a soft-derived key's nonce is drawn afresh.
pub fn coin_key(
	entropy: &[u8],
	purse: PurseId,
	installation: InstallationId,
	item: u32,
) -> Result<(CoinSecret, [u8; 32]), InvalidEntropy> {
	let parent = hard_path(
		root(entropy)?,
		&[
			Junction::Name(COINS),
			Junction::Number(u64::from(purse.0)),
			Junction::Installation(installation),
		],
	);
	let code = ChainCode(Junction::Number(u64::from(item)).chain_code());
	let (coin, _) = parent.derived_key_simple(code, []);
	Ok((CoinSecret(coin.secret.to_bytes()), coin.public.to_bytes()))
}

/// The key of the `//datastore` account, which pays for the installation
/// registry's appends: its secret, in the layout of [`CoinSecret`], and its
/// account.
pub fn datastore_key(entropy: &[u8]) -> Result<(CoinSecret, [u8; 32]), InvalidEntropy> {
	let keypair = hard_path(root(entropy)?, &[Junction::Name(DATASTORE)]);
	Ok((CoinSecret(keypair.secret.to_bytes()), keypair.public.to_bytes()))
}

/// The seed of the entry at `item` of `purse` under `installation`: the
/// entropy hashed once per junction, with keyed BLAKE2b-256 whose key is the
/// junction's chain code.
pub fn entry_seed(entropy: &[u8], purse: PurseId, installation: InstallationId, item: u32) -> EntrySeed {
	let junctions = [
		Junction::Name(ENTRIES),
		Junction::Number(u64::from(purse.0)),
		Junction::Installation(installation),
		Junction::Number(u64::from(item)),
	];
	let mut seed = entropy.to_vec();
	for junction in junctions {
		seed = blake2b_simd::Params::new()
			.hash_length(32)
			.key(&junction.chain_code())
			.hash(&seed)
			.as_bytes()
			.to_vec();
	}
	EntrySeed(seed.try_into().expect("BLAKE2b-256 hashes to 32 bytes"))
}

/// The seed of the entry at `item` of `purse` under `installation`, and its
/// public member key as a ring lists it.
pub fn entry_key(
	entropy: &[u8],
	purse: PurseId,
	installation: InstallationId,
	item: u32,
	vrf: &Vrf,
) -> Result<(EntrySeed, [u8; 32]), RingVrfError> {
	let seed = entry_seed(entropy, purse, installation, item);
	let member = vrf.member(&seed.0)?;
	Ok((seed, member))
}

/// The root key of the recovery phrase whose entropy is `entropy`.
fn root(entropy: &[u8]) -> Result<Keypair, InvalidEntropy> {
	let mini_secret = substrate_bip39::mini_secret_from_entropy(entropy, "").map_err(|_| InvalidEntropy)?;
	Ok(mini_secret.expand_to_keypair(ExpansionMode::Ed25519))
}

/// `keypair` derived along hard junctions.
fn hard_path(mut keypair: Keypair, junctions: &[Junction<'_>]) -> Keypair {
	for junction in junctions {
		let code = ChainCode(junction.chain_code());
		let (mini_secret, _) = keypair.secret.hard_derive_mini_secret_key(Some(code), b"");
		keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
	}
	keypair
}

#[cfg(test)]
mod tests;
