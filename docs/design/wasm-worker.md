---
title: "Wasm Product Workers"
type: design
status: experimental
created: 2026-10-08
---

# Wasm product workers

A product can ship its worker as a Rust crate compiled to `wasm32-unknown-unknown`.
The core runs the module in process, bound to one product, beside the QuickJS
sandbox the [worker lifecycle RFC](../rfcs/worker-lifecycle.md) describes for
JavaScript workers. The module calls TrUAPI through wasm imports, and each import
reaches the product's typed trait implementation directly, with no frames or
dispatcher in between.

```
 worker.wasm (one product)                    core
+------------------------------+          +------------------------------------+
| truapi_guest_api::account::  |  import  | WasmWorker                         |
|   get_user_id(()).await      |--------->|   truapi.account_get_user_id       |
|                              |          |     -> Account::get_user_id(&cx,   |
| exports:                     |  export  |          request) on the product's |
|   truapi_start               |<---------|        ProductRuntimeHost          |
|   truapi_alloc               |          |   answers through truapi_on_event  |
|   truapi_on_event            |          |                                    |
+------------------------------+          +------------------------------------+
```

## One definition, both sides

`#[wasm_env]` sits on every service trait in `truapi::api`. From one reading of the
trait it emits:

- **Host side**, under the `wasm-worker` feature: `WasmEnv::link_<trait>`, which
  links each method to a typed call on any `H: Trait`. `WasmEnv::for_product`
  links every trait for a `TrUApi` implementation.
- **Guest side**, under the `guest` feature: a `guest` module beside the trait,
  holding the import declarations and one typed async function per method.
  `truapi-guest-api` re-exports these modules and adds the executor and exports.

Import names are `<trait>_<method>`, the same names the wire table uses, and a
test holds the linked set equal to the wire table's product-started methods.
Internal methods and host-initiated methods get no import.

## The import module

Every import lives in the `truapi` wasm module. The names are constants in
`truapi::wasm_abi`.

| Import | Signature | Meaning |
| --- | --- | --- |
| `<trait>_<method>` | `(request_ptr, request_len) -> handle` | Start a call with a SCALE-encoded versioned request |
| `release` | `(handle)` | Cancel a request or stop a subscription |
| `log` | `(ptr, len)` | Write a UTF-8 line to the host's log |
| `finish` | `(succeeded, ptr, len)` | Report that the entry point returned, with an error message when it failed |

| Export | Signature | Meaning |
| --- | --- | --- |
| `truapi_start` | `()` | Run the entry point |
| `truapi_alloc` | `(len) -> ptr` | Memory for a payload the host is about to write |
| `truapi_on_event` | `(handle, kind, ptr, len)` | Deliver one event for a call |

An event is a `Response` (`Result<Response, CallError<Error>>`), an `Item` of a
subscription, or the `End` of a subscription (`Result<(), CallError<Error>>`).
Payloads are the same versioned SCALE values the wire carries, so a worker built
against an older protocol is answered in its own version.

Imports never call back into the guest. The host queues each call and starts it
once the guest returns control, then delivers events one at a time.

## Writing a worker

```rust
use truapi_guest_api::{Error, account, log};

truapi_guest_api::main!(run);

async fn run() -> Result<(), Error> {
    let user = account::get_user_id(()).await?;
    log!("user id: {}", user.primary_username);
    Ok(())
}
```

Requests are futures and subscriptions are streams, both over `truapi::latest`
types. Dropping either before it ends releases the call. The examples in
[`rust/guests`](../../rust/guests) build with:

```bash
cargo build --manifest-path rust/guests/Cargo.toml --target wasm32-unknown-unknown --release
```

and run against a CLI host with `/worker <wasm-path>`, as the selected product.

## Limits

- A worker gets a fixed amount of fuel for each entry from the host and 64 MiB of
  linear memory. The values are placeholders.
- A module that imports anything the host does not link is rejected before it
  runs.
- The worker runs as the `Worker` execution kind, so Worker-only services such as
  `Chat` and `Pocket` are available to it.

## Open questions

- Host-initiated methods (`renderer.render`, the RFC's `worker.accept_new_update`)
  need a guest export that accepts a call the host starts.
- How the manifest marks a Worker executable as wasm rather than JavaScript.
- `truapi::latest` does not yet re-export every request type a worker builds.
- Whether browsers run workers through this engine or the platform's own
  `WebAssembly`.
