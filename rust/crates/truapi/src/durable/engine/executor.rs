//! The submission executor: builds every transaction waiting for its policy,
//! with one task per domain, policy and group.

mod attempt;
mod backoff;
mod bucket;

#[cfg(test)]
mod tests;

use core::future::Future;
use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Weak},
};

use futures::{
	future::{AbortHandle, AbortRegistration, Abortable, FutureExt},
	stream::{BoxStream, StreamExt},
};
use parking_lot::Mutex;
use tracing::warn;

use super::DurableTxEngine;
use crate::{
	durable::{
		dao,
		model::{DomainId, DurableTxId, GroupId, PolicyId, ScheduledDurableTx},
	},
	store::DbError,
};
use bucket::BucketRun;

/// The executor's tasks and what they remember between rounds. Held in
/// memory only: after a restart every waiting row is simply built again.
#[derive(Default)]
pub struct SubmissionExecutor {
	collector: Mutex<Option<AbortHandle>>,
	/// The running bucket tasks. A bucket in the map is being worked on.
	buckets: Mutex<HashMap<SubmissionBucket, AbortHandle>>,
	/// How many attempts each row was built into, for the rebuild cooldown.
	attempts_started: Mutex<HashMap<DurableTxId, u32>>,
}

/// The rows one task builds together: one domain's, of one policy and group.
#[derive(Clone, Debug, PartialEq, Eq, Hash, derive_more::Display)]
#[display(
    "domain {} policy {} group {:?}",
    domain.as_str(),
    policy.as_str(),
    group.as_ref().map(GroupId::as_str)
)]
pub struct SubmissionBucket {
	domain: DomainId,
	policy: PolicyId,
	group: Option<GroupId>,
}

impl SubmissionBucket {
	fn of(scheduled: &ScheduledDurableTx) -> Self {
		Self {
			domain: scheduled.domain.clone(),
			policy: scheduled.plan.policy.clone(),
			group: scheduled.group.clone(),
		}
	}
}

impl SubmissionExecutor {
	/// Stops every task, leaving the rows they worked on waiting.
	pub fn abort_all(&self) {
		if let Some(collector) = self.collector.lock().take() {
			collector.abort();
		}
		for (_, bucket) in self.buckets.lock().drain() {
			bucket.abort();
		}
	}

	/// Drops what is remembered about row `id`, once nothing will build it
	/// again.
	pub fn forget_attempts(&self, id: DurableTxId) {
		self.attempts_started.lock().remove(&id);
	}

	/// Counts one more attempt built for row `id`.
	fn count_started_attempt(&self, id: DurableTxId) {
		let mut started = self.attempts_started.lock();
		*started.entry(id).or_default() += 1;
	}

	/// The most attempts any row of `rows` was built into.
	fn most_attempts(&self, rows: &[ScheduledDurableTx]) -> u32 {
		let started = self.attempts_started.lock();
		let attempts = rows.iter().filter_map(|row| started.get(&row.id)).max();
		attempts.copied().unwrap_or(0)
	}
}

impl DurableTxEngine {
	/// Builds every transaction waiting for its policy, now and whenever more
	/// are scheduled or handed back, until the engine is dropped. Calling it
	/// again while it runs does nothing.
	pub fn ensure_submissions_started(self: &Arc<Self>) {
		let mut collector = self.executor.collector.lock();
		if collector.is_some() {
			return;
		}
		let (abort, registration) = AbortHandle::new_pair();
		*collector = Some(abort);
		drop(collector);
		let waiting = dao::observe_pending_submissions(&self.db);
		let task = launch_on_pending_change(Arc::downgrade(self), waiting);
		self.spawn_abortable(task, registration);
	}

	/// Starts a task for every bucket of `waiting` that has none.
	fn launch_buckets(self: &Arc<Self>, waiting: &[ScheduledDurableTx]) {
		let buckets: HashSet<SubmissionBucket> = waiting.iter().map(SubmissionBucket::of).collect();
		for bucket in buckets {
			self.launch_if_idle(bucket);
		}
	}

	/// Starts a task for `bucket` unless one runs. Registered before it is
	/// spawned, so a task that drains at once cannot leave a stale entry.
	fn launch_if_idle(self: &Arc<Self>, bucket: SubmissionBucket) {
		let mut buckets = self.executor.buckets.lock();
		if buckets.contains_key(&bucket) {
			return;
		}
		let (abort, registration) = AbortHandle::new_pair();
		buckets.insert(bucket.clone(), abort.clone());
		drop(buckets);
		let run = BucketRun::new(self, bucket, abort).run_until_drained();
		self.spawn_abortable(run, registration);
	}

	/// Spawns `task`, stopped by the handle paired with `registration`.
	fn spawn_abortable(
		&self,
		task: impl Future<Output = ()> + Send + 'static,
		registration: AbortRegistration,
	) {
		let abortable = Abortable::new(task, registration).map(|_| ());
		(self.spawner)(Box::pin(abortable));
	}
}

/// Launches buckets on every change of the waiting rows. Every emission
/// launches every idle bucket it names, because a bucket may have ended
/// since the last one.
async fn launch_on_pending_change(
	engine: Weak<DurableTxEngine>,
	mut waiting: BoxStream<'static, Result<Vec<ScheduledDurableTx>, DbError>>,
) {
	while let Some(read) = waiting.next().await {
		let Some(engine) = engine.upgrade() else {
			return;
		};
		match read {
			Ok(rows) => engine.launch_buckets(&rows),
			Err(error) => warn!(%error, "durable submissions could not be read"),
		}
	}
	if let Some(engine) = engine.upgrade() {
		engine.executor.collector.lock().take();
	}
}
