---
title: "Core-Owned Workers: When a Product Worker Runs and Who Decides"
type: design
status: draft
author: pgherveou
created: 2026-10-02
---

# Core-Owned Workers: When a Product Worker Runs and Who Decides

_Starts with the iOS host (`hosts/ios`) with the TrUAPI runtime on. The core API is platform neutral, so Android follows the same contract._

## Summary

**The Rust core decides which product workers run, records why in its SQLite database, and keeps their bundles up to date. The host app only reports what the user did and runs the JavaScript engine the core asks for.**

- The app sends one kind of message to the core: a worker intent, such as "the user opened this product's chat" or "the user added this card".
- The core turns intents into durable rows in a `product_workers` table and decides from that table which workers run.
- The core asks the host to start or stop an engine for a product. The host owns no lifecycle state of its own.
- A worker with a durable reason (an installed chat, an added card) runs from app launch until that reason is removed. Navigating away from the chat or Pocket screen does not stop it.
- One product has one worker, however many reasons it has.

```
  Swift app (hosts/ios)                         Rust core (truapi runtime)
 +-----------------------------+               +------------------------------------+
 | Chat screen, Pocket screen, |  intents      | Worker supervisor                  |
 | SPA screen                  |-------------->|   reasons + transient references   |
 |                             |               |   decides: run / stop / restart    |
 |                             |               |                                    |
 | Engine pool                 | start(bundle) |   core.sqlite3                     |
 |   hidden WKWebView per      |<--------------|     product_workers                |
 |   running worker            | stop          |     product_worker_reasons         |
 |                             |-------------->|                                    |
 |                             | exited(error) | Bundle store                       |
 +-----------------------------+               |   resolve dotNS, fetch, refresh    |
                                               +------------------------------------+
```

## Why now

The host and the core both make worker decisions today, and on iOS the core's decisions are ignored.

| Today                                                                                 | Consequence                                                    |
| ------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| iOS has two worker paths chosen by a debug flag, native and TrUAPI                    | Lifecycle rules exist twice and drift                          |
| With TrUAPI on, each chat bot opens its own Worker execution and its own WKWebView    | The core's reference counter (`WorkerLedger`) is unused on iOS |
| Which products are "installed" lives in CoreData (`CDProduct`, `CDProductOperation`)  | Android stores the same facts in Room, with its own rules      |
| Swift resolves dotNS, downloads bundles and caches them; the core resolves manifests  | Two resolvers, two caches, no shared update policy             |
| Android runs the core counter for Pocket and a native counter for Chat and operations | Two counters on one platform, split by modality                |

Sources: `hosts/ios/polkadot-app/Modules/Products/ProductBotFactory.swift`, `hosts/ios/polkadot-app/Modules/Products/Worker/ProductWorkerManager.swift`, `hosts/ios/polkadot-app/Modules/Products/Chat/Rust/ChatRustRuntime.swift`, `rust/crates/truapi/src/host_logic/worker.rs`, `hosts/android/.../truapi/worker/TrUAPIWorkerSupervisor.kt`.

## Responsibilities

### Rust core

- Owns the worker table and every rule that reads or writes it.
- Decides when a worker starts, stops and restarts.
- Opens the Worker execution for the product and hands the host what it needs to run it: the execution, the local bundle path and the bootstrap script.
- Decides when a product's bundle is fetched or checked for a newer version, and records which content hash is installed. The fetch itself goes through a host callback (see [Updates](#updates)).
- Restarts a worker whose engine died, with backoff. The core sees this when the execution's ws-bridge connection drops, so the host reports nothing.

### Host app (Swift)

- Sends intents when the user acts. It never persists which workers exist.
- Runs an engine when asked: a hidden WKWebView attached to the window so its timers are not throttled, connected to the execution's ws-bridge.
- Fetches a worker bundle when the core asks: dotNS content hash, CAR download from IPFS, unpack to a local directory.
- Forwards app lifecycle (foreground, background) as it does today.

The host keeps no `ProductWorkerManager`, `ProductWorkerOperationReconciler` or `CDProduct` on the TrUAPI path.

## Rules

A worker runs while it has at least one **reason** (durable, stored in SQLite) or at least one **transient reference** (in memory, held while work is in flight). Both feed the same per-product decision, so a product never gets two workers.

### Durable reasons

| Rule | Trigger (intent from the host)                            | Effect                                                                  |
| ---- | --------------------------------------------------------- | ----------------------------------------------------------------------- |
| R1   | User opens the chat of a product from its SPA, first time | Add reason `chat`, start the worker, start it on every later launch     |
| R2   | User adds a Pocket card                                   | Add reason `pocket(card_id)`, start the worker, start on later launches |
| R3   | User removes the chat or the last card of a product       | Remove that reason; stop the worker if nothing else holds it            |
| R4   | App launch with the runtime on                            | Start every worker that has a reason                                    |
| R5   | Account signs out                                         | Stop every worker; the rows stay in that account's database             |

### Transient references

These already exist in the core and stay as they are:

