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

## Host setup

The host serves a worker with the same per-product object an iframe product talks to: [`ProductRuntimeHost`](../../rust/crates/truapi/src/runtime.rs), which implements every service trait for one product (the implementations are in [`runtime/capabilities`](../../rust/crates/truapi/src/runtime/capabilities)).

```rust
// `product` carries the Worker execution kind.
let host = runtime.product_admin(product).product_runtime().clone();
// Maps each import name to a typed call on `host`.
let env = WasmEnv::for_product(host);
// Fails if the module imports a name `env` does not provide.
let worker = WasmWorker::new(env, &wasm)?;
worker.run().await?;
```

A call goes through three steps, joined by the import name:

1. **Building the env.** `#[wasm_env]` registers one `Method` per import name. A `Method` takes the request bytes and returns the events to send back: it decodes the versioned request, runs the typed call, and encodes the result in the caller's protocol version.

   ```rust
   self.request("account_get_user_id", |host, cx, request| {
       Box::pin(async move { Account::get_user_id(&*host, &cx, request).await })
   });
   ```

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
- **Guest**, under the `guest` feature on wasm32: a `guest` module beside the trait, with the import declarations and one typed async function per method. `truapi-guest-api` re-exports it as `truapi::<trait>` and adds the executor and exports.

Import names are `<trait>_<method>`, the names the wire table uses, and a test holds the linked set equal to the wire table's product-started methods. Internal and host-initiated methods get no import.

## The ABI

`truapi::wasm_abi` holds what both sides must agree on beyond the generated method imports: the import module name, the fixed imports and exports, and the event kinds.

| Import (module `truapi`) | Signature | Meaning |
| --- | --- | --- |
| `<trait>_<method>` | `(request_ptr, request_len) -> handle` | Start a call with a SCALE-encoded versioned request |
| `release` | `(handle)` | Cancel a request or stop a subscription |
| `read_event` | `(ptr)` | Copy the payload of the event being delivered into guest memory, for the length `truapi_on_event` announced |
| `log` | `(ptr, len)` | Write a UTF-8 line to the host's log |
| `finish` | `(succeeded, ptr, len)` | Report that the entry point returned, with an error message when it failed |

| Export | Signature | Meaning |
| --- | --- | --- |
| `truapi_start` | `()` | Run the entry point |
| `truapi_on_event` | `(handle, kind, len)` | Announce one event for a call; the guest reads its payload with `read_event` |

The guest owns all of its memory: it passes the buffers for requests and events, and the host never allocates in it. This is the caller-provided buffer pattern of WASI preview 1 and `pallet-revive-uapi`, so the ABI itself needs no allocator in the guest. The typed bindings in `truapi-guest-api` still allocate, since protocol types hold `Vec` and `String`.

An event is a `Response` (`Result<Response, CallError<Error>>`), an `Item` of a subscription, or the `End` of a subscription (`Result<(), CallError<Error>>`). Payloads are the versioned SCALE values the wire carries, so a worker built against an older protocol is answered in its own version.

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

Requests are futures and subscriptions are streams, both over `truapi::latest` types. Dropping either before it ends releases the call. The examples in [`rust/guests`](../../rust/guests) build with:

```bash
cargo build --manifest-path rust/guests/Cargo.toml --target wasm32-unknown-unknown --release
```

and run against a CLI host with `/worker <wasm-path>`, as the selected product.

## Limits

- A worker gets a fixed amount of fuel for each entry from the host and 64 MiB of linear memory, may pass at most 8 MiB per request or log line, and may hold 1024 calls open. The values are placeholders.
- Releasing a call fires its cancellation token, as a `Cancel` frame does, and the host keeps polling it until it settles, dropping its events.
- Wasm runs synchronously on the task that drives the worker, so a long entry blocks that thread until it returns or runs out of fuel.
- A module that imports anything the host does not link is rejected before it runs.
- The worker runs as the `Worker` execution kind, so Worker-only services such as `Chat` and `Pocket` are available to it.

## Open questions

- Host-initiated methods (such as `renderer.render`) need a guest export that accepts a call the host starts.
- How the manifest marks a Worker executable as wasm rather than JavaScript.
- `truapi::latest` does not yet re-export every request type a worker builds.
- Whether browsers run workers through this engine or the platform's own `WebAssembly`.
