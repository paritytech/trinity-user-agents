//! What a runtime asks of the engine that runs its workers.

use std::sync::Arc;

use super::attempt::NativeWorkerAttempt;
use crate::platform::ProductId;

/// Runs a runtime's workers: starts each attempt and disposes it.
///
/// Calls arrive on whichever thread changed worker demand or reported a failure, which can be the
/// thread constructing the runtime or one of the core's own. Return without blocking: the starts
/// and stops of every product wait behind the call. The core holds no lock meanwhile, so an
/// implementation may call back into the runtime or the attempt.
#[uniffi::export(rust, foreign)]
pub trait NativeWorkerEngine: Send + Sync {
	/// Begin booting the product, reporting module start or failure through this attempt.
	///
	/// The core fails an attempt that is not running 90 seconds after its start, or after it
	/// lost its connection.
	fn start(&self, product_id: ProductId, attempt: Arc<NativeWorkerAttempt>);

	/// Dispose only the specified attempt, even when startup is incomplete.
	///
	/// Arrives once for each started attempt, after its [`Self::start`] call has returned,
	/// including after a failure the attempt reported. It can follow the start of the product's
	/// next attempt.
	fn stop(&self, product_id: ProductId, attempt_id: u64);
}
