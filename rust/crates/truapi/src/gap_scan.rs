//! Finds what a seed holds under keys it derives in order, by reading them until a run of
//! empty ones.
//!
//! [`scan_until_gap`] scans one index space: it derives keys in batches through a [`KeyRange`],
//! reads what they hold through a [`HoldingLookup`], and stops after a run of empty keys.
//! [`scan_purse_passes`] scans many index spaces, one per purse, in a list of
//! [`PurseScanPass`]es: each pass takes purse ids from its own [`PurseScanIdGenerator`] and may
//! stop after a run of empty purses, and a purse an earlier pass scanned is skipped.
//! [`HardPathKeys`] is the [`KeyRange`] of sr25519 keys under a hard derivation path.

use core::{marker::PhantomData, num::NonZeroU32, ops::Range};
use std::{
	collections::HashSet,
	hash::Hash,
	sync::{Arc, Weak},
};

use futures::{StreamExt, stream::BoxStream};

use crate::runtime::AuthorityError;

/// Derives the keys of an index space for a range of indices.
pub trait KeyRange: Send + Sync {
	/// The key one index derives.
	type Key;
	/// Why a range could not be derived.
	type Error;

	/// The keys at `indices`, one per index, in index order.
	fn derive_range(&self, indices: Range<u32>) -> Result<Vec<Self::Key>, Self::Error>;
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

/// When a scan of one index space ends and how many keys it reads at once.
pub struct GapLimit {
	/// The scan ends once more than this many keys in a row have held nothing.
	pub max_empty_run: u32,
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

/// Scans one index space from index 0 in whole batches and returns every key that holds
/// something, in index order.
///
/// Before each batch it stops if more than `limit.max_empty_run` keys in a row have held
/// nothing, so at least that many plus one empty keys follow the last key found. A batch is
/// always read whole, and what it finds past the limit is kept. Fails at the first batch that
/// cannot be derived or read.
pub async fn scan_until_gap<Keys, Lookup>(
	keys: &Keys,
	lookup: &Lookup,
	limit: GapLimit,
) -> Result<FoundKeys<Keys, Lookup>, ScanError<Keys, Lookup>>
where
	Keys: KeyRange + ?Sized,
	Keys::Key: Sync,
	Lookup: HoldingLookup<Keys::Key> + ?Sized,
{
	let mut found = Vec::new();
	let mut position = ScanPosition { next_index: 0, empty_run: 0 };
	while let Some(indices) = position.next_batch(&limit) {
		let hits = read_batch(keys, lookup, indices.clone()).await?;
		position = position.after_batch(indices, hits.last().map(|hit| hit.index));
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
	/// The indices of the next batch, or `None` once more than `limit.max_empty_run` keys in a
	/// row have held nothing or no index is left.
	fn next_batch(&self, limit: &GapLimit) -> Option<Range<u32>> {
		if self.empty_run > limit.max_empty_run || self.next_index == u32::MAX {
			return None;
		}
		let end = self.next_index.saturating_add(limit.batch_size.get());
		Some(self.next_index..end)
	}

	/// The position after reading `indices`, in which `last_hit` is the highest index that held
	/// something, if any did.
	fn after_batch(&self, indices: Range<u32>, last_hit: Option<u32>) -> Self {
		let empty_run = match last_hit {
			Some(hit) => indices.end - hit - 1,
			None => self.empty_run.saturating_add(indices.end - indices.start),
		};
		Self { next_index: indices.end, empty_run }
	}
}

/// Derives and reads the keys at `indices`, and returns those that hold something.
async fn read_batch<Keys, Lookup>(
	keys: &Keys,
	lookup: &Lookup,
	indices: Range<u32>,
) -> Result<FoundKeys<Keys, Lookup>, ScanError<Keys, Lookup>>
where
	Keys: KeyRange + ?Sized,
	Keys::Key: Sync,
	Lookup: HoldingLookup<Keys::Key> + ?Sized,
{
	let derived = keys.derive_range(indices.clone()).map_err(GapScanError::Derive)?;
	let holdings = lookup.read_holdings(&derived).await.map_err(GapScanError::Lookup)?;
	if holdings.len() != derived.len() {
		return Err(GapScanError::ResultCountMismatch {
			keys: derived.len(),
			results: holdings.len(),
		});
	}
	Ok(indices
		.zip(derived)
		.zip(holdings)
		.filter_map(|((index, key), holding)| Some(FoundKey { index, key, holding: holding? }))
		.collect())
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

/// Whether a scanned purse held anything.
pub enum PurseScanOutcome {
	/// At least one of its keys holds something.
	Held,
	/// None of its keys holds anything.
	Empty,
}

/// Scans one purse's index spaces to their gap and keeps what it finds.
#[async_trait::async_trait]
pub trait PurseScanner<PurseId: Send>: Send + Sync {
	/// Why a purse could not be scanned.
	type Error;

	/// Scans `purse` from its first index and says whether it held anything.
	async fn scan_purse(&self, purse: PurseId) -> Result<PurseScanOutcome, Self::Error>;
}

/// Runs `passes` in order, each over a fresh stream of its ids.
///
/// A purse id already scanned in this call, by an earlier pass or earlier in the same one, is
/// skipped, and the skip resets the current pass's run of empty purses. A pass ends when its
/// stream ends or its gap is reached, and takes no id after that. Fails at the first purse that
/// cannot be scanned.
pub async fn scan_purse_passes<PurseId, Scanner>(
	passes: &[PurseScanPass<PurseId>],
	scanner: &Scanner,
) -> Result<(), Scanner::Error>
where
	PurseId: Eq + Hash + Clone + Send + Sync,
	Scanner: PurseScanner<PurseId>,
{
	let mut scanned = HashSet::new();
	for pass in passes {
		run_purse_pass(pass, scanner, &mut scanned).await?;
	}
	Ok(())
}

/// Runs one pass, adding each purse it scans to `scanned`.
async fn run_purse_pass<PurseId, Scanner>(
	pass: &PurseScanPass<PurseId>,
	scanner: &Scanner,
	scanned: &mut HashSet<PurseId>,
) -> Result<(), Scanner::Error>
where
	PurseId: Eq + Hash + Clone + Send + Sync,
	Scanner: PurseScanner<PurseId>,
{
	let mut purse_ids = pass.purse_ids.purse_ids();
	let mut empty_run = 0;
	while !pass.gap_reached(empty_run) &&
		let Some(purse) = purse_ids.next().await
	{
		let outcome = scan_unless_scanned(scanner, scanned, purse).await?;
		empty_run = empty_run_after(empty_run, outcome);
	}
	Ok(())
}

/// Scans `purse` and records it in `scanned`, or returns `None` without scanning when
/// `scanned` already holds it.
async fn scan_unless_scanned<PurseId, Scanner>(
	scanner: &Scanner,
	scanned: &mut HashSet<PurseId>,
	purse: PurseId,
) -> Result<Option<PurseScanOutcome>, Scanner::Error>
where
	PurseId: Eq + Hash + Clone + Send + Sync,
	Scanner: PurseScanner<PurseId>,
{
	if !scanned.insert(purse.clone()) {
		return Ok(None);
	}
	scanner.scan_purse(purse).await.map(Some)
}

/// The run of empty purses after one more purse: an empty one adds to it, and one that held
/// something or was skipped resets it.
fn empty_run_after(empty_run: u32, outcome: Option<PurseScanOutcome>) -> u32 {
	match outcome {
		Some(PurseScanOutcome::Empty) => empty_run.saturating_add(1),
		Some(PurseScanOutcome::Held) | None => 0,
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

/// Derives public keys under hard paths from the session's seed, without exposing it.
pub trait HardKeyDerivation: Send + Sync {
	/// The sr25519 public key at `prefix//index` for each of `indices`, in order, with every
	/// junction hard. Fails without a session.
	fn public_keys_under(
		&self,
		prefix: &[String],
		indices: Range<u32>,
	) -> Result<Vec<[u8; 32]>, AuthorityError>;
}

/// The sr25519 public keys `prefix//index`, every junction hard, as the keys of an index space.
pub struct HardPathKeys<Key> {
	derivation: Weak<dyn HardKeyDerivation>,
	prefix: Vec<String>,
	key: PhantomData<fn() -> Key>,
}

impl<Key> HardPathKeys<Key> {
	/// The keys under `prefix`, derived by `derivation` while it is alive.
	pub fn new(derivation: Weak<dyn HardKeyDerivation>, prefix: Vec<String>) -> Self {
		Self { derivation, prefix, key: PhantomData }
	}
}

/// Fails with [`AuthorityError::Unavailable`] once the derivation is dropped.
impl<Key: From<[u8; 32]>> KeyRange for HardPathKeys<Key> {
	type Key = Key;
	type Error = AuthorityError;

	fn derive_range(&self, indices: Range<u32>) -> Result<Vec<Key>, AuthorityError> {
		let Some(derivation) = self.derivation.upgrade() else {
			let reason = "the hard key derivation was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		let keys = derivation.public_keys_under(&self.prefix, indices)?;
		Ok(keys.into_iter().map(Key::from).collect())
	}
}

#[cfg(test)]
mod tests;
