//! Unified [`ResourceAllocation`] trait.

use crate::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use crate::{CallContext, CallError};
use crate::{wasm_env, wire, wire_trait};

/// Resource pre-allocation (allowance management).
#[wasm_env]
#[wire_trait(id = 12)]
#[crate::async_trait]
pub trait ResourceAllocation: Send + Sync {
    /// Request the host to pre-allocate one or more resources.
    ///
    /// ```ts
    /// const result = await truapi.resourceAllocation.request({
    ///   resources: [
    ///     { tag: "StatementStoreAllowance" },
    ///     { tag: "BulletinAllowance" },
    ///     { tag: "SmartContractAllowance", value: { tag: "Index", value: 0 } },
    ///     { tag: "AutoSigning" },
    ///   ],
    /// });
    /// assert(result.isOk(), "request failed:", result);
    /// assert(result.value.outcomes.length === 4, "missing allocation outcomes:", result.value);
    /// // Statement Store and Bulletin back this example's storage APIs.
    /// assert(
    ///   result.value.outcomes.slice(0, 2).every((outcome) => outcome === "Allocated"),
    ///   "statement-store or bulletin allowance was not allocated:",
    ///   result.value,
    /// );
    /// // Smart-contract allowance and auto-signing are host capabilities:
    /// // unsupported hosts report NotAvailable rather than rejecting the request.
    /// assert(
    ///   result.value.outcomes.slice(2).every((outcome) => outcome !== "Rejected"),
    ///   "an optional allocation was rejected:",
    ///   result.value,
    /// );
    /// console.log("resource allocation outcomes:", result.value.outcomes);
    /// ```
    #[wire(id = 0)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        Err(CallError::unavailable())
    }
}
