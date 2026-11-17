//! F4 · Wallet reconstruction: the assets the local store no longer knows,
//! found again from the seed and the installation registry.
//!
//! F4 reconstructs assets, not operations (invariant 2). It adopts what the
//! chain shows under every known installation, never advancing a counter, and
//! a reconstructed entry waits out its dwell from the moment it is found
//! (invariant 3). The registry is the only thing that
//! survives a wipe (invariant 4).

use std::sync::Arc;

use chacha20poly1305::{
	ChaCha20Poly1305, Nonce,
	aead::{Aead, KeyInit},
};
use parity_scale_codec::{Decode, Encode};

use super::{
	Asset, ChainParams, CoinSecret, Exponent, PurseId,
	chain::CoinageChain,
	ledger::{self, AssetRecord, ScanProgress},
	namespace::{coin_key, entry_seed},
	plugins::Seed,
};
use crate::{installation::InstallationId, runtime::load_vrf, store::Db};

/// How far a scan reads (part of the layer's configuration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanConfig {
	/// Indices derived and read per request.
	pub batch: u32,
	/// Consecutive empty batches that end a default scan of one index space.
	pub gap: u32,
	/// The same for a deep scan; larger than `gap`.
	pub deep_gap: u32,
	/// Consecutive empty purses that end a walk over product purses.
	pub purse_gap: u32,
}

impl ScanConfig {
	/// The scan the core runs with.
	pub const SHIPPED: Self = Self { batch: 32, gap: 2, deep_gap: 8, purse_gap: 2 };
}

/// Which purses a reconstruction scans; the main purse always.
#[derive(Clone, Debug, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
pub enum Purses {
	/// Only the main purse.
	MainOnly,
	/// The main purse and these.
	These(Vec<PurseId>),
	/// The main purse and product purses from zero upwards, until
	/// `purse_gap` consecutive purses are empty.
	Walk,
}

/// The installation registry's records, as the contract returns them for
/// the `//datastore` account.
#[async_trait::async_trait]
pub trait InstallationRegistry: Send + Sync {
	/// Every record; none when the read failed.
	async fn records(&self) -> Option<Vec<Vec<u8>>>;
}

/// What a reconstruction reads and writes.
pub struct ReconstructionDeps {
	/// The shared store.
	pub store: Db,
	/// The recovery phrase's entropy.
	pub seed: Arc<dyn Seed>,
	/// Reads of the People chain.
	pub chain: Arc<dyn CoinageChain>,
	/// The installation registry.
	pub registry: Arc<dyn InstallationRegistry>,
}

/// Why a reconstruction stopped.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum ReconstructError {
	/// The seed, the keys or the chain are unavailable.
	#[display("the seed, the keys or the chain are unavailable")]
	Unavailable,
	/// The store failed.
	Store(crate::store::DbError),
}

/// The key installation records are sealed under (F4 invariant 4), shared
/// with the shipped wallets so each opens the records the others wrote.
pub struct RegistryKey([u8; 32]);

impl RegistryKey {
	/// The key of the seed whose `//datastore` secret is `datastore`.
	pub fn of(datastore: &CoinSecret) -> Option<Self> {
		let secret = schnorrkel::SecretKey::from_bytes(&datastore.0).ok()?;
		Some(Self::from_ed25519_secret(&secret.to_ed25519_bytes()))
	}

	/// `BLAKE2b-256("encryption", key = secret)`, where `secret` is the
	/// `//datastore` sr25519 secret in its ed25519-compatible form: the scalar
	/// times 8, then the nonce half.
	pub fn from_ed25519_secret(secret: &[u8; 64]) -> Self {
		Self(keyed_hash(secret, b"encryption"))
	}

	/// `installation` sealed for the registry: `nonce(12) ‖ ciphertext(32) ‖
	/// tag(16)`, with the nonce a keyed hash of the identifier, so sealing one
	/// identifier twice gives the same bytes.
	pub fn seal(&self, installation: InstallationId) -> [u8; 60] {
		let nonce_key = keyed_hash(&self.0, b"nonce-key");
		let nonce_hash = keyed_hash(&nonce_key, &[b"nonce".as_slice(), &installation.0].concat());
		let nonce = Nonce::from_slice(&nonce_hash[..12]);
		let cipher = ChaCha20Poly1305::new((&self.0).into());
		let ciphertext = cipher.encrypt(nonce, installation.0.as_slice()).expect("a 32-byte message encrypts");
		let mut sealed = [0u8; 60];
		sealed[..12].copy_from_slice(nonce);
		sealed[12..].copy_from_slice(&ciphertext);
		sealed
	}

