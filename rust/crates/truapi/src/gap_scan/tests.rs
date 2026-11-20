use std::{collections::BTreeSet, sync::Mutex};

use super::*;

/// The installation every test scans under.
const INSTALLATION: InstallationId = InstallationId([0x1a; 32]);

/// Keys that are their own purse and index, as `(purse, index)`. It records each purse whose scan
/// starts.
#[derive(Default)]
struct IndexKeys {
	purses_scanned: Mutex<Vec<u32>>,
}

impl IndexKeys {
	fn purses_scanned(&self) -> Vec<u32> {
		self.purses_scanned.lock().unwrap().clone()
	}
}

impl KeyRange for IndexKeys {
	type PurseId = u32;
	type Key = (u32, u32);

	fn derive_range(
		&self,
		purse: &u32,
		_installation: InstallationId,
		indices: Range<u32>,
	) -> anyhow::Result<Vec<(u32, u32)>> {
		if indices.start == 0 {
			self.purses_scanned.lock().unwrap().push(*purse);
		}
		Ok(indices.map(|index| (*purse, index)).collect())
	}
}

/// A chain on which the keys in `held` hold something. It records every key it reads,
/// and fails a read that includes `failing`.
#[derive(Default)]
struct ScriptedChain {
	held: BTreeSet<(u32, u32)>,
	failing: Option<(u32, u32)>,
	drops_a_result: bool,
	read: Mutex<Vec<(u32, u32)>>,
}

impl ScriptedChain {
	/// Holds something at `indices` of purse 0.
	fn holding(indices: impl IntoIterator<Item = u32>) -> Self {
		Self { held: indices.into_iter().map(|index| (0, index)).collect(), ..Self::default() }
	}

	/// Holds something at the first index of each of `purses`.
	fn holding_purses(purses: impl IntoIterator<Item = u32>) -> Self {
		Self { held: purses.into_iter().map(|purse| (purse, 0)).collect(), ..Self::default() }
	}

	fn keys_read(&self) -> usize {
		self.read.lock().unwrap().len()
	}
}

#[async_trait::async_trait]
impl HoldingLookup for ScriptedChain {
	type Key = (u32, u32);
	type Holding = ();

	async fn read_holdings(&self, keys: &[(u32, u32)]) -> anyhow::Result<Vec<Option<()>>> {
		self.read.lock().unwrap().extend(keys);
		if self.failing.is_some_and(|failing| keys.contains(&failing)) {
			anyhow::bail!("node unreachable");
		}
		let mut holdings: Vec<Option<()>> =
			keys.iter().map(|key| self.held.contains(key).then_some(())).collect();
		if self.drops_a_result {
			holdings.pop();
		}
		Ok(holdings)
	}
}

/// Keeps every key recorded, as its purse and index.
#[derive(Default)]
struct RecordedKeys(Mutex<Vec<(u32, u32)>>);

#[async_trait::async_trait]
impl ScannedPursesStore<u32, (u32, u32), ()> for RecordedKeys {
	async fn record_recovered_keys(
		&self,
		_purse: &u32,
		_installation: InstallationId,
		found: &[FoundKey<(u32, u32), ()>],
	) -> Result<(), DbError> {
		self.0.lock().unwrap().extend(found.iter().map(|hit| hit.key));
		Ok(())
	}
}

fn config(gap_size: u32, batch_size: u32) -> GapScanConfig {
	GapScanConfig { gap_size, batch_size: NonZeroU32::new(batch_size).unwrap() }
}

async fn found_indices(chain: &ScriptedChain, config: GapScanConfig) -> Vec<u32> {
	let found = scan_until_gap(&IndexKeys::default(), &0, INSTALLATION, chain, &config).await;
	found.unwrap().into_iter().map(|hit| hit.index).collect()
}

/// One unfilled `create_items(1000)` call leaves exactly 1000 empty keys, so the key after
/// them must still be read, and the scan must stop right after one more empty key.
#[tokio::test]
async fn the_scan_reads_past_exactly_the_gap_and_stops_one_key_later() {
	let after_exactly_the_gap = ScriptedChain::holding([1000]);
	let after_one_more = ScriptedChain::holding([1001]);

	assert_eq!(found_indices(&after_exactly_the_gap, config(1000, 1)).await, [1000]);
	assert_eq!(found_indices(&after_one_more, config(1000, 1)).await, Vec::<u32>::new());
	assert_eq!(after_one_more.keys_read(), 1001);
}

/// Batches are read whole, so a hit inside the batch that crosses the gap is kept and
/// restarts the run.
#[tokio::test]
async fn a_hit_in_the_batch_that_crosses_the_gap_restarts_the_run() {
	let chain = ScriptedChain::holding([1400, 2300]);

	assert_eq!(found_indices(&chain, config(1000, 500)).await, [1400, 2300]);
}

