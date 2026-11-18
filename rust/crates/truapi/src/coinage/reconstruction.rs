//! F4 · Wallet reconstruction: the assets the local store no longer knows,
//! found again from the seed and the installation registry.
//!
//! F4 reconstructs assets, not operations (invariant 2). It adopts what the
//! chain shows under every known installation, never advancing a counter, and
//! a reconstructed entry waits out its dwell from the moment it is found
//! (invariant 3). The registry is the only thing that
//! survives a wipe (invariant 4).

use std::{collections::BTreeMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};

use super::{
	Asset, ChainParams, Exponent, PurseId,
	assembly::UNLOADING_RECYCLER_CONTEXT,
	chain::{AliasState, CoinageChain, RingPosition},
	handover::{self, Handover},
	ledger::{self, AssetRecord, ScanProgress},
	namespace::{RegistryKey, coin_key, entry_seed},
	plugins::Seed,
};
use crate::{
	installation::InstallationId,
	runtime::{Vrf, load_vrf},
	store::Db,
};

/// How far a scan reads (part of the layer's configuration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanConfig {
	/// Indices derived and read per request.
	pub batch: u32,
	/// Consecutive empty batches that end a scan of one index space.
	pub gap: u32,
	/// Batches a deep scan reads past where the last scan of an index space
	/// stopped.
	pub deep_batches: u32,
	/// Consecutive empty purses that end a walk over product purses.
	pub purse_gap: u32,
}

impl ScanConfig {
	/// The scan the core runs with, the shipped wallet's: a default scan of
	/// a space ends after 2,000 consecutive empty indices, and a deep scan
	/// reads 5,000 more.
	pub const SHIPPED: Self = Self { batch: 500, gap: 4, deep_batches: 10, purse_gap: 2 };
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

/// Where the next scan of one index space of one installation's namespace
/// starts (F4): the index after the last batch read. An installation's
/// cursors are SCALE-encoded as its `ScanProgress::resume`; a space with no
/// cursor, or an unreadable `resume`, starts at index 0.
#[derive(Clone, Copy, Encode, Decode)]
struct Cursor {
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
	run(deps, &purses, deep, config, params, Installations::All).await
}

/// The default reconstruction (F4 invariant 5): adds the installations the
/// registry lists, then scans the main purse and every purse the store
/// knows under each installation whose first scan has not completed, so an
/// installation is scanned in full once. Returns how many assets it adopted.
pub async fn reconstruct_default(
	deps: &ReconstructionDeps,
	config: &ScanConfig,
	params: &ChainParams,
) -> Result<usize, ReconstructError> {
	run(deps, &Purses::MainOnly, false, config, params, Installations::Unscanned).await
}

/// Which known installations a reconstruction scans.
#[derive(Clone, Copy, PartialEq)]
enum Installations {
	All,
	Unscanned,
}

async fn run(
	deps: &ReconstructionDeps,
	purses: &Purses,
	deep: bool,
	config: &ScanConfig,
	params: &ChainParams,
	installations: Installations,
) -> Result<usize, ReconstructError> {
	let entropy = deps.seed.entropy().ok_or(ReconstructError::Unavailable)?;
	if let Some(records) = deps.registry.records().await {
		let (datastore, _) = super::namespace::datastore_key(&entropy).map_err(|_| ReconstructError::Unavailable)?;
		let key = RegistryKey::of(&datastore).ok_or(ReconstructError::Unavailable)?;
		let learned: Vec<InstallationId> = records.iter().filter_map(|record| key.open(record)).collect();
		deps.store.write(move |tx| ledger::add_installations(tx, &learned)).await?;
	}
	let (view, hand_over) = deps.store.read(|conn| Ok((ledger::snapshot(conn)?, handover::read(conn)?))).await?;
	let at = deps.chain.heads().await.map_err(|_| ReconstructError::Unavailable)?.finalized.hash;
	let limit = if deep { Limit::Batches(config.deep_batches) } else { Limit::Gap(config.gap) };
	let context = ScanContext { entropy: &entropy, at, limit, batch: config.batch, params, hand_over: hand_over.as_ref() };
	let known: Vec<PurseId> = view.purses().map(|purse| purse.id).collect();
	let mut adopted = 0;
	let mut failure = None;
	for (installation, progress) in view.installations().to_vec() {
		if installations == Installations::Unscanned && progress.first_scan_done {
			continue;
		}
		let mut scan = InstallationScan {
			installation,
			first_scan_done: progress.first_scan_done,
			cursors: Vec::decode(&mut progress.resume.as_slice()).unwrap_or_default(),
		};
		match scan.run(deps, &context, purses, &known, config.purse_gap).await {
			Ok(found) => adopted += found,
			Err(error) => {
				tracing::warn!(%error, "an installation's scan stopped; the next reconstruction continues it");
				failure.get_or_insert(error);
			},
		}
	}
	failure.map_or(Ok(adopted), Err)
}

/// When a scan of one index space ends (F4).
#[derive(Clone, Copy)]
enum Limit {
	/// After this many consecutive empty batches: the default scan.
	Gap(u32),
	/// After this many batches: a deep scan.
	Batches(u32),
}

impl Limit {
	fn reached(self, batches: u32, empty: u32) -> bool {
		match self {
			Self::Gap(gap) => empty >= gap,
			Self::Batches(count) => batches >= count,
		}
	}
}

/// What every namespace of one reconstruction is scanned with.
struct ScanContext<'a> {
	entropy: &'a [u8],
	at: subxt::utils::H256,
	limit: Limit,
	batch: u32,
	params: &'a ChainParams,
	hand_over: Option<&'a Handover>,
}

