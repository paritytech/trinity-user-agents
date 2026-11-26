//! The worker state of one host session, and the runtime's calls into it.

use core::time::Duration;
use std::sync::Arc;

use futures::{StreamExt, channel::mpsc};
use parking_lot::{MappedMutexGuard, MutexGuard};

use super::{
	engine::NativeWorkerEngine,
	observer::{NativeWorkerExecutionObserver, NativeWorkerExecutionState},
	registry::Registry,
};
use crate::{
	native::runtime::{HostCallbacks, NativeSubscription, NativeTrUApiHostRuntime},
	platform::ProductId,
};

/// How long an attempt may take to start running.
///
/// An attempt runs once its module has started and its execution has a live connection. The time
/// counts from the attempt's start, and again each time a running attempt loses its connection.
pub const STARTUP_DEADLINE: Duration = Duration::from_secs(90);

/// The session's worker state and the platform services its controller uses.
pub struct Session {
	/// Each product's worker and the observers following it.
	pub registry: Registry,
	/// Starts and stops every attempt.
	pub worker_engine: Arc<dyn NativeWorkerEngine>,
	/// Told when a product's worker keeps crashing.
	pub callbacks: Arc<dyn HostCallbacks>,
	/// How long an attempt may take to start running.
	pub startup_deadline: Duration,
}

impl Session {
	/// Keep worker state until shutdown.
	pub fn new(
		callbacks: &Arc<dyn HostCallbacks>,
		worker_engine: Arc<dyn NativeWorkerEngine>,
	) -> Self {
		Self::new_with_startup_deadline(callbacks, worker_engine, STARTUP_DEADLINE)
	}

	/// [`Self::new`], with `startup_deadline` in place of [`STARTUP_DEADLINE`].
	pub fn new_with_startup_deadline(
		callbacks: &Arc<dyn HostCallbacks>,
		worker_engine: Arc<dyn NativeWorkerEngine>,
		startup_deadline: Duration,
	) -> Self {
		Self {
			registry: Registry::default(),
			worker_engine,
			callbacks: callbacks.clone(),
			startup_deadline,
		}
	}

	/// The runtime's session, locked.
	///
	/// `None` once [`NativeTrUApiHostRuntime::shutdown_product_workers`] has ended it.
	pub fn lock(runtime: &NativeTrUApiHostRuntime) -> Option<MappedMutexGuard<'_, Self>> {
		MutexGuard::try_map(runtime.workers.lock(), Option::as_mut).ok()
	}
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
	/// Follow one worker independently of the consumers that keep it running.
	///
	/// The first update describes the current attempt. Cancellation or session shutdown ends
	/// delivery.
	pub fn observe_worker_execution(
		&self,
		product_id: ProductId,
		observer: Arc<dyn NativeWorkerExecutionObserver>,
	) -> Arc<NativeSubscription> {
		let (sender, mut states) = mpsc::unbounded();
		if let Some(mut session) = Session::lock(self) {
			session.registry.observe(product_id, sender);
		} else {
			sender
				.unbounded_send(NativeWorkerExecutionState::Idle)
				.expect("worker observation is open");
		}
		self.spawner.spawn_observation(async move {
			while let Some(state) = states.next().await {
				observer.changed(state);
			}
			observer.finished();
		})
	}

	/// Restart the product after an update if at least one consumer still needs it.
	pub fn reload_worker(&self, product_id: ProductId) {
		self.runtime.worker_ledger().reload(&product_id);
	}

	/// Revoke all worker claims and dispose their executions when the host session ends.
	///
	/// The workers are disposed when this returns, except one whose
	/// [`NativeWorkerEngine::start`] or stop call is still running, on this thread or
	/// another, which is disposed when that call returns. Observers receive `finished` afterwards.
	pub fn shutdown_product_workers(&self) {
		self.runtime.worker_ledger().shutdown();
		let session = self.workers.lock().take();
		drop(session);
	}
}
