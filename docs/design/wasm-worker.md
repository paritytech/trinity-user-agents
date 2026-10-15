---
title: "Wasm Product Workers"
type: design
status: experimental
created: 2026-10-08
---

# Wasm product workers

- A product can ship its worker as a Rust crate compiled to `wasm32-unknown-unknown`.
- The core runs it in process, bound to one product.
- Each TrUAPI call the worker makes is a wasm import.
- The host answers it by calling the matching method of the [service traits](../../rust/crates/truapi/src/api.rs) directly on the product's [`ProductRuntimeHost`](../../rust/crates/truapi/src/runtime.rs), with no frames or dispatcher in between.
- The calls the host starts into the product (`renderer.render`, the Chat Extension v2 queries) reach the handlers the worker registers, through a guest export. See [Calls the host starts](#calls-the-host-starts).

## Host setup

The host serves a worker with the same per-product object an iframe product talks to: [`ProductRuntimeHost`](../../rust/crates/truapi/src/runtime.rs), which implements every service trait for one product (the implementations are in [`runtime/capabilities`](../../rust/crates/truapi/src/runtime/capabilities)).

```rust
// The worker acts as `chess.dot`, running as its Worker executable.
let product = ProductContext::new_with_execution("chess.dot".to_string(), ProductExecutionKind::Worker)?;
// `runtime` is the host's `SigningHostRuntime` or `PairingHostRuntime`.
let host = runtime.product_admin(product).product_runtime().clone();
// Maps each import name to a typed call on `host`.
let env = WasmEnv::for_product(host);
// Fails if the module imports a name `env` does not provide.
let worker = WasmWorker::new(env, &wasm)?;
worker.run().await?;
```

A call goes through three steps, joined by the import name:

1. **Building the env.** `WasmEnv::for_product(host)` calls one `link_<trait>` per service trait. `#[wasm_env]` generates each of them, with one line per method naming the trait method to call:

   ```rust
   impl<H: Account + 'static> WasmEnv<H> {
       pub fn link_account(&mut self) {
           // `host` is the product's `ProductRuntimeHost`, `cx` carries the call id and its
           // cancellation token, `request` is the decoded typed request.
           self.request("account_get_user_id", |host, cx, request| {
               Box::pin(async move { Account::get_user_id(&*host, &cx, request).await })
           });
           // ...one line per Account method
       }
   }
   ```

   `WasmEnv::request` and `WasmEnv::subscription` hold the shared work, written once for every method. Each stores a `Method` under the import name: given the request bytes, it decodes them, runs the line above, and encodes the result in the caller's protocol version. In outline:

   ```rust
   fn request(&mut self, name: &'static str, call: impl Fn(Arc<H>, CallContext, Request) -> Future<Result<..>>) {
       let host = self.host.clone();
       self.methods.insert(name, |cx, bytes| {
           let request = Request::decode(bytes);   // versioned request
           let version = request.version();
           let result = call(host, cx, request).await;
           one_event(Response, result.in_version(version).encode())
       });
   }
   ```

   `subscription` is the same, except the trait method returns a stream: each item becomes an `Item` event and its end an `End` event. These mirror `Dispatcher::on_request` and `Dispatcher::on_subscription` on the frame path, keyed by import name instead of `(trait, method)` ids.

2. **Loading the module.** For each import the module declares, `WasmWorker::new` looks up the `Method` of that name and links it into wasmi. A name the env lacks rejects the module. Since a wasm import cannot wait, the linked closure only queues the call and returns a handle:

   ```rust
   let method = env.methods.get(name).ok_or_else(unknown_import)?.clone();
   linker.func_wrap("truapi", name, move |caller: Caller<'_, GuestState>, ptr: u32, len: u32| {
       start_call(caller, method.clone(), ptr, len) // queues Start { method, request }
   })?;
   ```

3. **Running.** `WasmWorker::run` takes each queued call, runs `method(cx, request)`, and delivers the events it yields to the guest.

## A call, end to end

```
 worker (guest)                                   core (host)
 truapi::account::get_user_id(()).await
   encode the request
   import truapi.account_get_user_id(ptr, len) --> copy the request, return handle 7
   the future for handle 7 waits, main yields
                                                  Account::get_user_id(&host, cx, request).await
                                                  encode the result
                                              <-- truapi_on_event(7, Response, len)
   allocate len bytes
   import truapi.read_event(ptr)              --> copy the result into ptr
   the future for handle 7 decodes it, main resumes
```

- Wasm imports are synchronous, so an import only starts the call and returns a handle. The answer arrives later through `truapi_on_event`, which wakes the future or stream holding that handle.
- A subscription is answered the same way: one `Item` event per item, then one `End` event.
- Dropping the future or stream calls the `release` import, which cancels the call on the host.
- Imports never call back into the guest. The host starts queued calls once the guest returns control, then delivers events one at a time.

## Where the bindings come from

`#[wasm_env]` sits on every service trait in `truapi::api`. From one reading of the trait it emits both sides, so they cannot drift apart:

- **Host**, under the `wasm-worker` feature: `WasmEnv::link_<trait>`, the table entries for that trait. `WasmEnv::for_product` links all of them.
- **Guest**, under the `guest` feature on wasm32: a `guest` module beside the trait, with the import declarations (public, in `guest::imports`, for raw calls) and one typed async function per method. `truapi-guest-api` re-exports it as `truapi::<trait>` and adds the executor and exports.

Import names are `<trait>_<method>`, the names the wire table uses. Internal and host-initiated methods (such as `renderer_render` and the Chat Extension v2 queries `chat_messages_query` and `chat_user_action`) get no import: `link_<trait>` records them with `WasmEnv::exclude`. A test holds that every wire table method is linked or excluded, never both. For each host-initiated method the guest module gets `on_<method>` instead, which registers the handler that answers it.

## The ABI

`truapi::wasm_abi` holds what both sides must agree on beyond the generated method imports: the import module name, the fixed imports and exports, and the event kinds.

| Import (module `truapi`) | Signature | Meaning |
| --- | --- | --- |
| `<trait>_<method>` | `(request_ptr, request_len) -> handle` | Start a call with a SCALE-encoded versioned request |
| `release` | `(handle)` | Cancel a request or stop a subscription |
| `read_event` | `(ptr)` | Copy the payload of the event being delivered into guest memory, for the length `truapi_on_event` announced |
| `log` | `(ptr, len)` | Write a UTF-8 line to the host's log |
| `finish` | `(succeeded, ptr, len)` | Report that the entry point returned, with an error message when it failed |
| `random` | `(ptr, len)` | Fill guest memory with random bytes from the host's operating system generator |
| `now_millis` | `() -> u64` | Unix time of the host clock in milliseconds |
| `monotonic_millis` | `() -> u64` | Milliseconds since the host loaded the worker; never goes back |
| `sleep` | `(millis: u64) -> handle` | Start a timer. When it elapses, the host sends one `Response` event with an empty payload; `release` cancels it |
| `call_item` | `(call, ptr, len)` | Send one item of a call the host started: the SCALE item in the version of the request |
| `call_end` | `(call, ptr, len)` | End a call the host started: a SCALE `Result<(), CallError<Error>>` in the version of the request. No item follows it |

| Export | Signature | Meaning |
| --- | --- | --- |
| `truapi_start` | `()` | Run the entry point |
| `truapi_on_event` | `(handle, kind, len)` | Announce one event for a call; the guest reads its payload with `read_event` |
| `truapi_on_call` | `(call, method_len, request_len)` | Start a call of the host; the guest reads the UTF-8 method name, then the SCALE versioned request, with `read_event` |
| `truapi_on_cancel` | `(call)` | The host stopped a call it started; it drops each answer that comes after |

A module exports both `truapi_on_call` and `truapi_on_cancel`, or neither. Without them the host ends each of its calls at once with `Unsupported`. The `call` ids are the host's own, apart from the handles of the guest's calls.

The guest owns all of its memory: it passes the buffers for requests and events, and the host never allocates in it. This is the caller-provided buffer pattern of WASI preview 1 and `pallet-revive-uapi`, so the ABI itself needs no allocator in the guest. The typed bindings in `truapi-guest-api` still allocate, since protocol types hold `Vec` and `String`.

An event is a `Response` (`Result<Response, CallError<Error>>`), an `Item` of a subscription, or the `End` of a subscription (`Result<(), CallError<Error>>`). Payloads are the versioned SCALE values the wire carries, so a worker built against an older protocol is answered in its own version.

## Calls the host starts

The host starts a call into the product when it needs the product's answer: a rendered body (`renderer.render`), or for a room the product keeps (`productSource`), a page of messages, what the user did, and the other Chat Extension v2 queries. Every such method is a subscription: the product sends items, then one end, and the host may stop it before the end. A script worker answers them over frames; a wasm worker answers them through the export `truapi_on_call` and the imports `call_item` and `call_end`.

```
 core (host)                                      worker (guest)
 execution.chat_user_action(request)
   HostInitiatedSubscriptionManager: Start frame
   WorkerFrames: Start -> HostCalls::start
                                              --> truapi_on_call(3, 16, len)
                                                  allocate, import read_event(ptr)
                                                  the executor spawns the handler of
                                                  "chat_user_action" as a task
                                                  sink.send(item) -> import call_item(3, ...)
                                                  Ok(())          -> import call_end(3, ...)
   the worker passes each answer on after the entry returns
   WorkerFrames: Receive, Interrupt frames
   HostInitiatedSubscriptionManager -> the query's stream
```

- **Where it plugs in.** The core starts every host-initiated call the same way, through the `HostInitiatedSubscriptionManager` of the product's connection, which sends a `Start` frame to the connection's frame sink and takes `Receive` and `Interrupt` frames back. `WasmWorker::for_product_runtime(wasm, open)` builds the connection with a frame sink of its own (`WorkerFrames`, in [`wasm_worker/product.rs`](../../rust/crates/truapi/src/wasm_worker/product.rs)): it maps the `(trait, method)` ids of a `Start` frame to the wire name, starts the call in the worker, and routes each answer back as a frame through `ProductRuntimeControl::receive_host_call_answer`. A `Stop` frame stops the call in the worker. Thus the core's own checks (the room declares the handler, a worker reference while the product answers, the screen of the answer, the persona check) apply to a wasm worker unchanged, and no method has code of its own. The native execution (`NativeProductExecution::start_wasm_worker`, so `render`, `chat_messages_query`, `chat_user_action` and the other queries) and `truapi-host dev --worker-wasm` both use it.
- **The worker side.** `WasmWorker::host_calls()` gives a `HostCalls` handle that any thread can use. `HostCalls::start(method, request, answers)` queues the call; the worker takes it between two entries, so an import never calls back into the guest. The worker gives each answer to `answers` (`HostCallEvent::Item`, then one `HostCallEvent::End`). Dropping the returned `HostCall` stops the call: the worker calls `truapi_on_cancel` and drops what comes after. `HostCalls::stream` gives the same answers as a stream.
- **Ends.** A method with no handler ends at once with `Unsupported`. A worker that ends, traps or never ran ends each open call with `HostFailure { reason: "the wasm worker ended" }`, so the host never waits on a worker that is gone.
- **Waiting.** While the host holds a `HostCalls` handle, a worker that waits on nothing else waits for calls. `run` returns `Stalled` only when no call of the worker is in flight and no handle is left.

### Guest API

`#[wasm_env]` generates one registration function for each host-initiated method, in the guest module of its service, mirroring the TypeScript client's `truapi.chat.onUserAction((request, send, interrupt) => ...)`:

| Function | Signature |
| --- | --- |
| `truapi::renderer::on_render` | `fn on_render<H, A>(handler: H) where H: Fn(ProductRendererRenderRequest, Sink<ProductRendererRenderItem>) -> A + 'static, A: Future<Output = Result<(), CallError<ProductRendererRenderError>>> + 'static` |
| `truapi::chat::on_<method>` | the same shape for `messages_query`, `user_action`, `reactions_query`, `participants_query`, `message_actions_query`, `participant_actions_query` and `search_query` |
| `truapi::Sink<Item>` | `fn send(&self, item: Item::Latest)`; `Clone`. Encodes the item in the version of the request |
| `truapi::raw::on_call` | `fn on_call<H, A>(method: &'static str, handler: H) where H: Fn(Vec<u8>, RawSink) -> A + 'static, A: Future<Output = Vec<u8>> + 'static` |
| `truapi::raw::RawSink` | `fn send(&self, item: &[u8])`; `Clone` |

Requests and items are `truapi::latest` types. The handler runs as a task on the guest executor. It sends items through the sink, and its result ends the call: `Ok(())` completes it, and an error interrupts it with the method's own error, in the version of the request. When the host stops the call, the executor drops the task, which releases the calls it holds. A second handler for a method replaces the first. A raw handler gets the SCALE versioned request and gives the SCALE end. Register the handlers before the first `.await` of the entry point: the host starts no call before the start entry returns, and a call that comes before its handler ends with `Unsupported`.

```rust
#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    truapi::chat::on_user_action(|request, sink| async move {
        if request.room_id != "notes" {
            return Err(CallError::Domain(ProductChatUserActionError::UnknownRoom));
        }
        sink.send(ProductChatUserActionItem::Acknowledged);
        Ok(())
    });
    truapi::renderer::on_render(|request, sink| async move {
        sink.send(RendererNode::String { text: format!("{} bytes", request.payload.len()) });
        // The body stays on screen until the host stops the render, which drops this task.
        futures::future::pending().await
    });
    truapi::chat::create_room(/* a room with `product_source: true` */).await?;
    futures::future::pending().await
}
```

[`rust/guests/chat-source`](../../rust/guests/chat-source) is a complete example: it keeps the room `notes` (`messages_query`, `user_action`), draws the `Custom` body of its first message (`render`), and answers `participants_query` with a raw handler.

## Writing a worker

A worker depends on `truapi-guest-api` under the name `truapi`, so calls read like the TypeScript client's `truapi.account.getUserId()`:

```toml
truapi = { package = "truapi-guest-api", path = "..." }
```

```rust
#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    let user = truapi::account::get_user_id(()).await?;
    truapi::log!("user id: {}", user.primary_username);
    Ok(())
}
```

Requests are futures and subscriptions are streams, both over `truapi::latest` types. Dropping either before it ends releases the call.

Beside the service modules, `truapi-guest-api` has:

| Item | Signature | Meaning |
| --- | --- | --- |
| `truapi::spawn` | `fn spawn(future: impl Future<Output = ()> + 'static)` | Run a task beside the entry point |
| `truapi::time::now_unix_millis` | `fn now_unix_millis() -> u64` | Unix time of the host clock |
| `truapi::time::monotonic_millis` | `fn monotonic_millis() -> u64` | Milliseconds since the worker loaded, for intervals |
| `truapi::time::sleep` | `fn sleep(duration: Duration) -> Sleep` | A host timer, rounded up to a full millisecond; `Sleep: Future<Output = ()> + Send`, and dropping it cancels the timer |
| `truapi::random::fill` | `fn fill(buffer: &mut [u8])` | Random bytes from the host, good for keys |
| `truapi::raw::call` | `fn call(import: StartImport, request: &[u8]) -> RawCall` | Start a request with an encoded versioned request; `RawCall: Future<Output = Vec<u8>> + Send` resolves to the SCALE `Result<Response, CallError<Error>>` |
| `truapi::raw::subscribe` | `fn subscribe(import: StartImport, request: &[u8]) -> RawSubscription` | Start a subscription; `RawSubscription: Stream<Item = RawEvent> + Send` yields `RawEvent::Item(bytes)`, then one `RawEvent::End(bytes)` with the SCALE `Result<(), CallError<Error>>` |
| `truapi::raw::StartImport` | `extern "C" fn(*const u8, u32) -> u32` | The import of a method, such as `truapi::local_storage::imports::read` |
| `truapi::<service>::on_<method>` | see [Guest API](#guest-api) | Answer the calls of a host-initiated method, such as `truapi::chat::on_user_action` |
| `truapi::raw::on_call` | `fn on_call(method: &'static str, handler)` | Answer the calls of a host-initiated method with encoded requests and items |

`truapi::codec`, `truapi::versioned`, `truapi::v01` and `truapi::v02` re-export the codec and the protocol types that a raw call encodes and decodes.

The executor polls a task only when its own waker is woken: at start, after a host event for one of its calls or timers, or after a different task wakes it. One entry from the host polls until no task is woken. Spawned tasks run while the entry point waits. When the entry point returns, the worker ends: the executor drops every task, which releases its calls, and reports the outcome. `RawCall`, `RawSubscription` and `Sleep` hold only a handle, so they are `Send` and can be in `async_trait` futures.

```rust
#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    let mut seed = [0u8; 32];
    truapi::random::fill(&mut seed);
    truapi::spawn(async {
        truapi::time::sleep(Duration::from_secs(1)).await;
        truapi::log!("one second later");
    });
    let request = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".into() }).encode();
    let payload = truapi::raw::call(truapi::local_storage::imports::read, &request).await;
    let read = Result::<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>::decode(&mut &payload[..])?;
    Ok(())
}
```

The examples in [`rust/guests`](../../rust/guests) build with:

```bash
cargo build --manifest-path rust/guests/Cargo.toml --target wasm32-unknown-unknown --release
```

and run against a CLI host with `/worker <wasm-path>`, as the selected product.
`truapi-host dev --worker-wasm <wasm-path>` runs one as the `Worker` of the
product beside the chat page `/chat` and its socket `/chat/ws`, so a person or
a test can act in its rooms; `chat-actions` is the example for a room the host
stores, and `chat-source` for a room the worker keeps. `/worker` takes no calls
from the host.

## Limits

- Fuel: each entry from the host (the start, or the delivery of one event) gets `DEFAULT_FUEL_PER_ENTRY`, 20 billion, about one unit for each wasm instruction. An entry that uses all of it traps. Release builds of wasmi on an Apple M-series machine use about 400 million fuel each second, so one entry can run for about 50 seconds. The value is high for crypto and database work in one entry (X448, sntrup761, an encrypted SQLite image), and it still stops a worker in an endless loop. No such workload has been measured yet. `WasmWorker::with_fuel_per_entry` changes it.
- Memory: `DEFAULT_MEMORY_LIMIT`, 256 MiB of linear memory. The engine gets memory only when the worker grows it. `WasmWorker::with_memory_limit` changes it.
- A worker may pass at most 8 MiB per request, item, end or log line (and for each `random` call), and may hold 1024 calls and timers open.
- Timers use `futures-timer`, so they work on any executor that drives `WasmWorker::run`, Tokio included, and come through the same event queue as calls. Releasing a timer removes it at once.
- Releasing a call fires its cancellation token, as a `Cancel` frame does, and the host keeps polling it until it settles, dropping its events.
- Wasm runs synchronously on the task that drives the worker, so a long entry blocks that thread until it returns or runs out of fuel.
- A module that imports anything the host does not link is rejected before it runs.
- The worker runs as the `Worker` execution kind, so Worker-only services such as `Chat` and `Pocket` are available to it.

## Open questions

- How the manifest marks a Worker executable as wasm rather than JavaScript.
- `truapi::latest` does not yet re-export every request type a worker builds.
- Whether browsers run workers through this engine or the platform's own `WebAssembly`.
