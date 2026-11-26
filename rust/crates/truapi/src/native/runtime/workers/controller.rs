//! Carries the worker ledger's starts and stops to the session's worker engine.

use std::sync::{Arc, Weak};

use super::{attempt::StartedAttempt, session::Session};
use crate::{
	host_logic::worker::WorkerController, native::runtime::NativeTrUApiHostRuntime,
	platform::ProductId,
};

/// The worker ledger's controller for a native runtime.
///
/// Turns each start the ledger requests into an attempt on the session's worker engine, and each
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
		let (previous, started) = {
			let Some(mut session) = Session::lock(&runtime) else { return };
			if !runtime.runtime.worker_ledger().is_current(product_id, id) {
				return;
			}
			let started = Arc::new(StartedAttempt::new(
				session.worker_engine.clone(),
				product_id.clone(),
				id,
				self.0.clone(),
			));
			let timeout = started.deadline(&runtime, session.startup_deadline);
			// A stop racing startup must reach the engine after its start call returns.
			let previous = session.registry.begin(product_id, timeout, started.clone());
			(previous, started)
		};
		if let Some(previous) = previous {
			previous.stop();
		}
		started.start();
	}

	fn stop_worker(&self, product_id: &ProductId) {
		let Some(runtime) = self.0.upgrade() else { return };
		let removed =
			Session::lock(&runtime).and_then(|mut session| session.registry.end(product_id));
		if let Some(removed) = removed {
			removed.stop();
		}
	}

	fn worker_crashing(&self, product_id: &ProductId, crashes: u32) {
		if let Some(callbacks) = self
			.0
			.upgrade()
			.and_then(|runtime| Some(Session::lock(&runtime)?.callbacks.clone()))
		{
			callbacks.worker_crashing(product_id.clone(), crashes);
		}
	}
}
