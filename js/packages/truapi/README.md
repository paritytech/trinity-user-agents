# @parity/truapi

_Typed TypeScript client for products that talk to a TrUAPI host._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)
[![Types](https://img.shields.io/badge/types-included-3178C6?style=flat-square&logo=typescript)](./package.json)

This package gives a product running inside a Polkadot host (Desktop Browser, Triangle webview) a fully typed client for every TrUAPI method. The transport, SCALE codecs, generated types, and generated domain clients are all bundled together.

## Install

```bash
npm install @parity/truapi
```

## Quick start

```ts
import {
  createClient,
  createMessagePortProvider,
  createTransport,
  type Client,
  type HostAccountGetResponse,
} from "@parity/truapi";

const provider = createMessagePortProvider(port);
const transport = createTransport(provider);
const truapi: Client = createClient(transport);

const result = await truapi.accountManagement.accountGet({
  productAccountId: { dotNsIdentifier: "my-product.dot", derivationIndex: { tag: "Index", value: 0 } },
});

if (result.isErr()) throw result.error;
const account: HostAccountGetResponse = result.value;
```

Request methods take the inner request value directly. The transport adds the wire-level version wrapper and unwraps versioned responses before the generated method returns.

Requests reject with `RequestTimeoutError` when no matching response arrives within 120 seconds.
Pass `{ requestTimeoutMs }` to `createTransport` to select a different positive deadline.

## Subscriptions

Streaming methods return a small Observable-compatible object:

```ts
import type { Subscription, RemoteChainHeadFollowItem } from "@parity/truapi";

const sub: Subscription = truapi.chainInteraction
  .chainHeadFollow({ request: { genesisHash, withRuntime: false } })
  .subscribe({
    next(event: RemoteChainHeadFollowItem) {
      console.log(event);
    },
    // `reason` carries the method's declared interrupt value, and is
    // undefined when the stream ended on a transport or decode failure.
    error(error) {
      console.error(error.reason ?? error);
    },
    complete() {
      console.log("stream ended");
    },
  });

sub.unsubscribe();
```

## What's in the package

- **Transport providers** for `MessagePort` pipes (used by both webview hosts and iframe hosts)
  and for WebSocket endpoints (used by hosts that serve frames on a loopback socket).
- **TrUAPI transport** that handles request, response, subscription, and handshake framing.
- **Generated domain clients and types** produced from the Rust API contract.
- **SCALE codec helpers** used by the generated code, also re-exported for direct use.
- **Sandbox bootstrap** (`@parity/truapi/sandbox`) that detects the host environment, builds the
  matching provider, and exposes a cached client - see below.

## Development escape hatches

- **`development_createAccountProof(client, request)`** — `account.createAccountProof`
  with `context` given as the exact 32-byte hex the proof is bound to, instead of a
  product-namespaced `ProductProofContext`. Yet to be removed before a production
  release; it lives entirely in `src/development.ts`.

## Sandbox bootstrap

`@parity/truapi/sandbox` wires up a client for browser-embedded hosts: it detects whether the app
runs inside a TrUAPI host, repeatedly announces iframe readiness until the host transfers a channel,
builds the matching provider, and caches the resulting client. Use it instead of assembling
`createTransport` / `createClient` by hand.

Iframe readiness messages carry a public `connectionId`: retries keep the same identifier,
while a new provider (including a document reload) gets a new one. Hosts must not replace an
adopted port for another readiness message with the same identifier. This identifier only
distinguishes transport attempts; it does not authenticate the product or grant authority.

```ts
import {
  getClientSync,
  isCorrectEnvironment,
  subscribeConnectionStatus,
} from "@parity/truapi/sandbox";

const client = getClientSync(); // null outside a host container
if (client) {
  // …make host calls
}

// Or drive UI off connection status:
const unsubscribe = subscribeConnectionStatus((status) => {
  // "disconnected" | "connecting" | "connected"
});
```

| Export                                      | Purpose                                         |
| ------------------------------------------- | ----------------------------------------------- |
| `isCorrectEnvironment(): boolean`           | Synchronous host-environment detection.         |
| `getClientSync(): TrUApiClient \| null`     | Cached client; `null` outside a host container. |
| `subscribeConnectionStatus(cb): () => void` | Connected / disconnected status listener.       |
| `connectWebSocketHost(url): TrUApiClient`   | Use a host that serves frames over a WebSocket. |

Native and CLI browser hosts inject a shared SDK client. Keep the client
returned by `getClientSync()`:
it remains usable when the host replaces a disconnected connection. Interrupted
requests reject with `ConnectionResetError`; subscriptions end with that error
as their cause. Nothing is replayed. Recreate read/watch subscriptions and discard
old chain handles before resuming work. Registered host-initiated handlers remain
installed. Older SDKs use a small MessagePort adapter and require a page reload
after disconnect.

The shared connection attempts one immediate reconnect. If a reconnect fails
while the page is visible, it tries again after 250 ms, 1 s and 4 s. After that,
the next API call or return to a visible page tries again. A product that stays
visible and only waits for connection status will not trigger further retries.

### Hosts on a WebSocket

For a direct connection to a loopback host such as
`truapi-host signing-host --frame-listen`, set the endpoint before anything else
touches the client. Native and CLI browser bootstrap already provide the client
and need no explicit endpoint:

```ts
import { connectWebSocketHost } from "@parity/truapi/sandbox";

connectWebSocketHost("ws://127.0.0.1:9955");
```

This is what makes a real host usable from an ordinary browser tab during development. For a
transport without the sandbox's caching and detection, `createWebSocketProvider(url)` from the
package root returns the bare `WireProvider`. Direct WebSocket providers close
permanently on disconnect. When using `connectWebSocketHost`, call `getClientSync()`
again to obtain a new direct client, then recreate its subscriptions.

## Observability / debugging

The debugger does not live in this package, and the product transport carries no debug seam -
`@parity/truapi` is genuinely untouched by observability. The host taps every product↔host frame in
its Rust core (`truapi`'s `DebugSink`) and streams each one as opaque bytes to a separate
debugger app, which decodes and groups them.

- The tap: `DebugSink` in `rust/crates/truapi/src/host_core.rs`, unset by default. It is read
  at two choke points — inbound before the frame is decoded, outbound after the product's copy is
  sent — and is fire-and-forget, so an absent or slow debugger loses traces, never a session.
- Topology: the host always dials the debugger, over `ws://` on a loopback host **only**. `wss://`,
  certificates, and non-loopback targets are rejected by the native dial gate
  (`truapi/src/native_debug.rs`), which requires every resolved address to be loopback and
  dials the addresses it checked.

The generated `WIRE_DECODE_TABLE` on the `./wire-decode` subpath (raw SCALE bytes → typed value)
stays here, since it is generated from this package's contract. It is the decode source a debugger
uses to render frame values, kept off the package barrel so importing `@parity/truapi` never pulls a
decoder into a product bundle. `@parity/truapi` itself never decodes payloads — the envelope decode it
does expose (`decodeWireMessage`: `requestId`, frame id) carries no payload value.

## Wire format

Frames are SCALE encoded:

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
```

The discriminant is a `(trait, method)` pair: the trait byte names the API trait and the method byte addresses a method within it, so method ids restart at 0 in every trait. Which leg a frame carries (a request's request/response/cancel, or a subscription's start/stop/interrupt/receive) is named by the `message_type` byte rather than by a separate id, so one method occupies exactly one id regardless of shape. The table is generated from the Rust trait-level `#[wire_trait(id = N)]` annotation plus the method-level `#[wire(id = N)]` annotation, and is written to `src/generated/wire-table.ts`.

This layout is not compatible with wire codec version 1, which addressed methods with a single flat byte. The codec version a client speaks is `TRUAPI_CODEC_VERSION`, stamped into the generated client from `truapi::WIRE_CODEC_VERSION`.

### Cancelling a call

Every generated request method takes `options?: CallOptions` last. Aborting its `signal` sends a `Cancel` frame on that method's own address, correlated by the same `requestId`:

```ts
const controller = new AbortController();
const pending = truapi.signing.createTransaction(request, { signal: controller.signal });
controller.abort();
const result = await pending; // Err(CallError.Cancelled) if the host stopped
```

Cancelling stops the waiting, not necessarily the work. The call still settles with exactly one response, and a host that received the cancel in time answers `Cancelled` whether or not its handler managed to unwind. A cancel that arrives after the response has gone out changes nothing, and the promise resolves with the real result. One that lands while the handler is still unwinding still wins, so a call aborted at the last moment can answer `Cancelled` even though its work completed. A signal already aborted when the call is made sends nothing and rejects immediately.

A host that predates the `Cancel` leg drops the frame with no reply, and the call then settles on the client's own deadline instead. There is no way to detect that first: `system.featureSupported` answers only about chains, so an abort a host never understood looks the same as one it honoured.

The pair `(255, 255)` is reserved for method-independent protocol errors. When a peer rejects an unknown API message with that frame, requests resolve as `CallError.Unsupported` and subscriptions terminate with an `UnsupportedMessageError` cause carrying the unsupported `(trait, method)` pair.

## Generated files

`src/generated/`, `src/playground/codegen/`, and `test/generated/examples/` are produced by [`truapi-codegen`](../../../rust/crates/truapi-codegen/) from the Rust crate and are ignored by git. Do not edit generated files directly. Run from the repo root:

```bash
./scripts/codegen.sh
```

## Develop

```bash
npm install
npm run build
npm test
```

On a clean checkout, the first build or test run will generate the ignored TypeScript outputs from the Rust sources, so Rust stable + nightly must be installed locally. `npm test` runs the package's [`bun test`](https://bun.sh/docs/cli/test) suite (`src/**/*.test.ts`) directly against the source `.ts` files (no build step), so [bun](https://bun.sh/) must also be installed.

## License

[MIT](../../../LICENSE)
