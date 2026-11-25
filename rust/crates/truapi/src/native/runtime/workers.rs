//! Session-owned product worker startup, cancellation and availability.

mod registry;

use core::time::Duration;
use std::sync::{Arc, Weak};

use futures::{StreamExt, channel::mpsc};
use parking_lot::{MappedMutexGuard, MutexGuard};

use super::{
	ExecutionCallbacks, HostCallbacks, HostRejection, NativeProductExecution, NativeSubscription,
	NativeTrUApiHostRuntime, NativeWorkerExecution,
};
use crate::{
	background_task::BackgroundTask,
	host_logic::worker::WorkerController,
	platform::{ExecutableKind, ProductContext, ProductId},
};
use registry::{Registry, Removed, Signal};

/// How long an attempt may take to start running.
///
/// An attempt runs once its module has started and its bridge connection is authenticated. The
/// time counts from the attempt's start, and again each time a running attempt loses its
/// connection.
const STARTUP_DEADLINE: Duration = Duration::from_secs(90);

/// Starts and disposes the WebView of each worker attempt.
///
/// Calls arrive on whichever thread changed worker demand or reported a failure, which can be the
/// thread constructing the runtime or one of the core's own. Return without blocking: the starts
/// and stops of every product wait behind the call. The core holds no lock meanwhile, so an
/// implementation may call back into the runtime or the attempt.
#[uniffi::export(rust, foreign)]
pub trait NativeHostWebViewWorker: Send + Sync {
	/// Begin booting the product, reporting module start or failure through this attempt.
	///
	/// The core fails an attempt that is not running 90 seconds after its start, or after it
	/// lost its connection.
	fn start(&self, product_id: ProductId, attempt: Arc<NativeWorkerAttempt>);

	/// Dispose only the specified attempt's WebView, even when startup is incomplete.
	///
	/// Arrives once for each started attempt, after its [`Self::start`] call has returned,
	/// including after a failure the attempt reported. It can follow the start of the product's
	/// next attempt.
	fn stop(&self, product_id: ProductId, attempt_id: u64);
}

/// Availability of the product's session-owned worker.
#[derive(Clone, uniffi::Enum)]
pub enum NativeWorkerExecutionState {
	/// The session has no demand for the product, or a reload is replacing its worker.
	Idle,
	/// The worker is starting, reconnecting, or waiting for its next start after a failure.
	Pending,
	/// The module started and its authenticated transport can accept host calls.
	Running {
		/// Consumer operations on this execution.
		execution: Arc<dyn NativeWorkerExecution>,
	},
}

/// Observes one product without claiming its worker.
#[uniffi::export(rust, foreign)]
pub trait NativeWorkerExecutionObserver: Send + Sync {
	/// Receive the current state at registration, followed by each change in order.
	///
	/// `Running` arrives again when the worker's connection is replaced.
	fn changed(&self, state: NativeWorkerExecutionState);

	/// Finish the observation after the session has delivered its last update.
	fn finished(&self);
}

/// The session's worker state and the platform services its controller uses.
pub struct Session {
	registry: Registry,
	worker_host: Arc<dyn NativeHostWebViewWorker>,
	callbacks: Arc<dyn HostCallbacks>,
	startup_deadline: Duration,
}

impl Session {
	/// Keep worker state until shutdown.
	pub fn new(
		callbacks: &Arc<dyn HostCallbacks>,
		worker_host: Arc<dyn NativeHostWebViewWorker>,
	) -> Self {
		Self::new_with_startup_deadline(callbacks, worker_host, STARTUP_DEADLINE)
	}

	/// [`Self::new`], with `startup_deadline` in place of [`STARTUP_DEADLINE`].
	pub fn new_with_startup_deadline(
		callbacks: &Arc<dyn HostCallbacks>,
		worker_host: Arc<dyn NativeHostWebViewWorker>,
		startup_deadline: Duration,
	) -> Self {
		Self {
			registry: Registry::default(),
			worker_host,
			callbacks: callbacks.clone(),
			startup_deadline,
		}
	}
}

