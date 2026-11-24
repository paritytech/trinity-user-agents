//! Session-owned WebView worker claims, startup attempts and availability.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, Weak, atomic::Ordering},
};

use futures::{StreamExt, channel::mpsc};

use super::{
	HostCallbacks, HostRejection, NativeChatCallbacks, NativeGameCallbacks, NativeProductExecution,
	NativeRuntimeConfigError, NativeSubscription, NativeTrUApiHostRuntime, NativeWorkerExecution,
	worker_execution,
};
use crate::{
	WorkerTransition,
	background_task::BackgroundTask,
	host_logic::worker::WorkerController,
	native::subscribers::Subscribers,
	platform::{ExecutableKind, ProductContext, ProductId},
};

/// Operations the native UI thread performs on an individual WebView.
#[uniffi::export(rust, foreign)]
pub trait NativeWebViewWorkerHost: Send + Sync {
	/// Begin booting the product, reporting readiness or failure through this attempt.
	fn start(&self, product_id: ProductId, attempt: Arc<NativeWorkerAttempt>);

	/// Dispose only the specified attempt's WebView, even when startup is incomplete.
	fn stop(&self, product_id: ProductId, attempt_id: u64);
}

/// Availability of the product's session-owned worker.
#[derive(Clone, uniffi::Enum)]
pub enum NativeWorkerExecutionState {
	/// The session has no demand for the product.
	Idle,
	/// The current attempt is starting its module or connecting its transport.
	Pending,
	/// The module started and its authenticated transport can accept host calls.
	Running {
		/// Consumer operations on this execution.
		execution: Arc<NativeWorkerExecution>,
	},
	/// The attempt exited and its retry has not started.
	Failed {
		/// Why startup or the running WebView failed.
		reason: String,
	},
}

/// Observes one product without claiming its worker.
#[uniffi::export(rust, foreign)]
pub trait NativeWorkerExecutionObserver: Send + Sync {
	/// Receive the current state at registration, followed by each change in order.
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

enum Entry {
	Starting {
		id: u64,
		owner: Option<Arc<NativeProductExecution>>,
		module_ready: bool,
		_timeout: BackgroundTask,
	},
	Running {
		id: u64,
		owner: Arc<NativeProductExecution>,
		consumer: Arc<NativeWorkerExecution>,
	},
	Failed(String),
}

impl Entry {
	fn state(&self) -> NativeWorkerExecutionState {
		match self {
			Self::Starting { .. } => NativeWorkerExecutionState::Pending,
			Self::Running { consumer, .. } => {
				NativeWorkerExecutionState::Running { execution: consumer.clone() }
			},
			Self::Failed(reason) => NativeWorkerExecutionState::Failed { reason: reason.clone() },
		}
	}

	fn id(&self) -> Option<u64> {
		match self {
			Self::Starting { id, .. } | Self::Running { id, .. } => Some(*id),
			Self::Failed(_) => None,
		}
	}

	fn stop(self, webviews: &dyn NativeWebViewWorkerHost, product: ProductId) {
		let Some(id) = self.id() else { return };
		match self {
			Self::Starting { owner: Some(owner), .. } | Self::Running { owner, .. } => owner.shutdown(),
			_ => {},
		}
		webviews.stop(product, id);
	}
}

/// Current managed executions and their native observations.
pub struct Registry {
	entries: HashMap<ProductId, Entry>,
	observers: Subscribers<ProductId, NativeWorkerExecutionState>,
	webviews: Option<Arc<dyn NativeWebViewWorkerHost>>,
	callbacks: Weak<dyn HostCallbacks>,
	closed: bool,
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
}

impl Drop for Registry {
	fn drop(&mut self) {
		if let Some(webviews) = self.webviews.take() {
			for (product, entry) in self.entries.drain() {
				entry.stop(webviews.as_ref(), product);
			}
		}
	}
}

/// The native embedding either supplies its own worker executions or installs a session adapter.
pub enum Workers {
	/// Existing host-owned executions, replaced when the host opens another worker for the product.
	External(Mutex<HashMap<ProductId, Weak<NativeProductExecution>>>),
	/// Workers whose startup and shutdown are driven by the ledger.
	Managed(Mutex<Registry>),
}

impl Workers {
	/// Create the registry while the ledger waits for the session adapter.
	pub fn managed(callbacks: &Arc<dyn HostCallbacks>) -> Self {
		Self::Managed(Mutex::new(Registry {
			entries: HashMap::new(),
			observers: Subscribers::default(),
			webviews: None,
			callbacks: Arc::downgrade(callbacks),
			closed: false,
		}))
	}

