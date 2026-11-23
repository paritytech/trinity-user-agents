---
title: "Product storage subscriptions and worker references"
owner: "Sergey Zhuravlev"
status: draft
---

# RFC — Product storage subscriptions and worker references

|                 |                                          |
| --------------- | ---------------------------------------- |
| **Start Date**  | 2026-08-25                               |
| **Description** | Two TrUAPI additions so a background worker can finish a multi-step task and coordinate with the app through storage. |
| **Authors**     | Sergey Zhuravlev                         |

## Summary

- `localStorage.subscribe(key)` streams a key's value on every change, within the product's own namespace.
- `worker.retain()` returns a reference on the running worker. Release it when the work finishes.

## Motivation

A funding operation, part of a safety-net release, builds a transaction, submits it, waits for inclusion and records the result. Steps hit a backend, so one run takes tens of seconds with polling in between. It runs in a worker so it outlives the product's screen. Two things make that unsafe today.

When the screen holds the last reference, closing it can stop the worker during submission. The worker needs a way to hold a reference while it finishes its task.

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

### Worker references

A worker can take a reference on its own execution before the last screen or other host consumer leaves. Rust increments the session's existing count for the authenticated product. The guest can finish work while the screen is closed. Each reference has one owner.

```ts
const claim = await truapi.worker.retain();
try {
  await submitAndRecord();
} finally {
  claim.release();
}
```

On the wire, `worker.retain_reference` is a Worker-only subscription at (19, 2). The host sends one acknowledgement after incrementing the count. A Stop releases that subscription's reference. The SDK's `retain({ signal })` also sends Stop if the caller aborts before acknowledgement. Disconnect releases every reference acquired by that connection. Each connection can hold up to 64 guest references. The next acquisition fails with `TooManyReferences`. A worker can lose its execution when the host or operating system ends its session. Work that must survive process death needs durable state and a host owner for recovery.

## Compatibility

The previous `worker.beginOperation` and `worker.endOperation` wire addresses (19, 0) and (19, 1) are retired. Hosts and products must support (19, 2) to use explicit guest references. A host SDK upgrade removes the product-operation callback group. The legacy mobile Nova runtime uses a different protocol; only that runtime restores its own persisted operation records.