impl NativeTrUApiHostRuntime {
	fn worker_session(&self) -> Option<MappedMutexGuard<'_, Session>> {
		MutexGuard::try_map(self.workers.lock(), Option::as_mut).ok()
	}
}

/// The right to open one worker execution and report its module start or its failure.
#[derive(uniffi::Object)]
pub struct NativeWorkerAttempt {
	product: ProductId,
	id: u64,
	runtime: Weak<NativeTrUApiHostRuntime>,
}

#[uniffi::export]
impl NativeWorkerAttempt {
	/// The ID used by the platform to dispose only this WebView.
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

	/// Revoke this attempt after startup failure or renderer loss, applying the ledger's retry
	/// policy.
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
		let session = runtime.worker_session()?;
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

	/// Withdraw a disconnected worker until its WebView authenticates a new connection.
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
		if session.registry.running(&self.product).is_some() {
			runtime.runtime.worker_ledger().running(&self.product, self.id);
		}
		used
	}
}

struct WebView {
	host: Arc<dyn NativeHostWebViewWorker>,
	attempt: Arc<NativeWorkerAttempt>,
}

impl Drop for WebView {
	fn drop(&mut self) {
		self.host.stop(self.attempt.product.clone(), self.attempt.id);
	}
}

/// The worker ledger's controller for a native runtime.
///
/// Turns each start the ledger requests into an attempt on the session's WebView host, and each
/// stop into the disposal of that attempt.
/// The runtime owns the ledger that holds this controller, so the reference back is weak.
struct Controller(Weak<NativeTrUApiHostRuntime>);

/// Deliver worker demand queued during runtime construction.
pub fn install_controller(runtime: &Arc<NativeTrUApiHostRuntime>) {
	assert!(
		runtime
			.runtime
			.worker_ledger()
			.install_worker_controller(Arc::new(Controller(Arc::downgrade(runtime))))
	);
}

impl WorkerController for Controller {
	fn start_worker(&self, product_id: &ProductId, id: u64) {
		let Some(runtime) = self.0.upgrade() else { return };
		let (previous, webview) = {
			let Some(mut session) = runtime.worker_session() else { return };
			if !runtime.runtime.worker_ledger().is_current(product_id, id) {
				return;
			}
			let attempt = Arc::new(NativeWorkerAttempt {
				product: product_id.clone(),
				id,
				runtime: self.0.clone(),
			});
			let webview = Arc::new(WebView { host: session.worker_host.clone(), attempt });
			let timeout = webview.attempt.deadline(&runtime, session.startup_deadline);
			// A stop racing startup must reach the native host after its start callback returns.
			let previous = session.registry.begin(product_id, timeout, webview.clone());
			(previous, webview)
		};
		if let Some(previous) = previous {
			previous.stop();
		}
		webview.host.start(product_id.clone(), webview.attempt.clone());
	}

	fn stop_worker(&self, product_id: &ProductId) {
		let Some(runtime) = self.0.upgrade() else { return };
		let removed = runtime
			.worker_session()
			.and_then(|mut session| session.registry.end(product_id));
		if let Some(removed) = removed {
			removed.stop();
		}
	}

	fn worker_crashing(&self, product_id: &ProductId, crashes: u32) {
		if let Some(callbacks) = self
			.0
			.upgrade()
			.and_then(|runtime| Some(runtime.worker_session()?.callbacks.clone()))
		{
			callbacks.worker_crashing(product_id.clone(), crashes);
		}
	}
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
	/// Borrow the current ready execution without taking another reference.
	pub fn current_worker_execution(
		&self,
		product_id: ProductId,
	) -> Option<Arc<dyn NativeWorkerExecution>> {
		let execution: Arc<dyn NativeWorkerExecution> =
			self.worker_session()?.registry.running(&product_id)?;
		Some(execution)
	}

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
		if let Some(mut session) = self.worker_session() {
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
	/// [`NativeHostWebViewWorker::start`] or stop call is still running, on this thread or
	/// another, which is disposed when that call returns. Observers receive `finished` afterwards.
	pub fn shutdown_product_workers(&self) {
		self.runtime.worker_ledger().shutdown();
		let session = self.workers.lock().take();
		drop(session);
	}
}