	/// Replace an externally controlled worker. Managed workers open through their attempt.
	pub fn register_external(
		&self,
		execution: &Arc<NativeProductExecution>,
	) -> Result<(), NativeRuntimeConfigError> {
		let Self::External(external) = self else {
			return Err(NativeRuntimeConfigError::Invalid {
				reason: "open a managed worker through its startup attempt".into(),
			});
		};
		let previous = external
			.lock()
			.expect("worker registry mutex poisoned")
			.insert(execution.product.product_id.clone(), Arc::downgrade(execution))
			.and_then(|previous| previous.upgrade());
		if let Some(previous) = previous {
			previous.shutdown();
		}
		Ok(())
	}

	fn registry(&self) -> Option<std::sync::MutexGuard<'_, Registry>> {
		match self {
			Self::External(_) => None,
			Self::Managed(registry) => {
				Some(registry.lock().expect("worker registry mutex poisoned"))
			},
		}
	}
}

/// The right to open and acknowledge precisely one worker startup.
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
		let mut registry = runtime.workers.registry().ok_or_else(stale)?;
		let Some(Entry::Starting { id, owner, .. }) = registry.entries.get_mut(&self.product)
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

	/// Acknowledge that the platform has started this execution's JavaScript entry.
	///
	/// Returns false for a stale or unrelated execution. Publication also requires its
	/// authenticated transport, which may connect before or after this acknowledgement.
	pub fn ready(&self, execution: Arc<NativeProductExecution>) -> bool {
		self.update_readiness(Some(&execution))
	}

	/// Revoke this attempt after startup failure or renderer loss, applying the ledger's retry
	/// policy.
	pub fn failed(&self, reason: String) {
		self.fail_if(reason, |_| true);
	}
}

impl NativeWorkerAttempt {
	fn deadline(self: &Arc<Self>, runtime: &NativeTrUApiHostRuntime) -> BackgroundTask {
		let attempt = self.clone();
		BackgroundTask::spawn(&runtime.spawner, async move {
			tokio::time::sleep(core::time::Duration::from_secs(90)).await;
			attempt.startup_timeout();
		})
	}

	fn fail_if(&self, reason: String, applies: impl FnOnce(&Entry) -> bool) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		let (old, webviews) = {
			let mut registry =
				runtime.workers.registry().expect("attempt belongs to a managed runtime");
			let Some(entry) = registry.entries.get(&self.product) else { return };
			if entry.id() != Some(self.id) ||
				!applies(entry) ||
				!runtime.runtime.worker_ledger().is_current(&self.product, self.id)
			{
				return;
			}
			let old = registry
				.entries
				.insert(self.product.clone(), Entry::Failed(reason))
				.expect("current attempt");
			registry.changed(&self.product);
			(old, registry.webviews.clone().expect("attempt has a WebView host"))
		};
		old.stop(webviews.as_ref(), self.product.clone());
		runtime.runtime.worker_attempt_exited(&self.product, self.id);
	}

	fn startup_timeout(&self) {
		self.fail_if("worker startup timed out".to_owned(), |entry| {
			matches!(entry, Entry::Starting { .. })
		});
	}

	/// Publish startup when the bridge authenticates after the platform acknowledgement.
	pub fn connected(&self) {
		self.update_readiness(None);
	}

	/// Withdraw a disconnected worker until its WebView authenticates a new connection.
	pub fn disconnected(self: &Arc<Self>) {
		let Some(runtime) = self.runtime.upgrade() else { return };
		let mut registry = runtime.workers.registry().expect("managed worker controller");
		let Some(Entry::Running { id, owner, .. }) = registry.entries.get(&self.product) else {
			return;
		};
		if self.id != *id || owner.has_live_transport() {
			return;
		}
		let owner = owner.clone();
		registry.entries.insert(
			self.product.clone(),
			Entry::Starting {
				id: self.id,
				owner: Some(owner),
				module_ready: true,
				_timeout: self.deadline(&runtime),
			},
		);
		registry.changed(&self.product);
	}

	fn update_readiness(&self, acknowledged: Option<&Arc<NativeProductExecution>>) -> bool {
		let Some(runtime) = self.runtime.upgrade() else { return false };
		let mut registry =
			runtime.workers.registry().expect("attempt belongs to a managed runtime");
		if acknowledged.is_none() &&
			matches!(registry.entries.get(&self.product),
			Some(Entry::Running { id, owner, .. }) if *id == self.id && owner.has_live_transport())
		{
			registry.changed(&self.product);
			return true;
		}
		let Some(Entry::Starting { id, owner: Some(owner), module_ready, .. }) =
			registry.entries.get_mut(&self.product)
		else {
			return false;
		};
		if *id != self.id ||
			owner.closed.load(Ordering::Acquire) ||
			!runtime.runtime.worker_ledger().is_current(&self.product, self.id)
		{
			return false;
		}
		if let Some(acknowledged) = acknowledged {
			if !Arc::ptr_eq(owner, acknowledged) {
				return false;
			}
			*module_ready = true;
		}
		if *module_ready && owner.has_live_transport() {
			let running = Entry::Running {
				id: self.id,
				owner: owner.clone(),
				consumer: worker_execution::shared(owner),
			};
			registry.entries.insert(self.product.clone(), running);
			registry.changed(&self.product);
		}
		true
	}
}

