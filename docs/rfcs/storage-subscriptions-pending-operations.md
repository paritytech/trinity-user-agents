---
title: "Product storage subscriptions"
owner: "Sergey Zhuravlev"
status: draft
---

# RFC — Product storage subscriptions

|                 |                                          |
| --------------- | ---------------------------------------- |
| **Start Date**  | 2026-08-25                               |
| **Description** | Apps and workers observe updates to their shared product storage. |
| **Authors**     | Sergey Zhuravlev                         |

## Motivation

An app and its worker share a product storage namespace. Subscribing to a key lets the app show progress as the worker records it, including an initial value when the screen reopens.

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
