//! Finds what a seed holds under keys it derives in order, by reading them until a run of
//! empty ones.
//!
//! [`scan_until_gap`] scans one index space, the keys of one purse under one installation:
//! it derives keys in batches through a [`KeyRange`], reads what they hold through a
//! [`HoldingLookup`], and stops after a run of empty keys. [`PurseScanner`] scans many purses
//! under every installation in a list of [`PurseScanPass`]es: each pass takes purse ids from its
//! own [`PurseScanIdGenerator`] and may stop after a run of empty purses, a purse an earlier
//! pass scanned is skipped, and what each purse holds goes to a [`ScannedPursesStore`].

use core::{fmt::Display, num::NonZeroU32, ops::Range};
use std::{collections::HashSet, hash::Hash, sync::Arc};

use futures::{StreamExt, stream::BoxStream};
use truapi::latest::GenericError;

use crate::{
	chain_runtime::RuntimeFailure, installation::InstallationId,
	installation_registry::RegistryReadError, store::DbError,
};

/// Derives the keys of a purse's index space under one installation.
pub trait KeyRange: Send + Sync {
	/// The id of a purse.
	type PurseId;
	/// The key one index derives.
	type Key;
	/// Why a range could not be derived.
	type Error;

	/// The keys of `purse` allocated by `installation` at `indices`, one per index, in index
	/// order.
	fn derive_range(
		&self,
		purse: &Self::PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<Self::Key>, Self::Error>;
}

/// Reads what keys hold on chain.
#[async_trait::async_trait]
pub trait HoldingLookup<Key: Sync>: Send + Sync {
	/// What a key holds.
	type Holding;
	/// Why a read failed. A failed read is never an empty result.
	type Error;