- An open render stream (Pocket face, chat render) holds a reference while it is open.
- A worker operation (`worker.begin_operation` until it ends) holds a reference. Operations are stored in the core database so they survive a restart, which replaces `CDProductOperation`.

### What does not start a worker

- Opening a product's SPA (App executable). The App has no worker reference, as in [Worker Lifecycle](../rfcs/worker-lifecycle.md).
- Navigating to or away from the Chat or Pocket screen. Workers with a durable reason stay running, so their state is hot when the user returns.

### Sharing

The table is keyed by product id (dotNS base name). Reasons are rows under that key, so a product that is both a chat bot and a Pocket card has one row in `product_workers`, two rows in `product_worker_reasons`, and one running worker. The core already enforces one Worker execution per product (`rust/crates/truapi/src/native/runtime.rs`, the `worker_executions` map); the supervisor makes that the only path.

## Storage

Three tables in `core.sqlite3`, added as the first entries of `core_migrations()` in `rust/crates/truapi/src/store.rs`.

```sql
CREATE TABLE product_workers (
    product_id        TEXT PRIMARY KEY,   -- dotNS base name, e.g. "chess.dot"
    content_hash      BLOB NOT NULL,      -- worker bundle currently installed
    pending_hash      BLOB,               -- newer bundle downloaded, not yet applied
    manifest          BLOB NOT NULL,      -- encoded manifest the bundle was resolved from
    checked_at        INTEGER NOT NULL,   -- last update check, unix seconds
    failure_count     INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT
);

CREATE TABLE product_worker_reasons (
    product_id  TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,            -- 'chat' | 'pocket'
    card_id     TEXT NOT NULL DEFAULT '', -- empty for 'chat'
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (product_id, kind, card_id)
);

CREATE TABLE product_worker_operations (
    product_id    TEXT NOT NULL REFERENCES product_workers(product_id) ON DELETE CASCADE,
    operation_id  INTEGER NOT NULL,
    label         TEXT,
    started_at    INTEGER NOT NULL,
    PRIMARY KEY (product_id, operation_id)
);
```

A `product_workers` row with no reason and no operation is deleted. Whether the worker runs is derived from the reason and operation rows plus the in-memory references, never stored, so there is no `enabled` flag to keep in sync.

Bundle bytes stay where Swift keeps them today, addressed by content hash, as [Product Manifest](product-manifest.md) already specifies (bytes cached forever by CID). The core stores only the hash.

The database is per account: the host gives each account its own `database_directory`, so signing in as another account opens other rows and switching back restores them. Today the runtime is built once per process with a single directory (`Application Support/truapi`), so this needs the runtime to be rebuilt, or its database reopened, on account switch.

### Mapping from the iOS host

What `hosts/ios` stores today (CoreData `UserDataModel52` and UserDefaults) and where it goes:

| iOS today                                                                                                             | Core table                                                                 |
| --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `CDProduct.identifier`, the installed chat products                                                                   | `product_workers.product_id` plus a `chat` row in `product_worker_reasons` |
| `CDProduct.name`                                                                                                      | Not stored; the display name comes from the manifest                       |
| Nothing, iOS has no Pocket store                                                                                      | `pocket` rows in `product_worker_reasons`                                  |
| `CDProductOperation` (`identifier` = `"<productId>-<operationId>"`, `productId`, `operationId`, `label`, `startedAt`) | `product_worker_operations`, keyed by `(product_id, operation_id)`         |
| UserDefaults suite `io.products.dotns.cache`, name to content hash                                                    | `product_workers.content_hash`, for worker bundles only                    |
| Files under `Application Support/DotNsContent/<hash>`                                                                 | Unchanged, still Swift owned                                               |
| Nothing                                                                                                               | `pending_hash`, `checked_at`, `failure_count`, `last_error`                |

The iOS model has no record of why a product is installed beyond "it has a chat", no update state and no failure state. Everything else maps one to one.

The experiment creates its own account-scoped schema. It does not import or delete legacy worker records. New user actions create durable reasons and operations through Rust.

## Updates

- The core checks a product's content hash at launch and when `checked_at` is older than a fixed interval, the same 24 hour bound the core uses for manifests (`rust/crates/truapi/src/runtime/product_manifest.rs`).
- The download goes through a host callback, `fetch_worker_bundle`, so Swift keeps its dotNS and IPFS code. The core calls it, stores the returned hash as `pending_hash`, and owns every decision around it. Moving the fetch into the core later changes only who implements the callback.
- Once the new bundle is on disk, the core restarts the worker onto it. A worker with an operation in flight restarts when the operation ends.
- `appVersion` is not a change signal; the content hash is.

## Interface sketch

Host to core, one entry point:

```rust
pub enum WorkerIntentAction {
    Add,
    Remove,
}

pub enum WorkerModality {
    Chat,
    /// A product can have several cards; the worker runs while any of them remains.
    Card { card_id: String },
}

impl NativeTrUApiHostRuntime {
    /// Records the intent and starts or stops workers to match.
    pub fn notify_worker_intent(&self, product_id: String, action: WorkerIntentAction, modality: WorkerModality);
}
```

