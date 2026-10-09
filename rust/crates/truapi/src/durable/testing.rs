//! Fixtures shared by the durable engine's tests.

use core::time::Duration;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use parking_lot::{Mutex, MutexGuard};
use subxt::tx::{TransactionValid, ValidationResult};
use subxt::utils::H256;
use tempfile::TempDir;

use super::engine::{DurableDeps, DurableTxEngine};
use super::ladder::{PinnedView, SearchResult};
use super::model::{DomainId, DurableTxEntry, DurableTxId, DurableTxStatus, HeadKind};
use super::oracle::{DurableRegistry, PassScope, Unobservable};
use super::time::Timer;
use crate::chain::{
    BlockBackend, ChainHeads, DispatchOutcome, EncodedExtrinsic, HashAndNumber, HeadEvent, Heads,
    MortalExtrinsic, Mortality, TxSubmitter, TxValidator, WatchEvent,
};
use crate::chain_runtime::RuntimeFailure;
use crate::store::{Db, core_db_config};

/// Genesis hash of the chain every fixture lives on.
pub const GENESIS: H256 = H256([0xab; 32]);

/// A core database in a fresh temporary directory, so reads go through the
/// reader pool as they do in production.
pub fn open_db() -> (TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = futures::executor::block_on(Db::open(core_db_config(dir.path()))).unwrap();
    (dir, db)
}

/// Block `number`, with a hash derived from it.
pub fn block(number: u64) -> HashAndNumber {
    HashAndNumber {
        hash: block_hash(number),
        number,
    }
}

/// The hash [`block`] gives block `number`.
pub fn block_hash(number: u64) -> H256 {
    let mut hash = [0u8; 32];
    hash[..8].copy_from_slice(&number.to_be_bytes());
    H256(hash)
}

/// A distinct extrinsic, tagged `tag`, born at `birth` with `period`.
pub fn extrinsic(tag: u8, birth: u64, period: u64) -> MortalExtrinsic {
    MortalExtrinsic {
        extrinsic: EncodedExtrinsic::new(vec![4, tag]),
        mortality: Mortality::new(block(birth), period).unwrap(),
    }
}

/// A pending entry of the `test` domain, born at `birth` with `period`.
pub fn entry(id: DurableTxId, birth: u64, period: u64) -> DurableTxEntry {
    DurableTxEntry {
        id,
        domain: DomainId::new("test"),
        group: None,
        tx_hash: H256::repeat_byte(id.0 as u8),
        mortality: Mortality::new(block(birth), period).unwrap(),
        status: DurableTxStatus::Pending,
        success_detected_at: None,
    }
}

/// A domain that answers whatever the case says, for every transaction.
#[derive(Clone, Copy, Default)]
pub struct ScriptedScope {
    pub completed_at_finalized: bool,
    pub completed_at_best: bool,
    pub not_completed_at_finalized: bool,
    pub not_completed_at_best: bool,
}

impl PassScope for ScriptedScope {
    fn proven_completed(&self, _tx: &DurableTxEntry, head: HeadKind) -> bool {
        match head {
            HeadKind::Finalized => self.completed_at_finalized,
            HeadKind::Best => self.completed_at_best,
        }
    }

    fn proven_not_completed(&self, _tx: &DurableTxEntry, head: HeadKind) -> bool {
        match head {
            HeadKind::Finalized => self.not_completed_at_finalized,
            HeadKind::Best => self.not_completed_at_best,
        }
    }
}

/// A pinned view that answers every search with one result and counts them,
/// because two rules exist to avoid a search.
pub struct FakeView {
    heads: Heads,
    result: SearchResult,
    ranges: Mutex<Vec<(u64, u64)>>,
}

impl FakeView {
    /// Finalized at `finalized`, best ten blocks above it.
    pub fn new(finalized: u64, result: SearchResult) -> Self {
        Self {
            heads: Heads {
                finalized: block(finalized),
                best: block(finalized + 10),
            },
            result,
            ranges: Mutex::new(Vec::new()),
        }
    }