/// One installation's scan: the cursors of its index spaces, and whether its
/// first scan has completed, which the progress recorded after each batch
/// keeps.
struct InstallationScan {
	installation: InstallationId,
	first_scan_done: bool,
	cursors: Vec<Cursor>,
}

impl InstallationScan {
	/// Scans the namespace of every purse in `purses`, and of every purse
	/// in `known`, then records the first scan as done. Returns how many
	/// assets it adopted; the first failed read or write ends the scan, and
	/// the cursors recorded so far keep what it covered.
	async fn run(
		&mut self,
		deps: &ReconstructionDeps,
		context: &ScanContext<'_>,
		purses: &Purses,
		known: &[PurseId],
		purse_gap: u32,
	) -> Result<usize, ReconstructError> {
		let mut scanned: Vec<PurseId> = match purses {
			Purses::MainOnly | Purses::Walk => vec![PurseId::MAIN],
			Purses::These(listed) => core::iter::once(PurseId::MAIN).chain(listed.iter().copied()).collect(),
		};
		scanned.extend(known.iter().copied().filter(|id| *id != PurseId::MAIN));
		scanned.sort();
		scanned.dedup();
		let mut adopted = 0;
		for purse in scanned {
			adopted += self.scan(deps, context, purse).await?;
		}
		if *purses == Purses::Walk {
			let mut empty = 0;
			let mut purse = 0;
			while empty < purse_gap {
				let found = self.scan(deps, context, PurseId(purse)).await?;
				adopted += found;
				empty = if found == 0 && !known.contains(&PurseId(purse)) { empty + 1 } else { 0 };
				purse += 1;
			}
		}
		self.first_scan_done = true;
		let (installation, progress) = (self.installation, self.progress());
		deps.store.write(move |tx| ledger::record_scan(tx, installation, &progress)).await?;
		Ok(adopted)
	}

	/// Scans one namespace's coin and entry spaces, batch by batch, each from
	/// its cursor until the context's limit, adopting every index the chain
	/// shows and the ledger does not hold, and recording the cursors after
	/// each batch.
	async fn scan(
		&mut self,
		deps: &ReconstructionDeps,
		context: &ScanContext<'_>,
		purse: PurseId,
	) -> Result<usize, ReconstructError> {
		let ScanContext { entropy, at, limit, batch, params, hand_over } = *context;
		let installation = self.installation;
		let marks = hand_over.filter(|_| purse == PurseId::MAIN).and_then(|hand_over| hand_over.marks(installation));
		let vrf = load_vrf().await.map_err(|_| ReconstructError::Unavailable)?;
		let instance = params.config().instance;
		let mut adopted = 0;
		for space in [Space::Coins, Space::Entries] {
			let mut start = self.next(purse, space);
			let mark = marks.map_or(0, |(coins, entries)| if space == Space::Coins { coins } else { entries });
			let (mut batches, mut empty) = (0, 0);
			while !limit.reached(batches, empty) || start < mark {
				let indices: Vec<u32> = (start..start.saturating_add(batch)).collect();
				let mut disclosed = Vec::new();
				let found: Vec<AssetRecord> = if space == Space::Coins {
					let accounts = indices
						.iter()
						.map(|index| coin_key(entropy, purse, installation, *index).map(|(_, account)| account))
						.collect::<Result<Vec<_>, _>>()
						.map_err(|_| ReconstructError::Unavailable)?;
					let read = deps.chain.coins_at(at, &accounts).await.map_err(|_| ReconstructError::Unavailable)?;
					indices
						.iter()
						.zip(accounts)
						.zip(read)
						.filter_map(|((index, account), coin)| {
							let coin = coin.filter(|coin| coin.instance == instance)?;
							let exponent = Exponent::from_chain(coin.value, params)?;
							let asset = Asset::Coin { purse, installation, index: *index };
							if let Some(payment) = hand_over.and_then(|hand_over| hand_over.payment_of(&account)) {
								disclosed.push((payment, asset, exponent));
							}
							Some(AssetRecord { asset, exponent })
						})
						.collect()
				} else {
					let members: Vec<[u8; 32]> = indices
						.iter()
						.filter_map(|index| vrf.member(&entry_seed(entropy, purse, installation, *index).0).ok())
						.collect();
					let read = deps.chain.members_at(at, &members).await.map_err(|_| ReconstructError::Unavailable)?;
					let loaded: Vec<(u32, i8, AssetRecord)> = indices
						.iter()
						.zip(read)
						.filter_map(|(index, loaded)| {
							let (_, value) = loaded.filter(|(loaded_instance, _)| *loaded_instance == instance)?;
							let exponent = Exponent::from_chain(value, params)?;
							let asset = Asset::Entry { purse, installation, index: *index };
							Some((*index, value, AssetRecord { asset, exponent }))
						})
						.collect();
					not_unloaded(deps, &vrf, entropy, at, (purse, installation), loaded).await?
				};
				batches += 1;
				empty = if found.is_empty() { empty + 1 } else { 0 };
				adopted += found.len();
				start = start.saturating_add(batch);
				self.advance(purse, space, start);
				let progress = self.progress();
				deps.store
					.write(move |tx| {
						ledger::adopt(tx, &found)?;
						for (payment, coin, exponent) in disclosed {
							handover::adopt_disclosed(tx, payment, coin, exponent)?;
						}
						ledger::record_scan(tx, installation, &progress)
					})
					.await?;
			}
		}
		Ok(adopted)
	}

