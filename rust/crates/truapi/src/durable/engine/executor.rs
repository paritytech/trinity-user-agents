//! The submission executor: builds every transaction waiting for its policy,
//! with one worker per domain and group, started and stopped by one
//! builder task.

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
	channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
	future::{AbortHandle, AbortRegistration, Abortable, FutureExt},
	stream::{BoxStream, StreamExt},
};
use parking_lot::Mutex;
use tracing::warn;

use super::DurableTxEngine;
use crate::{
	durable::{
		dao,
		model::{DomainId, GroupId, ScheduledDurableTx},
	},
	store::DbError,
};
use bucket::BucketRun;

/// The executor's builder task. Held in memory only: after a restart every
/// waiting row is simply built again.
#[derive(Default)]
pub struct SubmissionExecutor {
	builder: Mutex<Option<AbortHandle>>,
}

/// The rows one task builds together: one domain's, of one group.
#[derive(Clone, Debug, PartialEq, Eq, Hash, derive_more::Display)]
#[display(
    "domain {} group {:?}",
    domain.as_str(),
    group.as_ref().map(GroupId::as_str)
)]
pub struct SubmissionBucket {
	domain: DomainId,
	group: Option<GroupId>,
}

impl From<&ScheduledDurableTx> for SubmissionBucket {
	fn from(scheduled: &ScheduledDurableTx) -> Self {
		Self { domain: scheduled.domain.clone(), group: scheduled.group.clone() }
	}
}

impl SubmissionExecutor {
	/// Stops the builder and with it every worker, leaving the rows they
	/// worked on waiting.
	pub fn abort_all(&self) {
		if let Some(builder) = self.builder.lock().take() {
			builder.abort();
		}
	}
}

impl DurableTxEngine {
	/// Builds every transaction waiting for its policy, now and whenever more
	/// are scheduled or handed back, until the engine is dropped. Calling it
	/// again while it runs does nothing.
	pub fn ensure_submissions_started(self: &Arc<Self>) {
		let mut builder = self.executor.builder.lock();
		if builder.is_some() {
			return;
		}
		let (abort, registration) = AbortHandle::new_pair();
		*builder = Some(abort);
		drop(builder);
		let waiting = dao::observe_pending_submissions(&self.db);
		let task = Builder::new(Arc::downgrade(self)).run(waiting);
		self.spawn_abortable(task, registration);
	}

	/// Spawns `task`, stopped by the handle paired with `registration`.
	fn spawn_abortable(
		&self,
		task: impl Future<Output = ()> + Send + 'static,
		registration: AbortRegistration,
	) {
		let abortable = Abortable::new(task, registration).map(|_| ());
		self.spawner.spawn(abortable);
	}
}

/// A running worker. Dropping it stops the worker, so stopping the builder,
/// which owns every worker, stops them all.
struct Worker(AbortHandle);

impl Drop for Worker {
	fn drop(&mut self) {
		self.0.abort();
	}
}

/// What the builder reacts to, one at a time.
enum BuilderEvent {
	/// The waiting rows changed.
	Waiting(Result<Vec<ScheduledDurableTx>, DbError>),
	/// The worker of this bucket found it empty and returned.
	Finished(SubmissionBucket),
}

/// The only task that starts and stops workers, so a bucket never has two
/// and a waiting bucket always has one.
struct Builder {
	engine: Weak<DurableTxEngine>,
	workers: HashMap<SubmissionBucket, Worker>,
	/// The buckets of the latest waiting rows.
	waiting: HashSet<SubmissionBucket>,
	finished_tx: UnboundedSender<SubmissionBucket>,
	finished_rx: UnboundedReceiver<SubmissionBucket>,
}

impl Builder {
	fn new(engine: Weak<DurableTxEngine>) -> Self {
		let (finished_tx, finished_rx) = mpsc::unbounded();
		Self { engine, workers: HashMap::new(), waiting: HashSet::new(), finished_tx, finished_rx }
	}

	/// Handles every change of `waiting` and every finished worker until the
	/// waiting rows can no longer be observed or the engine is dropped. On
	/// the former it frees its slot, so the next
	/// [`DurableTxEngine::ensure_submissions_started`] starts a new builder.
	async fn run(mut self, waiting: BoxStream<'static, Result<Vec<ScheduledDurableTx>, DbError>>) {
		let mut waiting = waiting.fuse();
		loop {
			let event = futures::select! {
				read = waiting.next() => match read {
					Some(read) => BuilderEvent::Waiting(read),
					None => break,
				},
				bucket = self.finished_rx.select_next_some() => BuilderEvent::Finished(bucket),
			};
			let Some(engine) = self.engine.upgrade() else {
				return;
			};
			self.handle(&engine, event);
		}
		// Workers stop before the slot frees, so a builder started next never
		// runs beside them.
		self.workers.clear();
		if let Some(engine) = self.engine.upgrade() {
			engine.executor.builder.lock().take();
		}
	}

	/// Starts a worker for every waiting bucket that has none, unless its
	/// domain is not registered. A finished
	/// worker's bucket is started again when the latest rows still wait in
	/// it: those rows may have been scheduled after the worker's last read.
	fn handle(&mut self, engine: &Arc<DurableTxEngine>, event: BuilderEvent) {
		match event {
			BuilderEvent::Waiting(Ok(rows)) => {
				self.waiting = rows.iter().map(SubmissionBucket::from).collect();
				for bucket in self.waiting.clone() {
					self.start_if_idle(engine, bucket);
				}
			},
			BuilderEvent::Waiting(Err(error)) => {
				warn!(%error, "durable submissions could not be read")
			},
			BuilderEvent::Finished(bucket) => {
				self.workers.remove(&bucket);
				if self.waiting.contains(&bucket) {
					self.start_if_idle(engine, bucket);
				}
			},
		}
	}

	fn start_if_idle(&mut self, engine: &Arc<DurableTxEngine>, bucket: SubmissionBucket) {
		if self.workers.contains_key(&bucket) {
			return;
		}
		let Some(run) = BucketRun::new(engine, bucket.clone(), self.finished_tx.clone()) else {
			return;
		};
		let (abort, registration) = AbortHandle::new_pair();
		self.workers.insert(bucket, Worker(abort));
		engine.spawn_abortable(run.run_until_drained(), registration);
	}
}