    /// How many searches ran.
    pub fn searches(&self) -> usize {
        self.ranges.lock().len()
    }

    /// The `(from, to)` of every search, in order.
    pub fn searched_ranges(&self) -> Vec<(u64, u64)> {
        self.ranges.lock().clone()
    }
}

#[async_trait::async_trait]
impl PinnedView for FakeView {
    fn heads(&self) -> Heads {
        self.heads
    }

    async fn search(&self, from: u64, to: u64, _tx_hash: H256) -> SearchResult {
        self.ranges.lock().push((from, to));
        self.result
    }
}

/// A linear chain answering all four chain capabilities from scripted state.
pub struct FakeChain {
    state: Mutex<ChainState>,
}

/// What a [`FakeChain`] answers, and what it was asked.
pub struct ChainState {
    pub finalized: u64,
    pub best: u64,
    /// Heights whose canonical hash is not [`block_hash`], after a reorg.
    pub reorged: HashMap<u64, H256>,
    pub bodies: HashMap<H256, Vec<H256>>,
    pub outcomes: HashMap<(H256, H256), DispatchOutcome>,
    pub unreadable_heights: HashSet<u64>,
    pub unreadable_bodies: HashSet<H256>,
    pub unreadable_outcomes: HashSet<H256>,
    pub heads_unavailable: bool,
    pub validation: Result<ValidationResult, RuntimeFailure>,
    pub submit_fails: bool,
    pub submitted: Vec<EncodedExtrinsic>,
    pub watches: VecDeque<mpsc::UnboundedReceiver<WatchEvent>>,
    pub head_streams: VecDeque<mpsc::UnboundedReceiver<Result<HeadEvent, RuntimeFailure>>>,
    pub body_reads: usize,
    pub heads_reads: usize,
}

impl FakeChain {
    /// A chain finalized at `finalized` with its best block at `best`.
    pub fn new(finalized: u64, best: u64) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ChainState {
                finalized,
                best,
                reorged: HashMap::new(),
                bodies: HashMap::new(),
                outcomes: HashMap::new(),
                unreadable_heights: HashSet::new(),
                unreadable_bodies: HashSet::new(),
                unreadable_outcomes: HashSet::new(),
                heads_unavailable: false,
                validation: Ok(ValidationResult::Valid(TransactionValid {
                    priority: 0,
                    requires: Vec::new(),
                    provides: Vec::new(),
                    longevity: 64,
                    propagate: true,
                })),
                submit_fails: false,
                submitted: Vec::new(),
                watches: VecDeque::new(),
                head_streams: VecDeque::new(),
                body_reads: 0,
                heads_reads: 0,
            }),
        })
    }

    /// The scripted state, to change or inspect.
    pub fn state(&self) -> MutexGuard<'_, ChainState> {
        self.state.lock()
    }

    /// Puts `tx_hash` in canonical block `number`, dispatched with `outcome`.
    pub fn include(&self, number: u64, tx_hash: H256, outcome: DispatchOutcome) {
        let mut state = self.state.lock();
        let hash = state.hash_at(number);
        state.bodies.entry(hash).or_default().push(tx_hash);
        state.outcomes.insert((hash, tx_hash), outcome);
    }

    /// The events the next submission reports, sent by the test.
    pub fn script_watch(&self) -> mpsc::UnboundedSender<WatchEvent> {
        let (sender, receiver) = mpsc::unbounded();
        self.state.lock().watches.push_back(receiver);
        sender
    }

    /// The events the next head subscription reports, sent by the test.
    pub fn script_heads(&self) -> mpsc::UnboundedSender<Result<HeadEvent, RuntimeFailure>> {
        let (sender, receiver) = mpsc::unbounded();
        self.state.lock().head_streams.push_back(receiver);
        sender
    }
}