	/// What each of `keys` holds, in the same order, with `None` for a key that holds nothing.
	async fn read_holdings(&self, keys: &[Key]) -> Result<Vec<Option<Self::Holding>>, Self::Error>;
}

/// How a scan of one index space reads its keys and when it ends.
pub struct GapScanConfig {
	/// The scan ends once more than this many keys in a row have held nothing.
	pub gap_size: u32,
	/// The number of keys derived and read in one batch.
	pub batch_size: NonZeroU32,
}

/// A key that holds something, with its index.
pub struct FoundKey<Key, Holding> {
	/// The key's index in its index space.
	pub index: u32,
	/// The key.
	pub key: Key,
	/// What it holds.
	pub holding: Holding,
}

/// Why a scan of one index space stopped before its gap.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum GapScanError<DeriveError, LookupError> {
	/// A batch of keys could not be derived.
	#[display("deriving keys failed: {_0}")]
	Derive(#[error(not(source))] DeriveError),
	/// A batch of keys could not be read.
	#[display("reading holdings failed: {_0}")]
	Lookup(#[error(not(source))] LookupError),
	/// A read returned a different number of results than keys it was given.
	#[display("a read of {keys} keys returned {results} results, expected one per key")]
	ResultCountMismatch {
		/// The number of keys read.
		keys: usize,
		/// The number of results returned.
		results: usize,
	},
}

/// The keys found in one index space, in index order.
pub type FoundKeys<Keys, Lookup> = Vec<
	FoundKey<<Keys as KeyRange>::Key, <Lookup as HoldingLookup<<Keys as KeyRange>::Key>>::Holding>,
>;

/// The error of [`scan_until_gap`] for these keys and this lookup.
pub type ScanError<Keys, Lookup> = GapScanError<
	<Keys as KeyRange>::Error,
	<Lookup as HoldingLookup<<Keys as KeyRange>::Key>>::Error,
>;

/// Scans the index space of `purse` under `installation` from index 0 in whole batches and
/// returns every key that holds something, in index order.
///
/// Before each batch it stops if more than `config.gap_size` keys in a row have held nothing,
/// so at least that many plus one empty keys follow the last key found. A batch is always read
/// whole, and what it finds past the gap is kept. Fails at the first batch that cannot be
/// derived or read.
pub async fn scan_until_gap<Keys, Lookup>(
	keys: &Keys,
	purse: &Keys::PurseId,
	installation: InstallationId,
	lookup: &Lookup,
	config: &GapScanConfig,
) -> Result<FoundKeys<Keys, Lookup>, ScanError<Keys, Lookup>>
where
	Keys: KeyRange + ?Sized,
	Keys::Key: Sync,
	Lookup: HoldingLookup<Keys::Key> + ?Sized,
{
	let mut found = Vec::new();
	let mut position = ScanPosition { next_index: 0, empty_run: 0 };
	while let Some(indices) = position.next_batch_indices(config) {
		let derived = keys
			.derive_range(purse, installation, indices.clone())
			.map_err(GapScanError::Derive)?;
		let hits = read_batch::<Keys, Lookup>(derived, lookup, indices.clone()).await?;
		position = position.advance_for_scan_result(indices, hits.last().map(|hit| hit.index));
		found.extend(hits);
	}
	Ok(found)
}

/// Where a scan of one index space stands.
struct ScanPosition {
	/// The next index to read.
	next_index: u32,
	/// The keys in a row that held nothing, up to `next_index`.
	empty_run: u32,
}

impl ScanPosition {
	/// The indices of the next batch, or `None` once more than `config.gap_size` keys in a row
	/// have held nothing or no index is left.
	fn next_batch_indices(&self, config: &GapScanConfig) -> Option<Range<u32>> {
		if self.empty_run > config.gap_size || self.next_index == u32::MAX {
			return None;
		}
		let end = self.next_index.saturating_add(config.batch_size.get());
		Some(self.next_index..end)
	}

	/// The position after reading `scanned_indices`, in which `last_hit` is the highest index
	/// that held something, if any did.
	fn advance_for_scan_result(&self, scanned_indices: Range<u32>, last_hit: Option<u32>) -> Self {
		let empty_run = match last_hit {
			Some(hit) => scanned_indices.end - hit - 1,
			None => self.empty_run.saturating_add(scanned_indices.end - scanned_indices.start),
		};
		Self { next_index: scanned_indices.end, empty_run }
	}
}

/// Reads the keys derived at `indices` and returns those that hold something.
async fn read_batch<Keys, Lookup>(
	derived: Vec<Keys::Key>,
	lookup: &Lookup,
	indices: Range<u32>,
) -> Result<FoundKeys<Keys, Lookup>, ScanError<Keys, Lookup>>
where
	Keys: KeyRange + ?Sized,
	Keys::Key: Sync,
	Lookup: HoldingLookup<Keys::Key> + ?Sized,
{
	let holdings = lookup.read_holdings(&derived).await.map_err(GapScanError::Lookup)?;
	if holdings.len() != derived.len() {
		return Err(GapScanError::ResultCountMismatch {
			keys: derived.len(),
			results: holdings.len(),
		});
	}
	Ok(build_found_keys(indices, derived, holdings))
}

/// The keys of `derived`, at `indices`, whose entry in `holdings` holds something.
fn build_found_keys<Key, Holding>(
	indices: Range<u32>,
	derived: Vec<Key>,
	holdings: Vec<Option<Holding>>,
) -> Vec<FoundKey<Key, Holding>> {
	indices
		.zip(derived)
		.zip(holdings)
		.filter_map(|((index, key), holding)| Some(FoundKey { index, key, holding: holding? }))
		.collect()
}

/// Why a backup recovery run stopped before it finished.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
	/// The finalized block or a contract read could not be reached.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The installation registry could not be read.
	#[error(transparent)]
	Registry(#[from] RegistryReadError),
	/// The current installation is not known yet.
	#[error("the current installation is unknown: {0}")]
	Installation(GenericError),
	/// A purse could not be scanned.
	#[error("scanning purse {purse} failed: {reason}")]
	Scan {
		/// The purse's id.
		purse: String,
		/// Why its scan failed.
		reason: String,
	},
	/// The core database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The purse ids one pass scans, in order.
pub trait PurseScanIdGenerator<PurseId>: Send + Sync {
	/// A fresh stream of ids, taken each time the pass runs. It may wait before it yields, for
	/// example for metadata that names purses.
	fn purse_ids(&self) -> BoxStream<'static, PurseId>;
}

/// One pass of a purse scan.
pub struct PurseScanPass<PurseId> {
	/// Where the pass takes its purse ids from.
	pub purse_ids: Arc<dyn PurseScanIdGenerator<PurseId>>,
	/// Ends the pass once this many purses in a row that the pass scanned held nothing. `None`
	/// scans the whole stream, which must then end.
	pub stop_after_gap: Option<NonZeroU32>,
}

impl<PurseId> PurseScanPass<PurseId> {
	/// Whether `empty_run` purses in a row holding nothing end this pass.
	fn gap_reached(&self, empty_run: u32) -> bool {
		self.stop_after_gap.is_some_and(|gap| empty_run >= gap.get())
	}
}

/// What became of one purse id a pass took.
pub enum PurseScanOutcome {
	/// At least one of its keys holds something.
	Held,
	/// None of its keys holds anything.
	Empty,
	/// It was not scanned, because an earlier pass or an earlier id of this pass scanned it.
	Skipped,
}

/// Keeps what a purse scan finds.
#[async_trait::async_trait]
pub trait ScannedPursesStore<PurseId: Sync, Key: Sync, Holding: Sync>: Send + Sync {
	/// Records `purse` and the keys in `found`, all of `purse` under `installation`, keeping
	/// what is already recorded.
	async fn record_recovered_keys(
		&self,
		purse: &PurseId,
		installation: InstallationId,
		found: &[FoundKey<Key, Holding>],
	) -> Result<(), DbError>;
}

/// Scans purses under every installation, each to its gap, and keeps what it finds.
pub struct PurseScanner<Keys: ?Sized, Lookup: ?Sized, Store> {
	/// Derives each purse's keys under an installation.
	pub keys: Arc<Keys>,
	/// Reads what the keys hold.
	pub holdings: Arc<Lookup>,
	/// The installations each purse is scanned under. A purse is empty only when it is empty
	/// under all of them.
	pub installations: Vec<InstallationId>,
	/// Where what is found goes.
	pub store: Store,
	/// How each index space is read and when it ends.
	pub config: GapScanConfig,
}

impl<Keys, Lookup, Store> PurseScanner<Keys, Lookup, Store>
where
	Keys: KeyRange + ?Sized,
	Keys::PurseId: Eq + Hash + Display + Send + Sync,
	Keys::Key: Send + Sync,
	Keys::Error: Display,
	Lookup: HoldingLookup<Keys::Key> + ?Sized,
	Lookup::Holding: Send + Sync,
	Lookup::Error: Display,
	Store: ScannedPursesStore<Keys::PurseId, Keys::Key, Lookup::Holding>,
{
	/// Runs `passes` in order, each over a fresh stream of its ids.
	///
	/// A purse id already scanned in this call, by an earlier pass or earlier in the same one,
	/// is skipped, and the skip resets the current pass's run of empty purses. A pass ends when
	/// its stream ends or its gap is reached, and takes no id after that. Fails at the first
	/// purse that cannot be scanned or recorded.
	pub async fn scan_purse_passes(
		&self,
		passes: &[PurseScanPass<Keys::PurseId>],
	) -> Result<(), RecoveryError> {
		let mut scanned = HashSet::new();
		for pass in passes {
			self.run_purse_pass(pass, &mut scanned).await?;
		}
		Ok(())
	}

	/// Runs one pass, adding each purse it scans to `scanned`.
	async fn run_purse_pass(
		&self,
		pass: &PurseScanPass<Keys::PurseId>,
		scanned: &mut HashSet<Keys::PurseId>,
	) -> Result<(), RecoveryError> {
		let mut purse_ids = pass.purse_ids.purse_ids();
		let mut empty_run = 0;
		while !pass.gap_reached(empty_run) &&
			let Some(purse) = purse_ids.next().await
		{
			let outcome = self.scan_unless_scanned(scanned, purse).await?;
			empty_run = adjust_empty_purses_gap_from_outcome(empty_run, outcome);
		}
		Ok(())
	}

	/// Scans `purse` and records it in `scanned`, or skips it when `scanned` already holds it.
	async fn scan_unless_scanned(
		&self,
		scanned: &mut HashSet<Keys::PurseId>,
		purse: Keys::PurseId,
	) -> Result<PurseScanOutcome, RecoveryError> {
		if scanned.contains(&purse) {
			return Ok(PurseScanOutcome::Skipped);
		}
		let outcome = self.scan_purse(&purse).await?;
		scanned.insert(purse);
		Ok(outcome)
	}

	/// Scans `purse` under each installation from its first index, records what it holds, and
	/// says whether it held anything under any of them.
	async fn scan_purse(&self, purse: &Keys::PurseId) -> Result<PurseScanOutcome, RecoveryError> {
		let mut outcome = PurseScanOutcome::Empty;
		for installation in &self.installations {
			let found = self.find_held_keys(purse, *installation).await?;
			if found.is_empty() {
				continue;
			}
			self.store.record_recovered_keys(purse, *installation, &found).await?;
			outcome = PurseScanOutcome::Held;
		}
		Ok(outcome)
	}

	/// The keys of `purse` under `installation` that hold something.
	async fn find_held_keys(
		&self,
		purse: &Keys::PurseId,
		installation: InstallationId,
	) -> Result<FoundKeys<Keys, Lookup>, RecoveryError> {
		let found =
			scan_until_gap(&*self.keys, purse, installation, &*self.holdings, &self.config).await;
		found.map_err(|error| RecoveryError::Scan {
			purse: purse.to_string(),
			reason: error.to_string(),
		})
	}
}

/// The run of empty purses after one more purse: an empty one adds to it, and one that held
/// something or was skipped resets it.
fn adjust_empty_purses_gap_from_outcome(empty_run: u32, outcome: PurseScanOutcome) -> u32 {
	match outcome {
		PurseScanOutcome::Empty => empty_run.saturating_add(1),
		PurseScanOutcome::Held | PurseScanOutcome::Skipped => 0,
	}
}

/// Yields the given purse ids in order.
pub struct ListedPurseIds<PurseId>(pub Vec<PurseId>);

impl<PurseId: Clone + Send + Sync + 'static> PurseScanIdGenerator<PurseId>
	for ListedPurseIds<PurseId>
{
	fn purse_ids(&self) -> BoxStream<'static, PurseId> {
		futures::stream::iter(self.0.clone()).boxed()
	}
}

/// Yields the purse ids of a range of numbers in order.
pub struct SequentialPurseIds(pub Range<u32>);

impl<PurseId: From<u32> + Send + 'static> PurseScanIdGenerator<PurseId> for SequentialPurseIds {
	fn purse_ids(&self) -> BoxStream<'static, PurseId> {
		futures::stream::iter(self.0.clone().map(PurseId::from)).boxed()
	}
}

#[cfg(test)]
mod tests;