struct Controller(Weak<NativeTrUApiHostRuntime>);

impl WorkerController for Controller {
	fn start_worker(&self, product_id: &ProductId, id: u64) {
		let Some(runtime) = self.0.upgrade() else { return };
		let attempt = Arc::new(NativeWorkerAttempt {
			product: product_id.clone(),
			id,
			runtime: self.0.clone(),
		});
		let (previous, webviews) = {
			let mut registry = runtime.workers.registry().expect("managed worker controller");
			if !runtime.runtime.worker_ledger().is_current(product_id, id) {
				return;
			}

			let previous = registry.entries.insert(
				product_id.clone(),
				Entry::Starting {
					id,
					owner: None,
					module_ready: false,
					_timeout: attempt.deadline(&runtime),
				},
			);
			registry.changed(product_id);
			(previous, registry.webviews.clone().expect("installed WebView host"))
		};
		if let Some(previous) = previous {
			previous.stop(webviews.as_ref(), product_id.clone());
		}
		webviews.start(product_id.clone(), attempt);
	}

	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition) {
		debug_assert_eq!(transition, WorkerTransition::Stop);
		let Some(runtime) = self.0.upgrade() else { return };
		let (previous, webviews) = {
			let mut registry = runtime.workers.registry().expect("managed worker controller");
			let Some(previous) = registry.entries.remove(product_id) else { return };
			registry.changed(product_id);
			(previous, registry.webviews.clone().expect("installed WebView host"))
		};
		previous.stop(webviews.as_ref(), product_id.clone());
	}

	fn worker_crashing(&self, product_id: &ProductId, crashes: u32) {
		if let Some(callbacks) = self
			.0
			.upgrade()
			.and_then(|runtime| runtime.workers.registry()?.callbacks.upgrade())
		{
			callbacks.worker_crashing(product_id.clone(), crashes);
		}
	}

	fn shutdown(&self) {
		if let Some(runtime) = self.0.upgrade() {
			let mut registry = runtime.workers.registry().expect("managed worker controller");
			registry.closed = true;
			registry.observers = Subscribers::default();
			let webviews = registry.webviews.take();
			drop(registry);
			drop(webviews);
		}
	}
}

#[uniffi::export]
impl NativeTrUApiHostRuntime {
	/// Install the WebView adapter once, delivering any demand held since session startup.
	///
	/// Returns false for an externally managed runtime or an adapter already installed.
	pub fn enable_product_workers(
		self: &Arc<Self>,
		host: Arc<dyn NativeWebViewWorkerHost>,
	) -> bool {
		{
			let Some(mut registry) = self.workers.registry() else { return false };
			if registry.webviews.is_some() || registry.closed {
				return false;
			}
			registry.webviews = Some(host);
		}
		self.runtime
			.worker_ledger()
			.install_worker_controller(Arc::new(Controller(Arc::downgrade(self))))
	}

	/// Borrow the current ready execution without taking another reference.
	pub fn current_worker_execution(
		&self,
		product_id: ProductId,
	) -> Option<Arc<NativeWorkerExecution>> {
		match self.workers.registry()?.entries.get(&product_id) {
			Some(Entry::Running { consumer, .. }) => Some(consumer.clone()),
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
		if let Some(mut registry) = self.workers.registry() {
			sender
				.unbounded_send(registry.state(&product_id))
				.expect("worker observation is open");
			if !registry.closed {
				registry.observers.register(product_id, sender);
			}
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
	pub fn shutdown_product_workers(&self) {
		if let Some(mut registry) = self.workers.registry() &&
			registry.webviews.is_none()
		{
			registry.closed = true;
			registry.observers = Subscribers::default();
		}
		self.runtime.worker_ledger().shutdown();
	}
}