impl ChainState {
    /// The canonical hash at `number`.
    pub fn hash_at(&self, number: u64) -> H256 {
        self.reorged
            .get(&number)
            .copied()
            .unwrap_or_else(|| block_hash(number))
    }

    fn number_of(&self, hash: H256) -> Option<u64> {
        let encoded = u64::from_be_bytes(hash.0[..8].try_into().unwrap());
        (encoded <= self.best && self.hash_at(encoded) == hash)
            .then_some(encoded)
            .or_else(|| {
                self.reorged
                    .iter()
                    .find(|(_, reorged)| **reorged == hash)
                    .map(|(number, _)| *number)
            })
    }

    fn head(&self, number: u64) -> HashAndNumber {
        HashAndNumber {
            hash: self.hash_at(number),
            number,
        }
    }
}

fn unavailable(method: &'static str) -> RuntimeFailure {
    RuntimeFailure::host_failure(method, "scripted failure")
}

#[async_trait::async_trait]
impl ChainHeads for FakeChain {
    async fn heads(&self, _genesis: H256) -> Result<Heads, RuntimeFailure> {
        let mut state = self.state.lock();
        state.heads_reads += 1;
        if state.heads_unavailable {
            return Err(unavailable("heads"));
        }
        Ok(Heads {
            finalized: state.head(state.finalized),
            best: state.head(state.best),
        })
    }

    async fn head_events(
        &self,
        _genesis: H256,
    ) -> Result<BoxStream<'static, Result<HeadEvent, RuntimeFailure>>, RuntimeFailure> {
        self.state
            .lock()
            .head_streams
            .pop_front()
            .map(StreamExt::boxed)
            .ok_or_else(|| unavailable("head_events"))
    }
}

#[async_trait::async_trait]
impl BlockBackend for FakeChain {
    async fn block_hash(
        &self,
        _genesis: H256,
        number: u64,
    ) -> Result<Option<H256>, RuntimeFailure> {
        let state = self.state.lock();
        if state.unreadable_heights.contains(&number) {
            return Err(unavailable("block_hash"));
        }
        Ok((number <= state.best).then(|| state.hash_at(number)))
    }

    async fn block_number(
        &self,
        _genesis: H256,
        hash: H256,
    ) -> Result<Option<u64>, RuntimeFailure> {
        Ok(self.state.lock().number_of(hash))
    }

    async fn extrinsic_hashes(
        &self,
        _genesis: H256,
        at: H256,
    ) -> Result<Option<Vec<H256>>, RuntimeFailure> {
        let mut state = self.state.lock();
        state.body_reads += 1;
        if state.unreadable_bodies.contains(&at) {
            return Err(unavailable("extrinsic_hashes"));
        }
        Ok(state
            .number_of(at)
            .map(|_| state.bodies.get(&at).cloned().unwrap_or_default()))
    }

    async fn dispatch_outcome(
        &self,
        _genesis: H256,
        at: HashAndNumber,
        extrinsic_hash: H256,
    ) -> Result<Option<DispatchOutcome>, RuntimeFailure> {
        let state = self.state.lock();
        if state.unreadable_outcomes.contains(&at.hash) {
            return Err(unavailable("dispatch_outcome"));
        }
        Ok(state.outcomes.get(&(at.hash, extrinsic_hash)).copied())
    }
}

#[async_trait::async_trait]
impl TxValidator for FakeChain {
    async fn validate(
        &self,
        _genesis: H256,
        _extrinsic: &EncodedExtrinsic,
    ) -> Result<ValidationResult, RuntimeFailure> {
        self.state.lock().validation.clone()
    }
}

#[async_trait::async_trait]
impl TxSubmitter for FakeChain {
    async fn submit_and_watch(
        &self,
        _genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<BoxStream<'static, WatchEvent>, RuntimeFailure> {
        let mut state = self.state.lock();
        if state.submit_fails {
            return Err(unavailable("submit_and_watch"));
        }
        state.submitted.push(extrinsic.clone());
        Ok(state
            .watches
            .pop_front()
            .map(StreamExt::boxed)
            .unwrap_or_else(|| futures::stream::pending().boxed()))
    }
}

