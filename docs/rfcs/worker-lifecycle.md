---
title: "Worker Lifecycle"
owner: "@johnthecat"
status: draft
---

# RFC — Worker Lifecycle

## Summary

The Rust core:

- Starts and terminates workers, and records in its database why each one runs.
- Polls for new versions of each running worker's code, and restarts the worker when a new version is downloaded.
- Runs each worker in a QuickJS sandbox instead of asking the host to start the worker in a hidden web view.

The host now simply signals intent to start or stop a worker, everything else is managed by the Rust core.

## Motivation

Each host implements worker management on its own:

- It decides when workers run, with its own counters and database.
- It resolves and caches worker code, with its own update rules.
- It runs each worker in a hidden WebView, attached to the window so its timers keep running, at the cost of one web content process per worker.

## Approach

### References

A worker runs while at least one reference holds it. Several references of one product hold a single worker.
App and Widget executables hold no reference; their lifetime is their screen.

References and how long each lasts:

| Reference        | Held while                                                                                                                   | Stored |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------ |
| Chat room        | From the first chat room the user opens with the product until its last room is removed                                      | Yes    |
| Added card       | From adding a Pocket card until the product's last card is removed                                                           | Yes    |
| Worker operation | From `worker.begin_operation` until `worker.end_operation`                                                                   | Yes    |
| Modality request | A request that needs the product's worker, such as a funding flow or an input round, is in flight or its result is on screen | No     |

- The core starts every worker with a stored reference when the runtime starts.
- Leaving the Chat or Pocket screen releases no stored reference, so the worker keeps its state.
- A worker operation is how a worker keeps itself running for work that must finish off screen, such as a payment settling.
  [Pending operations](storage-subscriptions-pending-operations.md) specifies how operations survive a restart and when
  they expire.
- When nothing holds a worker, the core stops it. Products must not rely on staying warm: state that must survive goes through host storage.
- Signing out stops every worker and clears the tables below.

### Storage

The core keeps stored references, and the bundle each of their products runs, in three SQLite tables.
A product's row is added with its first stored reference and removed with its last.

```
 product_workers                       product_worker_surfaces
+------------------------+  1     *  +------------------------+
| product_id         PK  |---------->| product_id         FK  |
| content_hash           |           | kind   chat | pocket   |
|                        |           | item_id  room or card  |
|                        |           +------------------------+
|                        |  1     *  product_worker_operations
|                        |---------->+------------------------+
+------------------------+           | product_id         FK  |
                                     | operation_id           |
                                     | started_at             |
                                     +------------------------+
```

<details>
<summary>SQL schema</summary>

```sql
CREATE TABLE product_workers (
    product_id    TEXT PRIMARY KEY,   -- dotNS base name, e.g. "chess.dot"
    content_hash  BLOB NOT NULL       -- worker bundle installed
);

CREATE TABLE product_worker_surfaces (
    product_id  TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,             -- 'chat' | 'pocket'
    item_id     TEXT NOT NULL,             -- chat room id or card id
    PRIMARY KEY (product_id, kind, item_id)
);

CREATE TABLE product_worker_operations (
    product_id    TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    operation_id  INTEGER NOT NULL,
    started_at    INTEGER NOT NULL,   -- unix seconds, for the operation expiry
    PRIMARY KEY (product_id, operation_id)
);
```

</details>

- Bundle bytes stay in content-addressed files outside the database, as [Product Manifest](product-manifest.md)
  specifies. The core stores the content hash.

### Updates

- The core polls the dotNS `contenthash` of every running worker when the runtime starts, at a given interval (tbd), and compares it with `content_hash`.
- A changed hash is downloaded through the host app's `fetch_worker_bundle` callback.
- The core then offers the new bundle to the worker through `worker.accept_new_update`, (see [Product interface](#product-interface)).

### Sandbox

Every worker runs in its own QuickJS sandbox, with heap and stack limits; repeated failures restart it with backoff.

The environment is the one the web host's worker sandbox
([`host-worker-sandbox`](https://github.com/paritytech/triangle-js-sdks/tree/main/packages/host-worker-sandbox/src))
already provides, so one bundle runs on every host.

On native hosts, workers need no localhost bridge: the sandbox exposes a global `truapi` object, a Rust type declared
with [`#[rquickjs::class]`](https://docs.rs/rquickjs/latest/rquickjs/class/trait.JsClass.html) whose methods reach the
core in process.

Native hosts implement the sandbox in a new crate around rquickjs and the LLRT modules, which exposes the sandbox runtime to the core.

### Host interface

#### Host to core

`notify_worker_intent` tells the core that the user added or removed one of a product's chat rooms or cards.
The core updates the product's stored references and starts or stops its worker to match.

```rust
impl NativeTrUApiHostRuntime {
    /// Records the intent and starts or stops workers to match.
    pub fn notify_worker_intent(&self, product_id: String, action: WorkerIntentAction, modality: WorkerModality);
}
```

<details>
<summary>Intent types</summary>

```rust
/// What the user did to a product's chat rooms or cards.
pub enum WorkerIntentAction {
    Add,
    Remove,
}

/// The surface the intent is about.
pub enum WorkerModality {
    Chat { room_id: String },
    Card { card_id: String },
}

```

</details>

#### Core to host

`fetch_worker_bundle` downloads a product's worker bundle. The download stays in the host app for now and the core calls
it over FFI, so the app can run it as a background task that keeps going when the app is backgrounded.

```rust
/// Host services the core uses to keep workers current.
#[async_trait]
pub trait WorkerHost: Send + Sync {
    /// Downloads the product's current worker bundle and returns its content hash and local directory.
    async fn fetch_worker_bundle(&self, product_id: String) -> Result<WorkerBundle, HostRejection>;
}
```

> **Note:** implementing this RFC removes `worker_demand_changed`, `acquire_worker` and `release_worker` from the host
> API, since the core starts and stops workers itself, from the references it holds.

### Product interface

One host-initiated method `accept_new_update` tells a running worker that a newer version of its code is ready,
and gives it time to finish its work before the core reloads it.

```rust
#[wire_trait(id = 19)]
pub trait Worker: Send + Sync {
    /// Offers the worker a newer bundle. The worker replies once it is ready to be reloaded on it.
    #[wire(host_initiated, id = 2)]
    async fn accept_new_update(
        &self,
        cx: &CallContext,
        request: ProductWorkerAcceptNewUpdateRequest,
    ) -> Result<ProductWorkerAcceptNewUpdateResponse, CallError<ProductWorkerAcceptNewUpdateError>>;
}
```

- The core calls `accept_new_update` on the running worker once the new bundle is downloaded.
- Until the worker replies, it keeps running its current bundle, so it can finish or save work in flight first.
- When the worker replies, returns an error, or does not reply before the request times out, the core stops it and
  starts it again on the new bundle, whose hash becomes `content_hash`.

## Trade-offs

- An installed worker runs for the whole session, which costs memory for every installed product even when idle.
- QuickJS interprets without a JIT, so CPU-bound worker code runs slower than in a WebView.
- Bundles that need the DOM or `WebAssembly` do not run.

## Rollout

All of it ships behind the TrUAPI runtime flag. With the flag off, hosts keep their current worker paths.

