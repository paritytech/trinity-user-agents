//! [`BucketRun`]: one task building the waiting rows of one bucket, round
//! after round, until none is left.

use core::time::Duration;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Weak};

use futures::future::AbortHandle;
use subxt::utils::H256;
use tracing::{error, warn};

use super::{SubmissionBucket, SubmissionExecutor};
use crate::durable::dao;
use crate::durable::engine::DurableTxEngine;
use crate::durable::model::{DurableTxId, ScheduledDurableTx};
use crate::durable::policy::{Preparation, SubmissionPolicy};
use crate::durable::time::Timer;
use crate::store::{Db, DbError};

/// The wait after the first round that decided nothing.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
/// The longest wait between rounds that decide nothing.
pub const MAX_BACKOFF: Duration = Duration::from_secs(120);
/// The wait before building a row again after its first attempt.
pub const REBUILD_COOLDOWN: Duration = Duration::from_secs(5);
/// The longest wait before building a row again.
pub const MAX_REBUILD_COOLDOWN: Duration = Duration::from_secs(600);

/// One bucket's task. Holds the engine weakly, so a policy that never
/// answers cannot keep a dropped engine alive.
pub struct BucketRun {
    engine: Weak<DurableTxEngine>,
    executor: Arc<SubmissionExecutor>,
    db: Db,
    timer: Arc<dyn Timer>,
    bucket: SubmissionBucket,
    abort: AbortHandle,
}

/// What builds a bucket's rows: its policy, and the chain its domain lives
/// on.
struct Builder {
    policy: Arc<dyn SubmissionPolicy>,
    genesis: H256,
}

impl BucketRun {
    /// The task for `bucket`, stopped through `abort`.
    pub fn new(
        engine: &Arc<DurableTxEngine>,
        bucket: SubmissionBucket,
        abort: AbortHandle,
    ) -> Self {
        Self {
            engine: Arc::downgrade(engine),
            executor: engine.executor.clone(),
            db: engine.db.clone(),
            timer: engine.timer.clone(),
            bucket,
            abort,
        }
    }

    /// Runs rounds until the bucket is drained, backing off after every round
    /// that decided nothing.
    pub async fn run(self) {
        let mut fruitless_rounds = 0;
        while self.engine.strong_count() > 0 {
            if self.round().await {
                fruitless_rounds = 0;
            } else {
                fruitless_rounds += 1;
                self.timer.sleep(backoff(fruitless_rounds)).await;
            }
            if self.finish().await {
                return;
            }
        }
    }

    /// Asks the policy about every waiting row once and applies its answers.
    /// Returns whether any row was decided.
    async fn round(&self) -> bool {
        let rows = match self.waiting().await {
            Ok(rows) => rows,
            Err(error) => {
                warn!(%error, "durable submission bucket could not read its rows");
                return false;
            }
        };
        if rows.is_empty() {
            return true;
        }
        let Some(engine) = self.engine.upgrade() else {
            return false;
        };
        // The engine is held only while this round decides, never across a
        // policy call that may not return.
        let builder = builder_of(&engine, &self.bucket);
        match builder {
            Some(builder) => {
                drop(engine);
                self.build(&builder, rows).await
            }
            None => abandon_all(&engine, &self.bucket, &rows).await,
        }
    }

    /// Builds `rows` with `builder` after their cooldown.
    async fn build(&self, builder: &Builder, rows: Vec<ScheduledDurableTx>) -> bool {
        self.cool_down(&rows).await;
        match builder.policy.prepare(&rows).await {
            Ok(answers) => self.apply_all(builder.genesis, rows, answers).await,
            Err(error) => {
                warn!(policy = self.bucket.policy.as_str(), %error, "durable submission not prepared");
                false
            }
        }
    }

    /// Applies the answer for each row of `rows`. Answers about other rows
    /// are ignored. Returns whether any row was decided.
    async fn apply_all(
        &self,
        genesis: H256,
        rows: Vec<ScheduledDurableTx>,
        mut answers: HashMap<DurableTxId, Preparation>,
    ) -> bool {
        let mut decided = false;
        for row in rows {
            if let Some(preparation) = answers.remove(&row.id) {
                decided |= self.apply(genesis, row.id, preparation).await;
            }
        }
        decided
    }