	/// The identifier in `record`, whatever its nonce; none when it does not
	/// open under this key or is malformed.
	pub fn open(&self, record: &[u8]) -> Option<InstallationId> {
		if record.len() != 60 {
			return None;
		}
		let cipher = ChaCha20Poly1305::new((&self.0).into());
		let plaintext = cipher.decrypt(Nonce::from_slice(&record[..12]), &record[12..]).ok()?;
		Some(InstallationId(plaintext.try_into().ok()?))
	}
}

/// `BLAKE2b-256(message, key = key)`.
fn keyed_hash(key: &[u8], message: &[u8]) -> [u8; 32] {
	let hash = blake2b_simd::Params::new().hash_length(32).key(key).hash(message);
	hash.as_bytes().try_into().expect("the hash is 32 bytes")
}

/// Where an interrupted scan of one installation resumes (F4): the purse,
/// the index space and the next index, SCALE-encoded as the installation's
/// `ScanProgress::resume`. An empty or unreadable `resume`, or one naming a
/// purse the reconstruction does not list, starts the scan from the
/// beginning.
#[derive(Clone, Copy, Encode, Decode)]
struct Position {
	purse: PurseId,
	space: Space,
	next: u32,
}

/// A namespace's two index spaces, in the order a scan reads them.
#[derive(Clone, Copy, PartialEq, PartialOrd, Encode, Decode)]
enum Space {
	Coins,
	Entries,
}

/// Reconstructs `purses` under every known installation: adds the
/// installations the registry lists, then scans each namespace's coins and
/// entries, adopting what the chain shows. Returns how many assets it
/// adopted. Each installation's scan continues from the progress B2 stores
/// for it, which is recorded after every batch.
pub async fn reconstruct(
	deps: &ReconstructionDeps,
	purses: Purses,
	deep: bool,
	config: &ScanConfig,
	params: &ChainParams,
) -> Result<usize, ReconstructError> {
	let entropy = deps.seed.entropy().ok_or(ReconstructError::Unavailable)?;
	if let Some(records) = deps.registry.records().await {
		let (datastore, _) = super::namespace::datastore_key(&entropy).map_err(|_| ReconstructError::Unavailable)?;
		let key = RegistryKey::of(&datastore).ok_or(ReconstructError::Unavailable)?;
		let learned: Vec<InstallationId> = records.iter().filter_map(|record| key.open(record)).collect();
		deps.store.write(move |tx| ledger::add_installations(tx, &learned)).await?;
	}
	let view = deps.store.read(ledger::snapshot).await?;
	let at = deps.chain.heads().await.map_err(|_| ReconstructError::Unavailable)?.finalized.hash;
	let context = ScanContext { entropy: &entropy, at, gap: if deep { config.deep_gap } else { config.gap }, batch: config.batch, params };
	let mut adopted = 0;
	for (installation, progress) in view.installations().to_vec() {
		let mut scanned: Vec<PurseId> = match &purses {
			Purses::MainOnly | Purses::Walk => vec![PurseId::MAIN],
			Purses::These(listed) => core::iter::once(PurseId::MAIN).chain(listed.iter().copied()).collect(),
		};
		scanned.extend(view.purses().map(|purse| purse.id).filter(|id| *id != PurseId::MAIN));
		scanned.sort();
		scanned.dedup();
		let resume = Position::decode(&mut progress.resume.as_slice())
			.ok()
			.filter(|position| scanned.contains(&position.purse));
		let first_scan_done = progress.first_scan_done;
		let skipped = |purse: &PurseId| resume.is_some_and(|position| position.purse != *purse);
		for purse in scanned.iter().copied().skip_while(skipped) {
			let namespace = Namespace { purse, installation, first_scan_done };
			adopted += scan(deps, &context, namespace, resume.filter(|position| position.purse == purse)).await?;
		}
		if purses == Purses::Walk {
			let mut empty = 0;
			let mut purse = 0;
			while empty < config.purse_gap {
				let namespace = Namespace { purse: PurseId(purse), installation, first_scan_done };
				let found = scan(deps, &context, namespace, None).await?;
				adopted += found;
				empty = if found == 0 { empty + 1 } else { 0 };
				purse += 1;
			}
		}
		let progress = ScanProgress { first_scan_done: true, resume: Vec::new() };
		deps.store.write(move |tx| ledger::record_scan(tx, installation, &progress)).await?;
	}
	Ok(adopted)
}