/// A failed read is never an empty result, or recovery would skip what the keys hold.
#[tokio::test]
async fn a_failed_read_fails_the_scan() {
	let chain = ScriptedChain { failing: Some((0, 7)), ..ScriptedChain::holding([3]) };

	let scanned =
		scan_until_gap(&IndexKeys::default(), &0, INSTALLATION, &chain, &config(10, 5)).await;

	assert!(matches!(scanned, Err(GapScanError::Lookup(_))));
}

/// Results that do not line up with the keys would put holdings on the wrong keys.
#[tokio::test]
async fn a_read_with_a_missing_result_fails_the_scan() {
	let chain = ScriptedChain { drops_a_result: true, ..ScriptedChain::holding([3]) };

	let scanned =
		scan_until_gap(&IndexKeys::default(), &0, INSTALLATION, &chain, &config(10, 5)).await;

	assert!(matches!(scanned, Err(GapScanError::ResultCountMismatch { keys: 5, results: 4 })));
}

/// A scanner over `chain` that ends each purse at its first empty key.
fn scanner(chain: ScriptedChain) -> PurseScanner<IndexKeys, ScriptedChain, RecordedKeys> {
	PurseScanner {
		keys: Arc::default(),
		holdings: Arc::new(chain),
		installations: vec![INSTALLATION],
		store: RecordedKeys::default(),
		config: config(0, 1),
	}
}

fn listed(purses: impl IntoIterator<Item = u32>) -> PurseScanPass<u32> {
	let purses = ListedPurseIds(purses.into_iter().collect());
	PurseScanPass { purse_ids: Arc::new(purses), stop_after_gap: None }
}

fn sequential_with_gap(stop_after_gap: u32) -> PurseScanPass<u32> {
	PurseScanPass {
		purse_ids: Arc::new(SequentialPurseIds(0..u32::MAX)),
		stop_after_gap: NonZeroU32::new(stop_after_gap),
	}
}

/// The gap counts only purses the pass itself scanned, and a skipped purse resets it, so a
/// purse known from an earlier pass never shortens the walk.
#[tokio::test]
async fn a_skipped_purse_resets_the_run_of_empty_purses() {
	let with_a_known_purse = scanner(ScriptedChain::holding_purses([5]));
	let without = scanner(ScriptedChain::holding_purses([3]));

	with_a_known_purse
		.scan_purse_passes(&[listed([2]), sequential_with_gap(3)])
		.await
		.unwrap();
	without.scan_purse_passes(&[sequential_with_gap(3)]).await.unwrap();

	assert_eq!(with_a_known_purse.keys.purses_scanned(), [2, 0, 1, 3, 4, 5, 6, 7, 8]);
	assert_eq!(without.keys.purses_scanned(), [0, 1, 2]);
}

/// What a purse holds is kept as soon as the purse is scanned.
#[tokio::test]
async fn the_keys_of_a_held_purse_are_recorded() {
	let purses = scanner(ScriptedChain::holding_purses([1, 4]));

	purses.scan_purse_passes(&[listed([0, 1, 2, 4])]).await.unwrap();

	assert_eq!(*purses.store.0.lock().unwrap(), [(1, 0), (4, 0)]);
}

/// Purses named up front must all be scanned, each only once.
#[tokio::test]
async fn a_pass_without_a_gap_scans_its_whole_stream_and_skips_scanned_purses() {
	let purses = scanner(ScriptedChain::default());

	purses.scan_purse_passes(&[listed([7, 8, 9]), listed([8, 10])]).await.unwrap();

	assert_eq!(purses.keys.purses_scanned(), [7, 8, 9, 10]);
}

/// A pass that reached its gap must not wait on its generator for an id it will not scan.
#[tokio::test]
async fn a_pass_takes_no_id_after_its_gap() {
	let purses = scanner(ScriptedChain::default());
	let ids_taken = Arc::new(Mutex::new(0));
	let counted = CountedPurseIds(ids_taken.clone());
	let pass = PurseScanPass { purse_ids: Arc::new(counted), stop_after_gap: NonZeroU32::new(2) };

	purses.scan_purse_passes(&[pass]).await.unwrap();

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
	let chain = ScriptedChain { failing: Some((1, 0)), ..ScriptedChain::default() };
	let purses = scanner(chain);

	let scanned = purses.scan_purse_passes(&[listed([0, 1, 2]), listed([3])]).await;

	let Err(PurseScanError::Scan { purse, .. }) = scanned else {
		panic!("the scan did not fail on purse 1: {scanned:?}");
	};
	assert_eq!(purse, "1");
	assert_eq!(purses.keys.purses_scanned(), [0, 1]);
}
