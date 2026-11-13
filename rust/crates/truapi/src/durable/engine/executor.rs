//! The submission executor: builds every transaction waiting for its policy,
//! with one worker per domain, started and stopped by one builder task.

mod attempt;
mod backoff;
mod domain_run;
mod schedule;

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
		model::{DeferredDurableTx, DomainId},
	},
	store::DbError,
};
use domain_run::DomainRun;

/// The executor's builder task. Held in memory only: after a restart every
/// waiting row is simply built again.
#[derive(Default)]
pub struct SubmissionExecutor {
	builder: Mutex<Option<AbortHandle>>,
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
	/// are deferred or handed back, until the engine is dropped. Calling it
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
	Waiting(Result<Vec<DeferredDurableTx>, DbError>),
	/// The worker of this domain found no waiting row and returned.
	Finished(DomainId),
}

/// The only task that starts and stops workers, so a domain never has two
/// and a domain with waiting rows always has one.
struct Builder {
	engine: Weak<DurableTxEngine>,
	workers: HashMap<DomainId, Worker>,
	/// The domains of the latest waiting rows.
	waiting: HashSet<DomainId>,
	finished_tx: UnboundedSender<DomainId>,
	finished_rx: UnboundedReceiver<DomainId>,
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
	async fn run(mut self, waiting: BoxStream<'static, Result<Vec<DeferredDurableTx>, DbError>>) {
		let mut waiting = waiting.fuse();
		loop {
			let event = futures::select! {
				read = waiting.next() => match read {
					Some(read) => BuilderEvent::Waiting(read),
					None => break,
				},
				domain = self.finished_rx.select_next_some() => BuilderEvent::Finished(domain),
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

	/// Starts a worker for every domain with waiting rows that has none,
	/// unless the domain is not registered. A finished worker's domain is
	/// started again when the latest rows still wait in it: those rows may
	/// have been deferred after the worker's last read.
	fn handle(&mut self, engine: &Arc<DurableTxEngine>, event: BuilderEvent) {
		match event {
			BuilderEvent::Waiting(Ok(rows)) => {
				self.waiting = rows.iter().map(|row| row.domain.clone()).collect();
				for domain in self.waiting.clone() {
					self.start_if_idle(engine, domain);
				}
			},
			BuilderEvent::Waiting(Err(error)) => {
				warn!(%error, "durable submissions could not be read")
			},
			BuilderEvent::Finished(domain) => {
				self.workers.remove(&domain);
				if self.waiting.contains(&domain) {
					self.start_if_idle(engine, domain);
				}
			},
		}
	}

	fn start_if_idle(&mut self, engine: &Arc<DurableTxEngine>, domain: DomainId) {
		if self.workers.contains_key(&domain) {
			return;
		}
		let Some(run) = DomainRun::new(engine, domain.clone(), self.finished_tx.clone()) else {
			return;
		};
		let (abort, registration) = AbortHandle::new_pair();
		self.workers.insert(domain, Worker(abort));
		engine.spawn_abortable(run.run_until_drained(), registration);
	}
}
