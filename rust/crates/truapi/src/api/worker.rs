//! Explicit references held by the running product worker.

use crate::{
	CallContext, CallError, Subscription,
	versioned::worker::{
		HostWorkerRetainReferenceError, HostWorkerRetainReferenceItem,
		HostWorkerRetainReferenceRequest,
	},
	wire, wire_trait,
};

/// Holds on a worker's own execution.
#[wire_trait(id = 19)]
#[crate::service(required_execution = Worker)]
#[async_trait::async_trait]
pub trait Worker: Send + Sync {
	/// Retain the worker until the caller releases its reference or disconnects.
	///
	/// The first item acknowledges acquisition. Stopping this subscription releases
	/// just this reference, even if its acknowledgement has not arrived yet.
	///
	/// ```ts
	/// const hold = await truapi.worker.retain();
	/// try {
	///   await truapi.system.info();
	/// } finally {
	///   hold.release();
	/// }
	/// ```
	#[wire(id = 2)]
	async fn retain_reference(
		&self,
		_cx: &CallContext,
		_request: HostWorkerRetainReferenceRequest,
	) -> Subscription<HostWorkerRetainReferenceItem, CallError<HostWorkerRetainReferenceError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
