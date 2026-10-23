---
title: "Product storage subscriptions and worker pending operations"
owner: "Sergey Zhuravlev"
status: draft
---

# RFC — Product storage subscriptions and worker pending operations

|                 |                                          |
| --------------- | ---------------------------------------- |
| **Start Date**  | 2026-08-25                               |
| **Description** | Two TrUAPI additions so a background worker can finish a multi-step task and coordinate with the app through storage. |
| **Authors**     | Sergey Zhuravlev                         |

## Summary

- `localStorage.subscribe(key)` streams a key's value on every change, within the product's own namespace.
- `worker.beginOperation()` / `worker.endOperation(id)` declare a pending operation. The core keeps the worker running while any operation is open, across restarts.

## Motivation

A funding operation, part of a safety-net release, builds a transaction, submits it, waits for inclusion and records the result. Steps hit a backend, so one run takes tens of seconds with polling in between. It runs in a worker so it outlives the product's screen. Two things make that unsafe today.

The host disposes a worker once its on-screen surface is gone (the worker's own `dispose` in `worker-runtime.ts` is a no-op that defers to the main thread), and the in-flight submission dies with it. The product has no way to say it is mid-operation.

The screen and the worker are separate runtimes over one storage namespace, and a write in one is invisible to the other until it re-reads. A progress view fed by the worker can only poll.

## Detailed Design

### localStorage.subscribe

On the `LocalStorage` trait next to `read`, `write` and `clear`:

```rust
async fn subscribe(
    &self,
    cx: &CallContext,
    request: HostLocalStorageSubscribeRequest, // { key: String }
) -> Subscription<HostLocalStorageChangeItem, CallError<GenericError>>;

pub struct HostLocalStorageChangeItem {
    /// `Some` on write, `None` after clear.
    pub value: Option<Vec<u8>>,
}
```

The host owns the store both runtimes write to, so the host emits the changes and the core forwards its stream to the subscriber. A write a product made through TrUAPI is emitted by the core that performed it, so a host reports only the changes it makes itself. Subscriptions are scoped to the product's namespace rather than to one execution, which is what lets a worker's write reach the screen. This adds one method to the required host `ProductStorage` trait:

```rust
fn subscribe_storage(
    &self,
    key: String,
) -> BoxStream<'static, Result<HostLocalStorageChangeItem, GenericError>>;
```

The core namespaces the key before calling, exactly as it does for `read`, `write` and `clear`, so a product only sees its own keys and the host needs no separate product argument. The first item is the current value, so there is no read-then-subscribe gap; a change landing while that first read is in flight repeats rather than being dropped. After that a write emits `Some(value)` and a clear emits `None`. A burst of distinct values emits one item per value; nothing coalesces.

A host stream that fails ends the subscription with `CallError::HostFailure`, so a product learns its view is stale instead of holding the last value it saw forever.

If `write` gets the bytes the key already holds, the core skips the store write, so the host never sees it and nothing is emitted. `clear` always reaches the host and always emits `None`, even on an absent key.

### Pending operations

A `begin`/`end` pair on a new `Worker` trait, gated to the Worker execution kind:

```rust
async fn begin_operation(
    &self,
    cx: &CallContext,
    request: HostWorkerBeginOperationRequest, // { label: Option<String> } for host logs and UI
) -> Result<HostWorkerBeginOperationResponse, CallError<HostWorkerOperationError>>;

/// Idempotent: an unknown or already-ended id returns `Ok`.
async fn end_operation(
    &self,
    cx: &CallContext,
    request: HostWorkerEndOperationRequest, // { id: OperationId }
) -> Result<HostWorkerEndOperationResponse, CallError<HostWorkerOperationError>>;

/// Core-assigned, unique per product across restarts.
pub type OperationId = u32;

pub struct HostWorkerBeginOperationResponse {
    pub id: OperationId,
}

pub enum HostWorkerOperationError {
    /// The product is at the per-product cap of open operations.
    /// `end` never returns this.
    TooManyOpen,
    Unknown { reason: String },
}
```

The core stores open operations, so an operation survives a restart and starts its worker again at the next launch. Ids stay unique per product across restarts. The product keeps each id in host storage and ends the operation with it, even after a restart. An operation still open after 24 hours expires, so one the product lost or forgot cannot keep its worker running.

The core owns operations, as it owns the rest of the [Worker Lifecycle](worker-lifecycle.md). It assigns ids and stores open operations, so no host implements them.

An open operation is a reference on the product's worker, like a chat room or a Pocket card. The reference is dropped when the operation ends or expires.

Operations are keyed by id and scoped to the product. Two tasks each begin one and the worker stays alive until both end. Ending an id twice, or one that was never begun, releases nothing.

Keep-alive is best-effort. iOS and Android can suspend or kill a backgrounded app, and its workers with it. Its open operations start the worker again at the next launch, and it resumes from saved state.

An operation is opaque: an optional label, no funding or deposit typing.

## Compatibility

All three wire methods are additive. `subscribe_storage` is a required host capability. Target is v0.2 / latest.

## Future directions

A host UI listing active operations with `status`, a `list_operations` read to feed it, and user cancel. A prefix subscription if products want to watch a set of keys.
