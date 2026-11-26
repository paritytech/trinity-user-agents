//! What following a product's worker reports.

use std::sync::Arc;

use crate::native::runtime::NativeWorkerExecution;

/// Availability of the product's session-owned worker.
#[derive(Clone, uniffi::Enum)]
pub enum NativeWorkerExecutionState {
	/// The session has no demand for the product, or a reload is replacing its worker.
	Idle,
	/// The worker is starting, reconnecting, or waiting for its next start after a failure.
	Pending,
	/// The module started and a live connection can accept host calls.
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