    /// Starts the attempt a ready row makes, or fails a row its policy gave
    /// up on. Returns whether the ledger changed.
    async fn apply(&self, genesis: H256, id: DurableTxId, preparation: Preparation) -> bool {
        let Some(engine) = self.engine.upgrade() else {
            return false;
        };
        match preparation {
            Preparation::Ready(extrinsic) => {
                let started = engine.start_built_attempt(id, genesis, extrinsic).await;
                if started {
                    *self.executor.attempts_started.lock().entry(id).or_default() += 1;
                }
                started
            }
            Preparation::GiveUp => engine.give_up_submission(id).await,
        }
    }

    /// Waits out the cooldown of the most rebuilt row of `rows`.
    async fn cool_down(&self, rows: &[ScheduledDurableTx]) {
        let wait = cooldown(self.most_attempts(rows));
        if !wait.is_zero() {
            self.timer.sleep(wait).await;
        }
    }

    /// The most attempts any row of `rows` was built into.
    fn most_attempts(&self, rows: &[ScheduledDurableTx]) -> u32 {
        let started = self.executor.attempts_started.lock();
        let attempts = rows.iter().filter_map(|row| started.get(&row.id)).max();
        attempts.copied().unwrap_or(0)
    }

    /// Whether to stop: true once no row is left. A row scheduled while the
    /// task leaves is caught by the second read, and the task stays unless a
    /// new one already took over.
    async fn finish(&self) -> bool {
        if !self.drained().await {
            return false;
        }
        self.executor.buckets.lock().remove(&self.bucket);
        if self.drained().await {
            return true;
        }
        match self.executor.buckets.lock().entry(self.bucket.clone()) {
            Entry::Vacant(vacant) => {
                vacant.insert(self.abort.clone());
                false
            }
            Entry::Occupied(_) => true,
        }
    }

    /// Whether no row is waiting. An unreadable ledger counts as not drained.
    async fn drained(&self) -> bool {
        matches!(self.waiting().await, Ok(rows) if rows.is_empty())
    }

    async fn waiting(&self) -> Result<Vec<ScheduledDurableTx>, DbError> {
        let bucket = self.bucket.clone();
        self.db
            .read(move |conn| {
                dao::bucket_submissions(conn, &bucket.domain, &bucket.policy, bucket.group.as_ref())
            })
            .await
    }
}

/// The policy and chain that build `bucket`, when both are still registered.
fn builder_of(engine: &DurableTxEngine, bucket: &SubmissionBucket) -> Option<Builder> {
    let genesis = engine.chain_of(&bucket.domain)?;
    let policy = engine.registry.policy(&bucket.policy)?.clone();
    Some(Builder { policy, genesis })
}

/// Fails every row of a bucket whose policy or domain is no longer
/// registered: nothing could ever build them.
async fn abandon_all(
    engine: &DurableTxEngine,
    bucket: &SubmissionBucket,
    rows: &[ScheduledDurableTx],
) -> bool {
    error!(
        domain = bucket.domain.as_str(),
        policy = bucket.policy.as_str(),
        "durable submissions name a policy or domain that is not registered"
    );
    let mut decided = false;
    for row in rows {
        decided |= engine.give_up_submission(row.id).await;
    }
    decided
}

/// The wait after `rounds` rounds in a row that decided nothing.
pub fn backoff(rounds: u32) -> Duration {
    doubled(INITIAL_BACKOFF, rounds.saturating_sub(1)).min(MAX_BACKOFF)
}

/// The wait before building a row that was already built into `attempts`
/// attempts.
pub fn cooldown(attempts: u32) -> Duration {
    match attempts {
        0 => Duration::ZERO,
        _ => doubled(REBUILD_COOLDOWN, attempts - 1).min(MAX_REBUILD_COOLDOWN),
    }
}

/// `base` doubled `times` times, at most ten.
fn doubled(base: Duration, times: u32) -> Duration {
    base * (1 << times.min(10))
}
