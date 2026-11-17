use std::{collections::BTreeSet, sync::Mutex};

use super::*;

/// Keys that are their own index.
struct IndexKeys;

impl KeyRange for IndexKeys {
	type Key = u32;
	type Error = String;

	fn derive_range(&self, indices: Range<u32>) -> Result<Vec<u32>, String> {
		Ok(indices.collect())
	}
}

/// A chain on which the keys in `held` hold something. It records every key it reads,
/// and fails a read that includes `failing`.
#[derive(Default)]
struct ScriptedChain {
	held: BTreeSet<u32>,
	failing: Option<u32>,
	drops_a_result: bool,
	read: Mutex<Vec<u32>>,
}

impl ScriptedChain {
	fn holding(held: impl IntoIterator<Item = u32>) -> Self {
		Self { held: held.into_iter().collect(), ..Self::default() }
	}

	fn keys_read(&self) -> usize {
		self.read.lock().unwrap().len()
	}
}

#[async_trait::async_trait]
impl HoldingLookup<u32> for ScriptedChain {
	type Holding = ();
	type Error = String;

	async fn read_holdings(&self, keys: &[u32]) -> Result<Vec<Option<()>>, String> {
		self.read.lock().unwrap().extend(keys);
		if self.failing.is_some_and(|failing| keys.contains(&failing)) {
			return Err("node unreachable".to_string());
		}
		let mut holdings: Vec<Option<()>> =
			keys.iter().map(|key| self.held.contains(key).then_some(())).collect();
		if self.drops_a_result {
			holdings.pop();
		}
		Ok(holdings)
	}
}

fn limit(max_empty_run: u32, batch_size: u32) -> GapLimit {
	GapLimit { max_empty_run, batch_size: NonZeroU32::new(batch_size).unwrap() }
}

async fn found_indices(chain: &ScriptedChain, limit: GapLimit) -> Vec<u32> {
	let found = scan_until_gap(&IndexKeys, chain, limit).await.unwrap();
	found.into_iter().map(|hit| hit.index).collect()
}

/// One unfilled `create_items(1000)` call leaves exactly 1000 empty keys, so the key after
/// them must still be read, and the scan must stop right after one more empty key.
#[tokio::test]
async fn the_scan_reads_past_exactly_the_limit_and_stops_one_key_later() {
	let after_exactly_the_limit = ScriptedChain::holding([1000]);
	let after_one_more = ScriptedChain::holding([1001]);

	assert_eq!(found_indices(&after_exactly_the_limit, limit(1000, 1)).await, [1000]);
	assert_eq!(found_indices(&after_one_more, limit(1000, 1)).await, Vec::<u32>::new());
	assert_eq!(after_one_more.keys_read(), 1001);
}

/// Batches are read whole, so a hit inside the batch that crosses the limit is kept and
/// restarts the run.
#[tokio::test]
async fn a_hit_in_the_batch_that_crosses_the_limit_restarts_the_run() {
	let chain = ScriptedChain::holding([1400, 2300]);

	assert_eq!(found_indices(&chain, limit(1000, 500)).await, [1400, 2300]);
}

/// A failed read is never an empty result, or recovery would skip what the keys hold.
#[tokio::test]
async fn a_failed_read_fails_the_scan() {
	let chain = ScriptedChain { failing: Some(7), ..ScriptedChain::holding([3]) };

	let scanned = scan_until_gap(&IndexKeys, &chain, limit(10, 5)).await;

	assert!(matches!(scanned, Err(GapScanError::Lookup(_))));
}

/// Results that do not line up with the keys would put holdings on the wrong keys.
#[tokio::test]
async fn a_read_with_a_missing_result_fails_the_scan() {
	let chain = ScriptedChain { drops_a_result: true, ..ScriptedChain::holding([3]) };

	let scanned = scan_until_gap(&IndexKeys, &chain, limit(10, 5)).await;

	assert!(matches!(scanned, Err(GapScanError::ResultCountMismatch { keys: 5, results: 4 })));
}

/// A derivation that has no seed behind it any more must fail rather than look empty.
#[test]
fn hard_path_keys_fail_once_their_derivation_is_dropped() {
	let keys: HardPathKeys<[u8; 32]> = HardPathKeys::new(Weak::<NoDerivation>::new(), Vec::new());

	assert!(matches!(keys.derive_range(0..1), Err(AuthorityError::Unavailable { .. })));
}

struct NoDerivation;

