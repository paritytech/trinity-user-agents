//! [`BucketRun`]: one task building the waiting rows of one bucket, round
//! after round, until none is left.

use std::{
	collections::{HashMap, hash_map::Entry},
	sync::{Arc, Weak},
};

use futures::future::{AbortHandle, join_all};
use subxt::utils::H256;
use tracing::{error, warn};

use super::{
	SubmissionBucket, SubmissionExecutor,
	attempt::{LedgerChange, give_up_submission, start_built_attempt},
	backoff::{backoff, cooldown},
};
use crate::{
	durable::{
		dao,
		engine::DurableTxEngine,
		model::{DurableTxId, ScheduledDurableTx},
		policy::{Preparation, SubmissionPolicy},
	},
	store::{Db, DbError},
	time::Timer,
};

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
struct ResolvedPolicy {
	policy: Arc<dyn SubmissionPolicy>,
	genesis: H256,
}

/// What a round, or one row of it, changed in the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoundOutcome {
	/// A row was decided, so the next round runs at once.
	Progress,
	/// Nothing was decided, so the next round backs off.
	NoProgress,
}

/// Whether a bucket task ends after a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BucketExit {
	/// No row is left, or another task took the bucket over.
	Leave,
	/// Rows still wait: another round runs.
	Stay,
}

impl From<LedgerChange> for RoundOutcome {
	/// [`Self::Progress`] when the ledger changed.
	fn from(change: LedgerChange) -> Self {
		match change {
			LedgerChange::Changed => Self::Progress,
			LedgerChange::Unchanged => Self::NoProgress,
		}
	}
}

impl RoundOutcome {
	/// [`Self::Progress`] when either `self` or `other` is.
	fn or(self, other: Self) -> Self {
		if self == Self::Progress { self } else { other }
	}
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
	pub async fn run_until_drained(self) {
		let mut rounds_without_progress = 0;
		while self.engine.strong_count() > 0 {
			match self.decide_round().await {
				RoundOutcome::Progress => rounds_without_progress = 0,
				RoundOutcome::NoProgress => {
					rounds_without_progress += 1;
					self.timer.sleep(backoff(rounds_without_progress)).await;
				},
			}
			if self.leave_if_drained().await == BucketExit::Leave {
				return;
			}
		}
	}

	/// Asks the policy about every waiting row once and applies its answers.
	/// An empty bucket needs no backoff: its task leaves next.
	async fn decide_round(&self) -> RoundOutcome {
		let Some(rows) = self.readable_rows().await else {
			return RoundOutcome::NoProgress;
		};
		if rows.is_empty() {
			return RoundOutcome::Progress;
		}
		let Some(engine) = self.engine.upgrade() else {
			return RoundOutcome::NoProgress;
		};
		let Some(resolved) = resolve_policy(&engine, &self.bucket) else {
			return abandon_unbuildable(&engine, &self.bucket, &rows).await;
		};
		// The engine is held only while this round decides, never across a
		// policy call that may not return.
		drop(engine);
		self.prepare_and_apply(&resolved, rows).await
	}

	/// The bucket's waiting rows, or `None` when the ledger cannot be read.
	async fn readable_rows(&self) -> Option<Vec<ScheduledDurableTx>> {
		match self.read_bucket_rows().await {
			Ok(rows) => Some(rows),
			Err(error) => {
				warn!(bucket = %self.bucket, %error, "durable submission bucket could not read its rows");
				None
			},
		}
	}

	/// Builds `rows` with `resolved` after their cooldown.
	async fn prepare_and_apply(
		&self,
		resolved: &ResolvedPolicy,
		rows: Vec<ScheduledDurableTx>,
	) -> RoundOutcome {
		self.cool_down(&rows).await;
		let prepared = resolved.policy.prepare(&rows).await;
		match prepared {
			Ok(answers) => self.apply_all(resolved.genesis, rows, answers).await,
			Err(error) => {
				warn!(bucket = %self.bucket, rows = rows.len(), %error, "durable submissions not prepared");
				RoundOutcome::NoProgress
			},
		}
	}

