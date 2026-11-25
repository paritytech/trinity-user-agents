//! Session-owned product worker startup, cancellation and availability.

use std::{
	collections::HashMap,
	sync::{Arc, Weak, atomic::Ordering},
};

use futures::{StreamExt, channel::mpsc};
use parking_lot::{MappedMutexGuard, MutexGuard};

use super::{
	HostCallbacks, HostRejection, NativeChatCallbacks, NativeGameCallbacks, NativeProductExecution,
	NativeSubscription, NativeTrUApiHostRuntime, NativeWorkerExecution,
};
use crate::{
	background_task::BackgroundTask,
	host_logic::worker::WorkerController,
	native::subscribers::Subscribers,
	platform::{ExecutableKind, ProductContext, ProductId},
};

/// How long an attempt may take to start running.
///
/// An attempt runs once its module has started and its bridge connection is authenticated. The
/// time counts from the attempt's start, and again each time a running attempt loses its
/// connection.
const STARTUP_DEADLINE: core::time::Duration = core::time::Duration::from_secs(90);

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
	/// No attempt will start for the current demand.
	Failed {
		/// Why this worker is unavailable.
		reason: String,
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

impl NativeProductExecution {
	fn has_live_transport(&self) -> bool {
		self.product_control
			.lock()
			.expect("product control mutex poisoned")
			.as_ref()
			.is_some_and(crate::ProductRuntimeControl::is_connected)
	}
}

enum Phase {
	Starting {
		owner: Option<Arc<NativeProductExecution>>,
		module_ready: bool,
		/// Fails the attempt if it is still starting after [`STARTUP_DEADLINE`]. Dropped, and so
		/// cancelled, with this phase.
		_timeout: BackgroundTask,
	},
	Running(Arc<NativeProductExecution>),
}

enum Entry {
	Active { id: u64, phase: Phase, cancel: Box<dyn FnOnce() + Send> },
	Retrying,
	Unavailable(String),
}

impl Entry {
	fn state(&self) -> NativeWorkerExecutionState {
		match self {
			Self::Active { phase: Phase::Starting { .. }, .. } | Self::Retrying => {
				NativeWorkerExecutionState::Pending
			},
			Self::Active { phase: Phase::Running(owner), .. } => {
				NativeWorkerExecutionState::Running { execution: owner.clone() }
			},
			Self::Unavailable(reason) => {
				NativeWorkerExecutionState::Failed { reason: reason.clone() }
			},
		}
	}

	fn stop(self) {
		if let Self::Active { phase, cancel, .. } = self {
			match phase {
				Phase::Starting { owner: Some(owner), .. } | Phase::Running(owner) => {
					owner.shutdown()
				},
				Phase::Starting { owner: None, .. } => {},
			}
			cancel();
		}
	}
}

#[derive(Default)]
struct Registry {
	entries: HashMap<ProductId, Entry>,
	observers: Subscribers<ProductId, NativeWorkerExecutionState>,
}

impl Registry {
	fn state(&self, product: &ProductId) -> NativeWorkerExecutionState {
		self.entries
			.get(product)
			.map(Entry::state)
			.unwrap_or(NativeWorkerExecutionState::Idle)
	}

	fn changed(&mut self, product: &ProductId) {
		self.observers.send(product, &self.state(product));
	}

	fn insert(&mut self, product: &ProductId, entry: Entry) -> Option<Entry> {
		let pending = |registry: &Self| {
			matches!(registry.state(product), NativeWorkerExecutionState::Pending)
		};
		let was_pending = pending(self);
		let previous = self.entries.insert(product.clone(), entry);
		if !(was_pending && pending(self)) {
			self.changed(product);
		}
		previous
	}
}

impl Drop for Registry {
	fn drop(&mut self) {
		for (_, entry) in self.entries.drain() {
			entry.stop();
		}
	}
}

/// The session's worker state and the platform services its controller uses.
pub struct Session {
	registry: Registry,
	worker_host: Option<Arc<dyn NativeHostWebViewWorker>>,
	callbacks: Arc<dyn HostCallbacks>,
}

impl Session {
	/// Keep worker state until shutdown, including failed starts on hosts without a launcher.
	pub fn new(
		callbacks: &Arc<dyn HostCallbacks>,
		worker_host: Option<Arc<dyn NativeHostWebViewWorker>>,
	) -> Self {
		Self { registry: Registry::default(), worker_host, callbacks: callbacks.clone() }
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
		callbacks: Arc<dyn HostCallbacks>,
		chat_callbacks: Option<Arc<dyn NativeChatCallbacks>>,
		game_callbacks: Option<Arc<dyn NativeGameCallbacks>>,
	) -> Result<Arc<NativeProductExecution>, HostRejection> {
		let stale =
			|| HostRejection::Rejected { reason: "worker startup is no longer pending".into() };
		let runtime = self.runtime.upgrade().ok_or_else(stale)?;
		let mut session = runtime.worker_session().ok_or_else(stale)?;
		let Some(Entry::Active { id, phase: Phase::Starting { owner, .. }, .. }) =
			session.registry.entries.get_mut(&self.product)
		else {
			return Err(stale());
		};
		if *id != self.id ||
			owner.is_some() ||
			!runtime.runtime.worker_ledger().is_current(&self.product, self.id)
		{
			return Err(stale());
		}
		let execution = runtime.open_product_execution_with_callbacks(
			callbacks,
			chat_callbacks,
			game_callbacks,
			ProductContext::new_with_execution(self.product.clone(), ExecutableKind::Worker),
			Some(self.clone()),
		);
		*owner = Some(execution.clone());
		Ok(execution)
	}

	/// Acknowledge that the module of this attempt's execution started. Its authenticated
	/// transport may connect later.
	///
	/// Returns false unless this attempt is current, has opened its execution and is still
	/// starting.
	pub fn module_started(&self) -> bool {
		self.update_readiness(true)
	}

	/// Revoke this attempt after startup failure or renderer loss, applying the ledger's retry
	/// policy.
	pub fn failed(&self, reason: String) {
		self.fail_if(&reason, |_| true);
	}
}

impl NativeWorkerAttempt {
	fn deadline(self: &Arc<Self>, runtime: &NativeTrUApiHostRuntime) -> BackgroundTask {
		let attempt = self.clone();
		BackgroundTask::spawn(&runtime.spawner, async move {
			tokio::time::sleep(STARTUP_DEADLINE).await;
			attempt.startup_timeout();
		})
	}

	fn fail_if(&self, reason: &str, applies: impl FnOnce(&Phase) -> bool) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		let old = {
			let Some(mut session) = runtime.worker_session() else { return };
			let registry = &mut session.registry;
			let Some(Entry::Active { id, phase, .. }) = registry.entries.get(&self.product) else {
				return;
			};
			if *id != self.id ||
				!applies(phase) ||
				!runtime.runtime.worker_ledger().is_current(&self.product, self.id)
			{
				return;
			}
			registry.insert(&self.product, Entry::Retrying).expect("current attempt")
		};
		tracing::warn!(
			product = %self.product,
			reason = %crate::logging::HostText::new(reason),
			"a worker attempt failed"
		);
		old.stop();
		runtime.runtime.worker_attempt_exited(&self.product, self.id);
	}

	fn startup_timeout(&self) {
		self.fail_if("worker startup timed out", |phase| matches!(phase, Phase::Starting { .. }));
	}

	/// Publish a worker when its authenticated bridge can receive calls.
	pub fn connected(&self) {
		self.update_readiness(false);
	}

	/// Withdraw a disconnected worker until its WebView authenticates a new connection.
	pub fn disconnected(self: &Arc<Self>) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		let Some(mut session) = runtime.worker_session() else { return };
		let registry = &mut session.registry;
		let Some(Entry::Active { id, phase, .. }) = registry.entries.get_mut(&self.product) else {
			return;
		};
		let Phase::Running(owner) = phase else { return };
		if self.id != *id || owner.has_live_transport() {
			return;
		}
		*phase = Phase::Starting {
			owner: Some(owner.clone()),
			module_ready: true,
			_timeout: self.deadline(&runtime),
		};
		registry.changed(&self.product);
	}

	fn update_readiness(&self, module_started: bool) -> bool {
		let Some(runtime) = self.runtime.upgrade() else { return false };
		let Some(mut session) = runtime.worker_session() else { return false };
		let registry = &mut session.registry;
		let Some(Entry::Active { id, phase, .. }) = registry.entries.get_mut(&self.product) else {
			return false;
		};
		if *id != self.id || !runtime.runtime.worker_ledger().is_current(&self.product, self.id) {
			return false;
		}
		match phase {
			Phase::Running(owner) if !module_started && owner.has_live_transport() => {},
			Phase::Starting { owner: Some(owner), module_ready, .. }
				if !owner.closed.load(Ordering::Acquire) =>
			{
				*module_ready |= module_started;
				if !*module_ready || !owner.has_live_transport() {
					return true;
				}
				*phase = Phase::Running(owner.clone());
			},
			_ => return false,
		}
		registry.changed(&self.product);
		true
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
/// Turns each start the ledger requests into an attempt on the session's WebView host, or into
/// a failed state when the session has none, and each stop into the disposal of that attempt.
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
			let Some(host) = session.worker_host.clone() else {
				session
					.registry
					.insert(product_id, Entry::Unavailable("no worker host configured".into()));
				return;
			};
			let attempt = Arc::new(NativeWorkerAttempt {
				product: product_id.clone(),
				id,
				runtime: self.0.clone(),
			});
			let webview = Arc::new(WebView { host, attempt });
			// A stop racing startup must reach the native host after its start callback returns.
			let started = webview.clone();
			let entry = Entry::Active {
				id,
				phase: Phase::Starting {
					owner: None,
					module_ready: false,
					_timeout: webview.attempt.deadline(&runtime),
				},
				cancel: Box::new(move || drop(started)),
			};
			let previous = session.registry.insert(product_id, entry);
			(previous, webview)
		};
		if let Some(previous) = previous {
			previous.stop();
		}
		webview.host.start(product_id.clone(), webview.attempt.clone());
	}

	fn stop_worker(&self, product_id: &ProductId) {
		let Some(runtime) = self.0.upgrade() else { return };
		let previous = {
			let Some(mut session) = runtime.worker_session() else { return };
			let Some(previous) = session.registry.entries.remove(product_id) else { return };
			session.registry.changed(product_id);
			previous
		};
		previous.stop();
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
		match self.worker_session()?.registry.entries.get(&product_id) {
			Some(Entry::Active { phase: Phase::Running(owner), .. }) => Some(owner.clone()),
			_ => None,
		}
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
			let registry = &mut session.registry;
			sender
				.unbounded_send(registry.state(&product_id))
				.expect("worker observation is open");
			registry.observers.register(product_id, sender);
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
	/// The workers are disposed when this returns, except one whose start or stop another thread
	/// is delivering, which completes on that thread. Observers receive `finished` afterwards.
	pub fn shutdown_product_workers(&self) {
		self.runtime.worker_ledger().shutdown();
		let session = self.workers.lock().take();
		drop(session);
	}
}
