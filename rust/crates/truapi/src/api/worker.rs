//! Unified [`Worker`] trait.

use crate::versioned::worker::{
    HostWorkerBeginOperationError, HostWorkerBeginOperationRequest,
    HostWorkerBeginOperationResponse, HostWorkerEndOperationError, HostWorkerEndOperationRequest,
    HostWorkerEndOperationResponse,
};
use crate::{CallContext, CallError};
use crate::{wasm_env, wire, wire_trait};

/// Worker background-operation APIs.
///
/// The host keeps a product's worker running while it holds at least one open
/// operation, which is how a worker outlives the surface that started it.
#[wasm_env]
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
}
