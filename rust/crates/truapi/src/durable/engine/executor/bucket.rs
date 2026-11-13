//! [`BucketRun`]: one worker building the waiting rows of one bucket, round
//! after round, until a round finds none.

use std::{
	collections::HashMap,
	sync::{Arc, Weak},
};

use futures::channel::mpsc::UnboundedSender;
use subxt::utils::H256;
use tracing::warn;

use super::{
	SubmissionBucket,
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

/// One bucket's worker. Holds the engine weakly, so a policy that never
/// answers cannot keep a dropped engine alive.
pub struct BucketRun {
	engine: Weak<DurableTxEngine>,
	db: Db,
	timer: Arc<dyn Timer>,
	bucket: SubmissionBucket,
	resolved: ResolvedPolicy,
	finished: UnboundedSender<SubmissionBucket>,
}

/// What builds a bucket's rows: its policy, and the chain its domain lives
/// on.
struct ResolvedPolicy {
	policy: Arc<dyn SubmissionPolicy>,
	genesis: H256,
}

/// What a round, or one row of it, changed in the ledger.
#[derive(Clone, PartialEq, Eq)]
enum RoundOutcome {
	/// A row was decided, so the next round runs at once.
	Progress,
	/// Nothing was decided, so the next round backs off.
	NoProgress,
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
	/// The worker for `bucket`, which sends `bucket` on `finished` once it
	/// finds the bucket empty, or `None` when its domain or policy is not
	/// registered: its rows are then left waiting untouched.
	pub fn new(
		engine: &Arc<DurableTxEngine>,
		bucket: SubmissionBucket,
		finished: UnboundedSender<SubmissionBucket>,
	) -> Option<Self> {
		let resolved = resolve_policy(engine, &bucket)?;
		Some(Self {
			engine: Arc::downgrade(engine),
			db: engine.db.clone(),
			timer: engine.timer.clone(),
			bucket,
			resolved,
			finished,
		})
	}

	/// Runs rounds until a round finds the bucket empty, backing off after
	/// every round that decided nothing, then tells the builder.
	pub async fn run_until_drained(self) {
		let mut rounds_without_progress = 0;
		while self.engine.strong_count() > 0 {
			match self.decide_round().await {
				None => break,
				Some(RoundOutcome::Progress) => rounds_without_progress = 0,
				Some(RoundOutcome::NoProgress) => {
					rounds_without_progress += 1;
					self.timer.sleep(backoff(rounds_without_progress)).await;
				},
			}
		}
		// The builder may be gone with the engine; then nobody needs to know.
		let _ = self.finished.unbounded_send(self.bucket.clone());
	}

	/// Asks the policy about every waiting row once and applies its answers,
	/// or `None` when no row is waiting. A row still owned by a watch counts
	/// as waiting but is left for a later round.
	async fn decide_round(&self) -> Option<RoundOutcome> {
		let Some(rows) = self.readable_rows().await else {
			return Some(RoundOutcome::NoProgress);
		};
		if rows.is_empty() {
			return None;
		}
		Some(self.decide_rows(rows).await)
	}

	/// Builds the waiting `rows` that no watch owns.
	async fn decide_rows(&self, rows: Vec<ScheduledDurableTx>) -> RoundOutcome {
		let Some(engine) = self.engine.upgrade() else {
			return RoundOutcome::NoProgress;
		};
		// A row whose previous attempt is still followed waits for that
		// follower to stop, so a transaction never has two.
		let rows = without_followed(&engine, rows);
		if rows.is_empty() {
			return RoundOutcome::NoProgress;
		}
		// The engine is held only while this round decides, never across a
		// policy call that may not return.
		drop(engine);
		self.prepare_and_apply(rows).await
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

	/// Builds `rows` with the bucket's policy after their cooldown.
	async fn prepare_and_apply(&self, rows: Vec<ScheduledDurableTx>) -> RoundOutcome {
		let resolved = &self.resolved;
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

	async fn read_bucket_rows(&self) -> Result<Vec<ScheduledDurableTx>, DbError> {
		let bucket = self.bucket.clone();
		self.db
			.read(move |conn| {
				dao::bucket_submissions(conn, &bucket.domain, &bucket.policy, bucket.group.as_ref())
			})
			.await
	}
}

/// `rows` without those a watch still owns.
fn without_followed(
	engine: &DurableTxEngine,
	rows: Vec<ScheduledDurableTx>,
) -> Vec<ScheduledDurableTx> {
	rows.into_iter().filter(|row| !engine.ownership.is_owned(row.id)).collect()
}

/// The policy and chain that build `bucket`, when both are still registered.
fn resolve_policy(engine: &DurableTxEngine, bucket: &SubmissionBucket) -> Option<ResolvedPolicy> {
	let genesis = engine.chain_of(&bucket.domain)?;
	let policy = engine.registry.submission_policy(&bucket.policy)?.clone();
	Some(ResolvedPolicy { policy, genesis })
}