	fn next(&self, purse: PurseId, space: Space) -> u32 {
		self.cursors.iter().find(|cursor| cursor.purse == purse && cursor.space == space).map_or(0, |cursor| cursor.next)
	}

	fn advance(&mut self, purse: PurseId, space: Space, next: u32) {
		match self.cursors.iter_mut().find(|cursor| cursor.purse == purse && cursor.space == space) {
			Some(cursor) => cursor.next = next,
			None => self.cursors.push(Cursor { purse, space, next }),
		}
	}

	fn progress(&self) -> ScanProgress {
		ScanProgress { first_scan_done: self.first_scan_done, resume: self.cursors.encode() }
	}
}
/// The entries of `loaded`, each given with its index and the exponent the
/// chain shows, whose alias in their ring is not unloaded.
///
/// An unload names only the entry's alias, so an unloaded entry's member key
/// stays in its ring until the ring is cleaned up. An entry that is
/// onboarding was never in a ring, and one that holds no ring index has no
/// alias to read, so both are kept.
async fn not_unloaded(
	deps: &ReconstructionDeps,
	vrf: &Vrf,
	entropy: &[u8],
	at: subxt::utils::H256,
	(purse, installation): (PurseId, InstallationId),
	loaded: Vec<(u32, i8, AssetRecord)>,
) -> Result<Vec<AssetRecord>, ReconstructError> {
	let mut by_exponent: BTreeMap<i8, Vec<([u8; 32], AssetRecord)>> = BTreeMap::new();
	for (index, exponent, record) in loaded {
		by_exponent.entry(exponent).or_default().push((entry_seed(entropy, purse, installation, index).0, record));
	}
	let mut kept = Vec::new();
	for (exponent, group) in by_exponent {
		let members =
			group.iter().map(|(seed, _)| vrf.member(seed)).collect::<Result<Vec<_>, _>>().map_err(|_| ReconstructError::Unavailable)?;
		let places = deps.chain.places_at(at, exponent, &members).await.map_err(|_| ReconstructError::Unavailable)?;
		let mut in_rings: BTreeMap<u32, Vec<([u8; 32], AssetRecord)>> = BTreeMap::new();
		for ((seed, record), place) in group.into_iter().zip(places) {
			let Some(RingPosition::Included(place)) = place else {
				kept.push(record);
				continue;
			};
			let alias = vrf.alias(&seed, &UNLOADING_RECYCLER_CONTEXT).map_err(|_| ReconstructError::Unavailable)?;
			in_rings.entry(place.ring).or_default().push((alias, record));
		}
		for (ring, group) in in_rings {
			let aliases: Vec<[u8; 32]> = group.iter().map(|(alias, _)| *alias).collect();
			let states = deps.chain.aliases_at(at, exponent, ring, &aliases).await.map_err(|_| ReconstructError::Unavailable)?;
			let unloaded = |state: &Option<AliasState>| *state == Some(AliasState::Unloaded);
			kept.extend(group.into_iter().zip(states).filter(|(_, state)| !unloaded(state)).map(|((_, record), _)| record));
		}
	}
	Ok(kept)
}

#[cfg(test)]
mod tests;
