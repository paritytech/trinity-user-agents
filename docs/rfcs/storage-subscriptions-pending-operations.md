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
- `worker.beginOperation()` / `worker.endOperation(id)` declare a pending operation. The host keeps the worker running while any operation is open.

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

/// Host-assigned, unique per product. Like `NotificationId`.
pub type OperationId = u32;

pub struct HostWorkerBeginOperationResponse {
    pub id: OperationId,
}

pub enum HostWorkerOperationError {
    /// The product is at the host's per-product cap of open operations.
    /// `end` never returns this.
    TooManyOpen,
    Unknown { reason: String },
}
```

The id is session-scoped and never persisted. No host stores operations, so if the host dies the worker, the id and the operation die together and the next launch starts with none open.

The host owns operations and the lifecycle. The core forwards each call to a host trait scoped to the calling product, and the operation existing is the keep-alive signal. Every host implements the trait; one with no worker lifecycle, such as the headless CLI, hands out ids and tracks nothing.

An open operation is also demand on the worker: the core takes one reference on the product's worker ledger per open id, so an operation reaches the host through the same `workerDemandChanged` signal an on-screen surface produces, and a host reading that signal needs no second keep-alive rule. The reference is dropped when the operation ends, and a connection that goes away with operations still open drops the ones it held.

```rust
#[async_trait]
pub trait ProductOperations: Send + Sync {
    /// `label` is empty when the product gave none.
    async fn begin_operation(
        &self,
        product: &ProductContext,
        label: String,
    ) -> Result<HostWorkerBeginOperationResponse, HostWorkerOperationError>;

    /// Idempotent.
    async fn end_operation(
        &self,
        product: &ProductContext,
        id: OperationId,
    ) -> Result<(), HostWorkerOperationError>;
}
```

Operations are keyed by id and scoped to the product. Two tasks each begin one and the worker stays alive until both end. Ending an id twice, or one that was never begun, releases nothing.

Keep-alive is best-effort. iOS and Android can kill a backgrounded worker. An open operation lets the host request the background time the platform allows (a background task assertion on iOS, a foreground service or WorkManager on Android), and the worker resumes from saved state after a kill.

An operation is opaque: an optional label, no funding or deposit typing.

## Compatibility

All three wire methods are additive. `subscribe_storage` and `ProductOperations` are required host capabilities. Target is v0.2 / latest.

## Future directions

Operations become one term of a general liveness rule: `can_execute = has pending operations || has chats || has pocket cards || ...`. On top of that, a host UI listing active operations with `status`, a `list_operations` read to feed it, and user cancel. A prefix subscription if products want to watch a set of keys.