/// A clock that moves only when a test advances it.
#[derive(Default)]
pub struct ManualTimer {
    state: Mutex<ManualTimerState>,
    sleeps: AtomicUsize,
}

#[derive(Default)]
struct ManualTimerState {
    now: Duration,
    sleepers: Vec<(Duration, oneshot::Sender<()>)>,
}

impl ManualTimer {
    /// Moves the clock forward, waking every sleeper whose deadline passed.
    pub fn advance(&self, by: Duration) {
        let mut state = self.state.lock();
        state.now += by;
        let now = state.now;
        let (due, waiting) = core::mem::take(&mut state.sleepers)
            .into_iter()
            .partition(|(deadline, _)| *deadline <= now);
        state.sleepers = waiting;
        drop(state);
        for (_, wake) in due {
            let _ = wake.send(());
        }
    }

    /// How many sleeps were started.
    pub fn sleeps(&self) -> usize {
        self.sleeps.load(Ordering::SeqCst)
    }
}

impl Timer for ManualTimer {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        let (wake, woken) = oneshot::channel();
        let mut state = self.state.lock();
        let deadline = state.now + duration;
        state.sleepers.push((deadline, wake));
        self.sleeps.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let _ = woken.await;
        })
    }
}

/// An engine over `chain` serving the `test` domain, decided by the search
/// alone, with time under the test's control.
pub fn test_engine(chain: &Arc<FakeChain>) -> (TempDir, Arc<DurableTxEngine>, Arc<ManualTimer>) {
    engine_with(
        chain,
        DurableRegistry::new().with_domain(DomainId::new("test"), Arc::new(Unobservable(GENESIS))),
    )
}

/// An engine over `chain` serving the domains of `registry`.
pub fn engine_with(
    chain: &Arc<FakeChain>,
    registry: DurableRegistry,
) -> (TempDir, Arc<DurableTxEngine>, Arc<ManualTimer>) {
    engine_with_spawner(chain, registry, crate::test_support::test_spawner())
}

/// [`engine_with`], running its background tasks on `spawner`.
pub fn engine_with_spawner(
    chain: &Arc<FakeChain>,
    registry: DurableRegistry,
    spawner: crate::subscription::Spawner,
) -> (TempDir, Arc<DurableTxEngine>, Arc<ManualTimer>) {
    let (dir, db) = open_db();
    let timer = Arc::new(ManualTimer::default());
    let engine = DurableTxEngine::new(DurableDeps {
        db,
        registry,
        heads: chain.clone(),
        blocks: chain.clone(),
        validator: chain.clone(),
        submitter: chain.clone(),
        timer: timer.clone(),
        spawner,
    });
    (dir, engine, timer)
}

/// Records `extrinsic` as a pending row of `domain` without watching it, as a
/// previous process would have left it.
pub fn insert(db: &Db, domain: &DomainId, extrinsic: MortalExtrinsic) -> DurableTxId {
    let domain = domain.clone();
    futures::executor::block_on(
        db.write(move |tx| super::dao::insert(tx, &domain, None, &extrinsic)),
    )
    .unwrap()
}

/// A spawner running each task on its own thread, counting the tasks that
/// finished.
pub fn counting_spawner() -> (crate::subscription::Spawner, Arc<AtomicUsize>) {
    let finished = Arc::new(AtomicUsize::new(0));
    let counter = finished.clone();
    let spawner: crate::subscription::Spawner = Arc::new(move |task| {
        let counter = counter.clone();
        std::thread::spawn(move || {
            futures::executor::block_on(task);
            counter.fetch_add(1, Ordering::SeqCst);
        });
    });
    (spawner, finished)
}
