//! Unified [`Worker`] trait.

use crate::{
	CallContext, CallError,
	versioned::worker::{
		HostWorkerBeginOperationError, HostWorkerBeginOperationRequest,
		HostWorkerBeginOperationResponse, HostWorkerEndOperationError,
		HostWorkerEndOperationRequest, HostWorkerEndOperationResponse,
		HostWorkerSetWakeWatchesError, HostWorkerSetWakeWatchesRequest,
		HostWorkerSetWakeWatchesResponse,
	},
	wire, wire_trait,
};

/// Worker background-operation APIs.
///
/// The host keeps a product's worker running while it holds at least one open
/// operation, which is how a worker outlives the surface that started it.
#[wire_trait(id = 19)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait Worker: Send + Sync {
	/// Begin a pending operation.
	///
	/// ```ts
	/// const result = await truapi.worker.beginOperation({ label: "funding" });
	/// assert(result.isOk(), "beginOperation failed:", result);
	/// console.log("operation started:", result.value.id);
	/// await truapi.worker.endOperation({ id: result.value.id });
	/// ```
	#[wire(id = 0)]
	async fn begin_operation(
		&self,
		_cx: &CallContext,
		_request: HostWorkerBeginOperationRequest,
	) -> Result<HostWorkerBeginOperationResponse, CallError<HostWorkerBeginOperationError>> {
		Err(CallError::unavailable())
	}

	/// End a pending operation. Idempotent: an unknown or already-ended id
	/// succeeds, so a retry after an ambiguous failure is safe.
	///
	/// ```ts
	/// const begun = await truapi.worker.beginOperation({});
	/// assert(begun.isOk(), "beginOperation failed:", begun);
	/// const result = await truapi.worker.endOperation({ id: begun.value.id });
	/// assert(result.isOk(), "endOperation failed:", result);
	/// console.log("operation ended");
	/// ```
	#[wire(id = 1)]
	async fn end_operation(
		&self,
		_cx: &CallContext,
		_request: HostWorkerEndOperationRequest,
	) -> Result<HostWorkerEndOperationResponse, CallError<HostWorkerEndOperationError>> {
		Err(CallError::unavailable())
	}

	/// Replace the statement-store topics whose new statements wake the
	/// worker, each group routed to a chat room of the product.
	///
	/// The host keeps the watches while the worker is stopped and passes the
	/// topics to its push relay, never to the product. A new statement on a
	/// watched topic holds the worker for a wake of 30 s and sends
	/// `ChatRoomEvent::Woken` to the room of the watch, at most 12 times in
	/// one hour. The relay learns the topics, not the statements. An empty
	/// list removes every watch. Call it again when the topics change; each
	/// call replaces the earlier watches whole.
	///
	/// ```ts
	/// const topic: `0x${string}` = `0x${"11".repeat(32)}`;
	/// const result = await truapi.worker.setWakeWatches({
	///   watches: [{ id: "inbox", roomId: "main", topics: [topic] }],
	/// });
	/// assert(result.isOk(), "setWakeWatches failed:", result);
	/// console.log("wake delivery:", result.value.delivery);
	/// ```
	#[wire(id = 2)]
	async fn set_wake_watches(
		&self,
		_cx: &CallContext,
		_request: HostWorkerSetWakeWatchesRequest,
	) -> Result<HostWorkerSetWakeWatchesResponse, CallError<HostWorkerSetWakeWatchesError>> {
		Err(CallError::unavailable())
	}
}