	/// Applies the answer for each row of `rows`. Answers about other rows
	/// are ignored.
	async fn apply_all(
		&self,
		genesis: H256,
		rows: Vec<ScheduledDurableTx>,
		mut answers: HashMap<DurableTxId, Preparation>,
	) -> RoundOutcome {
		let mut outcome = RoundOutcome::NoProgress;
		for row in rows {
			if let Some(preparation) = answers.remove(&row.id) {
				let applied = self.apply_preparation(genesis, row.id, preparation).await;
				outcome = outcome.or(applied);
			}
		}
		outcome
	}

	/// Starts the attempt a ready row makes, or fails a row its policy gave
	/// up on.
	async fn apply_preparation(
		&self,
		genesis: H256,
		id: DurableTxId,
		preparation: Preparation,
	) -> RoundOutcome {
		let Some(engine) = self.engine.upgrade() else {
			return RoundOutcome::NoProgress;
		};
		let change = match preparation {
			Preparation::Ready { extrinsic, params } => {
				start_built_attempt(&engine, id, genesis, extrinsic, params).await
			},
			Preparation::GiveUp => give_up_submission(&engine, id).await,
		};
		RoundOutcome::from(change)
	}

	/// Waits out the cooldown of the most rebuilt row of `rows`.
	async fn cool_down(&self, rows: &[ScheduledDurableTx]) {
		let attempts = rows.iter().map(|row| row.attempts).max().unwrap_or(0);
		self.timer.sleep(cooldown(attempts)).await;
	}

	/// Leaves once no row is left. A row scheduled while the task leaves is
	/// caught by the second read, and the task stays unless a new one
	/// already took over.
	async fn leave_if_drained(&self) -> BucketExit {
		if !self.bucket_is_drained().await {
			return BucketExit::Stay;
		}
		self.executor.buckets.lock().remove(&self.bucket);
		if self.bucket_is_drained().await {
			return BucketExit::Leave;
		}
		match self.executor.buckets.lock().entry(self.bucket.clone()) {
			Entry::Vacant(vacant) => {
				vacant.insert(self.abort.clone());
				BucketExit::Stay
			},
			Entry::Occupied(_) => BucketExit::Leave,
		}
	}

	/// Whether no row is waiting. An unreadable ledger counts as not drained.
	async fn bucket_is_drained(&self) -> bool {
		matches!(self.read_bucket_rows().await, Ok(rows) if rows.is_empty())
	}

	async fn read_bucket_rows(&self) -> Result<Vec<ScheduledDurableTx>, DbError> {
		let bucket = self.bucket.clone();
		self.db
			.read(move |conn| {
				dao::bucket_submissions(conn, &bucket.domain, &bucket.policy, bucket.group.as_ref())
			})
			.await
	}
}

/// The policy and chain that build `bucket`, when both are still registered.
fn resolve_policy(engine: &DurableTxEngine, bucket: &SubmissionBucket) -> Option<ResolvedPolicy> {
	let genesis = engine.chain_of(&bucket.domain)?;
	let policy = engine.registry.submission_policy(&bucket.policy)?.clone();
	Some(ResolvedPolicy { policy, genesis })
}

/// Fails every row of a bucket whose policy or domain is no longer
/// registered: nothing could ever build them.
async fn abandon_unbuildable(
	engine: &DurableTxEngine,
	bucket: &SubmissionBucket,
	rows: &[ScheduledDurableTx],
) -> RoundOutcome {
	let ids: Vec<i64> = rows.iter().map(|row| row.id.0).collect();
	error!(%bucket, ?ids, "durable submissions name a policy or domain that is not registered, failing them");
	let abandons = rows.iter().map(|row| give_up_submission(engine, row.id));
	let abandoned = join_all(abandons).await;
	abandoned
		.into_iter()
		.map(RoundOutcome::from)
		.fold(RoundOutcome::NoProgress, RoundOutcome::or)
}
