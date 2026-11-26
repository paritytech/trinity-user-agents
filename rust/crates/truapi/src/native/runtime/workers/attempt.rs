//! One attempt at running a product's worker, from its start on the engine to its stop there.

use core::time::Duration;
use std::sync::{Arc, Weak};

use parking_lot::MappedMutexGuard;

use super::{
	engine::NativeWorkerEngine,
	registry::{Registry, Removed, Signal},
	session::Session,
};
use crate::{
	background_task::BackgroundTask,
	native::runtime::{
		ExecutionCallbacks, HostRejection, NativeProductExecution, NativeTrUApiHostRuntime,
	},
	platform::{ExecutableKind, ProductContext, ProductId},
};

/// The right to open one worker execution and report its module start or its failure.
#[derive(uniffi::Object)]
pub struct NativeWorkerAttempt {
	product: ProductId,
	id: u64,
	runtime: Weak<NativeTrUApiHostRuntime>,
}

#[uniffi::export]
impl NativeWorkerAttempt {
	/// The ID [`NativeWorkerEngine::stop`] names this attempt by.
	pub fn id(&self) -> u64 {
		self.id
	}

	/// Open this attempt's execution with its trusted Worker context.
	///
	/// Fails if an execution was already opened, the demand ended or a replacement started.
	pub fn open_execution(
		self: &Arc<Self>,
		callbacks: ExecutionCallbacks,
	) -> Result<Arc<NativeProductExecution>, HostRejection> {
		let stale =
			|| HostRejection::Rejected { reason: "worker startup is no longer pending".into() };
		let runtime = self.runtime.upgrade().ok_or_else(stale)?;
		let mut session = self.current(&runtime).ok_or_else(stale)?;
		let open = || {
			runtime.open_execution(
				callbacks,
				ProductContext::new_with_execution(self.product.clone(), ExecutableKind::Worker),
				Some(self.clone()),
			)
		};
		session.registry.attach(&self.product, open).ok_or_else(stale)
	}

	/// Acknowledge that the module of this attempt's execution started. Its authenticated
	/// transport may connect later.
	///
	/// Returns false unless this attempt is current, has opened its execution and is still
	/// starting.
	pub fn module_started(&self) -> bool {
		self.update_readiness(Signal::ModuleStarted)
	}

	/// Revoke this attempt after its start failed or its worker was lost, applying the ledger's
	/// retry policy.
	pub fn failed(&self, reason: String) {
		self.fail(&reason, Registry::fail);
	}
}

impl NativeWorkerAttempt {
	fn deadline(
		self: &Arc<Self>,
		runtime: &NativeTrUApiHostRuntime,
		after: Duration,
	) -> BackgroundTask {
		let attempt = self.clone();
		BackgroundTask::spawn(&runtime.spawner, async move {
			tokio::time::sleep(after).await;
			attempt.startup_timeout();
		})
	}

	/// The session, locked, while the ledger counts this attempt as the product's current one.
	///
	/// The product's active entry is then this attempt's: an attempt is created in the lock hold
	/// that enters it, and the ledger's IDs only grow.
	fn current<'runtime>(
		&self,
		runtime: &'runtime NativeTrUApiHostRuntime,
	) -> Option<MappedMutexGuard<'runtime, Session>> {
		let session = Session::lock(runtime)?;
		runtime
			.runtime
			.worker_ledger()
			.is_current(&self.product, self.id)
			.then_some(session)
	}

	fn fail(
		&self,
		reason: &str,
		remove: impl FnOnce(&mut Registry, &ProductId) -> Option<Removed>,
	) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		let removed = self
			.current(&runtime)
			.and_then(|mut session| remove(&mut session.registry, &self.product));
		let Some(removed) = removed else { return };
		tracing::warn!(
			product = %self.product,
			reason = %crate::logging::HostText::new(reason),
			"a worker attempt failed"
		);
		removed.stop();
		runtime.runtime.worker_attempt_exited(&self.product, self.id);
	}

	fn startup_timeout(&self) {
		self.fail("worker startup timed out", Registry::time_out);
	}

	/// Publish a worker when its authenticated bridge can receive calls.
	pub fn connected(&self) {
		self.update_readiness(Signal::Connected);
	}

	/// Withdraw a disconnected worker until it authenticates a new connection.
	pub fn disconnected(self: &Arc<Self>) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		if let Some(mut session) = self.current(&runtime) {
			let after = session.startup_deadline;
			session.registry.lost(&self.product, || self.deadline(&runtime, after));
		}
	}

	fn update_readiness(&self, signal: Signal) -> bool {
		let Some(runtime) = self.runtime.upgrade() else { return false };
		let Some(mut session) = self.current(&runtime) else { return false };
		let used = session.registry.ready(&self.product, signal);
		if session.registry.is_running(&self.product) {
			runtime.runtime.worker_ledger().running(&self.product, self.id);
		}
		used
	}
}

/// An attempt on its engine, stopped there when the last copy is dropped.
///
/// The drop stops the attempt whether or not it was started, and the engine is promised a stop
/// only after its start returned, so the creator calls [`Self::start`] once while keeping a copy.
/// Both calls reach host code that can call back into the runtime, so neither is made with the
/// session locked.
pub struct StartedAttempt {
	engine: Arc<dyn NativeWorkerEngine>,
	attempt: Arc<NativeWorkerAttempt>,
}

impl StartedAttempt {
	/// The product's attempt `id`, for `engine` to run.
	pub fn new(
		engine: Arc<dyn NativeWorkerEngine>,
		product: ProductId,
		id: u64,
		runtime: Weak<NativeTrUApiHostRuntime>,
	) -> Self {
		Self { engine, attempt: Arc::new(NativeWorkerAttempt { product, id, runtime }) }
	}

	/// A task that fails the attempt if it is still starting once `after` has passed.
	pub fn deadline(&self, runtime: &NativeTrUApiHostRuntime, after: Duration) -> BackgroundTask {
		self.attempt.deadline(runtime, after)
	}

	/// Start the attempt on its engine.
	pub fn start(&self) {
		self.engine.start(self.attempt.product.clone(), self.attempt.clone());
	}
}

impl Drop for StartedAttempt {
	fn drop(&mut self) {
		self.engine.stop(self.attempt.product.clone(), self.attempt.id);
	}
}