impl HardKeyDerivation for NoDerivation {
	fn public_keys_under(
		&self,
		_prefix: &[String],
		_indices: Range<u32>,
	) -> Result<Vec<[u8; 32]>, AuthorityError> {
		unreachable!("the derivation is never alive")
	}
}

/// Purses whose index spaces hold something, recording each purse scanned, and failing the
/// scan of `failing`.
#[derive(Default)]
struct ScriptedPurses {
	held: BTreeSet<u32>,
	failing: Option<u32>,
	scanned: Mutex<Vec<u32>>,
}

impl ScriptedPurses {
	fn holding(held: impl IntoIterator<Item = u32>) -> Self {
		Self { held: held.into_iter().collect(), ..Self::default() }
	}

	fn scanned(&self) -> Vec<u32> {
		self.scanned.lock().unwrap().clone()
	}
}

#[async_trait::async_trait]
impl PurseScanner<u32> for ScriptedPurses {
	type Error = String;

	async fn scan_purse(&self, purse: u32) -> Result<PurseScanOutcome, String> {
		self.scanned.lock().unwrap().push(purse);
		if self.failing == Some(purse) {
			return Err(format!("purse {purse} could not be read"));
		}
		Ok(if self.held.contains(&purse) { PurseScanOutcome::Held } else { PurseScanOutcome::Empty })
	}
}

fn listed(purses: impl IntoIterator<Item = u32>) -> PurseScanPass<u32> {
	let purses = ListedPurseIds(purses.into_iter().collect());
	PurseScanPass { purse_ids: Arc::new(purses), stop_after_gap: None }
}

fn sequential(stop_after_gap: u32) -> PurseScanPass<u32> {
	PurseScanPass {
		purse_ids: Arc::new(SequentialPurseIds(0..u32::MAX)),
		stop_after_gap: NonZeroU32::new(stop_after_gap),
	}
}

/// The gap counts only purses the pass itself scanned, and a skipped purse resets it, so a
/// purse known from an earlier pass never shortens the walk.
#[tokio::test]
async fn a_skipped_purse_resets_the_run_of_empty_purses() {
	let with_a_known_purse = ScriptedPurses::holding([5]);
	let without = ScriptedPurses::holding([3]);

	scan_purse_passes(&[listed([2]), sequential(3)], &with_a_known_purse).await.unwrap();
	scan_purse_passes(&[sequential(3)], &without).await.unwrap();

	assert_eq!(with_a_known_purse.scanned(), [2, 0, 1, 3, 4, 5, 6, 7, 8]);
	assert_eq!(without.scanned(), [0, 1, 2]);
}

/// Purses named up front must all be scanned, each only once.
#[tokio::test]
async fn a_pass_without_a_gap_scans_its_whole_stream_and_skips_scanned_purses() {
	let purses = ScriptedPurses::default();

	scan_purse_passes(&[listed([7, 8, 9]), listed([8, 10])], &purses).await.unwrap();

	assert_eq!(purses.scanned(), [7, 8, 9, 10]);
}

/// A pass that reached its gap must not wait on its generator for an id it will not scan.
#[tokio::test]
async fn a_pass_takes_no_id_after_its_gap() {
	let purses = ScriptedPurses::default();
	let ids_taken = Arc::new(Mutex::new(0));
	let counted = CountedPurseIds(ids_taken.clone());
	let pass = PurseScanPass { purse_ids: Arc::new(counted), stop_after_gap: NonZeroU32::new(2) };

	scan_purse_passes(&[pass], &purses).await.unwrap();

	assert_eq!(*ids_taken.lock().unwrap(), 2);
}

/// Yields 0, 1, 2, … and counts the ids taken.
struct CountedPurseIds(Arc<Mutex<u32>>);

impl PurseScanIdGenerator<u32> for CountedPurseIds {
	fn purse_ids(&self) -> BoxStream<'static, u32> {
		let ids_taken = self.0.clone();
		futures::stream::iter(0..)
			.inspect(move |_| *ids_taken.lock().unwrap() += 1)
			.boxed()
	}
}

/// A purse that could not be read is never treated as empty, so the scan stops there.
#[tokio::test]
async fn a_failed_purse_ends_every_pass() {
	let purses = ScriptedPurses { failing: Some(1), ..ScriptedPurses::default() };

	let scanned = scan_purse_passes(&[listed([0, 1, 2]), listed([3])], &purses).await;

	assert_eq!(scanned, Err("purse 1 could not be read".to_string()));
	assert_eq!(purses.scanned(), [0, 1]);
}