Engine loss needs no entry point: the core already disposes an execution when its ws-bridge connection closes (`rust/crates/truapi/src/native/ws_bridge.rs`). One case to handle: a socket dropped because iOS suspended the app must not count as a crash. The bridge relistens on foreground today, and the supervisor restarts workers then.

Core to host, replacing `worker_demand_changed`:

```rust
pub trait WorkerEngineHost {
    /// Run the worker bundle against this execution until told to stop.
    fn start_worker(&self, execution: Arc<NativeProductExecution>, bundle: WorkerBundle);
    /// Tear down the engine for this product.
    fn stop_worker(&self, product_id: String);
    /// Download the product's current worker bundle and return its content hash and local path.
    async fn fetch_worker_bundle(&self, product_id: String) -> Result<WorkerBundle, HostRejection>;
}
```

`acquire_worker` and `release_worker` stop being host API on native hosts: transient references are taken by the core itself (render streams, operations), and durable ones come from intents.

## Comparison with the legacy app

The legacy app is `polkadot-ios-community`; `hosts/ios` is an import of it (`hosts/imports.json`), so its native path is the same code.

| Concern                | Legacy app (native path)                                                                                       | Legacy app (TrUAPI path)                                     | This design                                                   |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------- |
| Who decides            | Swift `ProductWorkerManager`, reference counted per product                                                    | Each chat bot opens its own execution; no counter            | Rust core supervisor                                          |
| What persists          | CoreData `CDProduct` (installed chat products), `CDProductOperation`                                           | Same CoreData rows                                           | Core SQLite tables above                                      |
| Start triggers         | Every installed chat product starts at main tab bar setup; SPA screen takes a reference while open; operations | Bot start at main tab bar setup                              | Durable reasons at launch and on intent; transient references |
| Chat opened from SPA   | Writes `CDProduct`, which creates a bot and starts it                                                          | Same                                                         | Intent `Add` + `Chat`, reason `chat`                          |
| Pocket                 | Not implemented, `includes.pocket` is parsed and ignored                                                       | Not implemented                                              | Reason `pocket(card_id)`                                      |
| Navigating away        | Chat worker keeps running; SPA screen releases its reference                                                   | Chat worker keeps running                                    | Workers with a reason keep running; SPA holds none            |
| Sharing                | One worker per product id                                                                                      | Not enforced by the host; core closes the previous execution | One worker per product id, enforced in one place              |
| Bundle fetch and cache | Swift: dotNS content hash read on every resolve, CAR from IPFS gateway, `DotNsContent/<hash>`                  | Same Swift code                                              | Same Swift code, called by the core when it decides           |
| Updates                | No version check; new hash applies at next worker start; running worker never restarted                        | Same                                                         | Periodic check, restart onto the new bundle when idle         |
| Engine crash           | Logged, engine stays in error, no restart                                                                      | Same                                                         | Core sees the bridge drop and restarts with backoff           |
| Logout                 | No explicit teardown found; operation rows have no clearing caller                                             | Same                                                         | Stop all; rows stay in that account's database                |
| Limits                 | No cap, no eviction, no background execution                                                                   | Same                                                         | No cap; a lighter engine (QuickJS) comes later                |

Legacy references: `polkadot-app/Modules/Products/Worker/ProductWorkerManager.swift`, `Modules/SPA/SPANativeRuntimeInteractor.swift`, `Modules/Chat/ChatExtension/ChatExtensionRegistring.swift`, `Modules/Products/Worker/ProductWorkerOperationReconciler.swift`, `Packages/Products/Sources/Products/DotNs/`.

The legacy rules on start triggers and sharing already match the direction here. What changes is where they live, that they cover Pocket, and that updates and crashes get a policy.

## Relationship to the Worker Lifecycle RFC

[Worker Lifecycle](../rfcs/worker-lifecycle.md) says a worker runs only while referenced, references last only while work is on screen or in flight, and the host may stop an unreferenced worker at any time. It lists an always-on worker as considered and dropped.

This design keeps its contract for products: one worker, products must still not rely on staying warm, state that must survive goes through host storage, nothing is added to the protocol. It changes the host policy: an installed chat or an added card is a durable reference, so those workers run for the whole session. The RFC's reference table needs that row added, or this design needs to say it overrides the RFC.

## Decisions

- **No resource cap for now.** Each worker is a hidden WKWebView with its own WebContent process. The cost is accepted until workers move to QuickJS. With QuickJS the core could run the engine itself, and `start_worker` and `stop_worker` would go away.
- **Bundle fetching stays in Swift** behind the `fetch_worker_bundle` callback.
- **Updates restart the worker** as soon as the new bundle is downloaded, unless an operation is in flight.
- **One database per account**, chosen by the directory the host passes.
- **Removing a chat (R3) has no UI yet.** The intent exists; the screen that sends it comes later.

## Open questions

1. **Background.** iOS suspends the app, and the engines with it. "Hot" therefore means hot while the app is alive. Is that enough, or do some products need background time?