/// What every namespace of one reconstruction is scanned with.
struct ScanContext<'a> {
	entropy: &'a [u8],
	at: subxt::utils::H256,
	gap: u32,
	batch: u32,
	params: &'a ChainParams,
}

/// One namespace, with whether its installation's first scan has completed,
/// which the progress recorded after each batch keeps.
#[derive(Clone, Copy)]
struct Namespace {
	purse: PurseId,
	installation: InstallationId,
	first_scan_done: bool,
}

/// Scans one namespace's coin and entry spaces, batch by batch, from
/// `resume` when given, until `gap` consecutive batches are empty, adopting
/// every index the chain shows and the ledger does not hold, and recording
/// after each batch where the scan resumes.
async fn scan(
	deps: &ReconstructionDeps,
	context: &ScanContext<'_>,
	namespace: Namespace,
	resume: Option<Position>,
) -> Result<usize, ReconstructError> {
	let ScanContext { entropy, at, gap, batch, params } = *context;
	let Namespace { purse, installation, first_scan_done } = namespace;
	let vrf = load_vrf().await.map_err(|_| ReconstructError::Unavailable)?;
	let instance = params.config().instance;
	let mut adopted = 0;
	for space in [Space::Coins, Space::Entries] {
		if resume.is_some_and(|position| space < position.space) {
			continue;
		}
		let mut empty = 0;
		let mut start = resume.filter(|position| position.space == space).map_or(0, |position| position.next);
		while empty < gap {
			let indices: Vec<u32> = (start..start.saturating_add(batch)).collect();
			let found: Vec<AssetRecord> = if space == Space::Coins {
				let accounts: Vec<[u8; 32]> = indices
					.iter()
					.filter_map(|index| coin_key(entropy, purse, installation, *index).ok().map(|(_, account)| account))
					.collect();
				let read = deps.chain.coins_at(at, &accounts).await.map_err(|_| ReconstructError::Unavailable)?;
				indices
					.iter()
					.zip(read)
					.filter_map(|(index, coin)| {
						let coin = coin.filter(|coin| coin.instance == instance)?;
						let exponent = Exponent::from_chain(coin.value, params)?;
						let asset = Asset::Coin { purse, installation, index: *index };
						Some(AssetRecord { asset, exponent })
					})
					.collect()
			} else {
				let members: Vec<[u8; 32]> = indices
					.iter()
					.filter_map(|index| vrf.member(&entry_seed(entropy, purse, installation, *index).0).ok())
					.collect();
				let read = deps.chain.members_at(at, &members).await.map_err(|_| ReconstructError::Unavailable)?;
				indices
					.iter()
					.zip(read)
					.filter_map(|(index, loaded)| {
						let (_, value) = loaded.filter(|(loaded_instance, _)| *loaded_instance == instance)?;
						let exponent = Exponent::from_chain(value, params)?;
						let asset = Asset::Entry { purse, installation, index: *index };
						Some(AssetRecord { asset, exponent })
					})
					.collect()
			};
			empty = if found.is_empty() { empty + 1 } else { 0 };
			adopted += found.len();
			start = start.saturating_add(batch);
			let progress = ScanProgress { first_scan_done, resume: Position { purse, space, next: start }.encode() };
			deps.store
				.write(move |tx| {
					ledger::adopt(tx, &found)?;
					ledger::record_scan(tx, installation, &progress)
				})
				.await?;
		}
	}
	Ok(adopted)
}

#[cfg(test)]
mod tests;
