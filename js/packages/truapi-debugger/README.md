# @parity/truapi-debugger

The debugger-side consumer for TrUAPI wire frames. **Private, in-repo, not published.**

The host taps every product↔host wire frame in its Rust core (`truapi`'s
`DebugSink`) and streams each one outward as a `{ channelId, dir, frame: bytes }`
envelope. This package is the other end: it owns **all** decoding — the wire
envelope (`requestId` and frame id, via `decodeWireMessage`), the grouping into
per-operation traces, and the per-frame payload decode. The host core treats
frames as opaque bytes and never decodes.

This keeps `@parity/truapi` (the product package) genuinely untouched: the tap is
in the Rust host, and the debugger's decode/trace logic lives here instead of in
the product transport.

> **Scope note.** This package holds both the debugger *library* (the trace,
> envelope-decode, and value-decode engines plus the ingest that turns a wire
> envelope into a decoded frame) and its two *mounts* — the standalone app
> (`server.ts`) and the in-app embed (`in-app.ts`). It lives in-repo because the
> debugger is coupled to the protocol this repo owns: it decodes wire frames with
> `@parity/truapi`, tracking the generated wire surface. *Where the app
> ultimately lives* (stays a truapi tool / own repo / a desktop app) is an open
> decision for the host-protocol owner; in-repo is the low-regret default and
> moving it later is cheap.

## What's here

- **`createDebugSession()`** — the trace engine wired to the ingest. Feed it
  envelopes with `handleEnvelope(...)`; read grouped traces from `traceEngine`,
  per-frame values from `frameDetail(...)` / `decodedFrames(...)`.
- **`createDebugIngest(sink)`** — decodes a `DebugFrameEnvelope` into an
  `ObservedFrame` and forwards it. The layer that turns raw wire bytes into
  something the trace engine can group.
- **`createWireDebugger(...)`** — accumulates observed frames into per-`requestId`
  traces (correlates with product-sdk telemetry spans on the same id).
- **`createFrameDecoder(...)`** — the level-2 value decoder (see below): a
  per-frame decode of a payload to a plain JS value, reusing `@parity/truapi`'s
  generated `WIRE_DECODE_TABLE`. Every frame it can decode, it does, with no
  sensitive special-casing. The bare factory takes `enabled: true` to opt in; a
  session turns it on for you.
- **`buildTraceView` / `wireTraceToView`, `renderOperationRow`,
  `renderTraceDetail`, `renderFrameValueDetail`** — the one view model and the one
  set of renderers both mounts share, so the two cannot drift apart.
- **`startDebugServer(...)`** (`server.ts`) — the standalone mount, below.
- **`createInAppDebugger(...)`** (`in-app.ts`) — the in-app mount, below.

## The two mounts

Both render the same view model with the same renderers and the same stylesheet.
They differ in where the debugger sits relative to the host:

```text
standalone: host process ──ws://127.0.0.1:9231──▶ debugger server ──HTTP──▶ browser
            (host dials out; frames leave the app; one server, many channels)

in-app:     host in the page ──handleFrame()──▶ InAppDebugger.mount(el)
            (same page as the host; no server, no dial; frames never leave the app)
```

- **Standalone** (`startDebugServer`): a Bun WS+HTTP server bound to
  `127.0.0.1` only. Hosts dial *in* and send one text message per frame,
  `{ channelId, dir, frame }` with `frame` base64-encoded, plus the wire-identity
  fields a versioned host stamps (`v`, `codec`, `schema`) and an optional
  `dropped` count. The browser view is a thin client over server-rendered
  fragments.
- **In-app** (`createInAppDebugger`): the second mount, for a host that runs in
  the page. It takes the same raw SCALE frame bytes with the same
  product-vantage `dir`, holds the session in-process, and renders the fragments
  directly with no polling. Browser-only (uses `document`); each browser tab is
  its own tenant, so there is nothing to host or scope.

## Value decode (level 2 — on by default)

This is a **dev-only tool that decodes everything**. The list views stay
payload-blind — they group frames and sum byte lengths, never their contents —
and the drill-down decodes a frame's payload to a plain JS value, for every
frame, with no "sensitive" special-casing. Its contract:

- **On by default.** The standalone server decodes unless
  `TRUAPI_DEBUGGER_DECODE_VALUES` is set to a falsy value
  (`0`/`false`/`no`/`off`), or `startDebugServer({ decodeValues: false })` /
  `createInAppDebugger({ decodeValues: false })` in code — useful for a demo.
  With decode off, every frame reports byte length only and no bytes are even
  retained.
- **Reuses the generated table.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)`
  from `@parity/truapi/wire-decode` — the same dev-only codecs the client uses.
  The debugger writes none of its own.
- **No redaction, no reveal toggle.** Every frame the table can decode is
  decoded, including signing, login, and payment. A developer inspecting their
  own session's traffic sees the real values; there is no denylist, no reveal
  escape hatch, and no `redacted` state. A frame the codec cannot type still
  shows its raw payload as `<n>B · 0x…` hex — a dev-only tool hides nothing it
  has the bytes for. Only a frame with no retained bytes (decode off) reads
  `payload not shown`.
- **Refused on contract drift.** Decode is allowed only for a channel whose
  declared `schema` fingerprint (`TRUAPI_WIRE_SCHEMA_HASH`) and `codec` match
  this debugger's; a mismatched or absent identity is refused (`/frame` answers
  409) and banners in the view. Payload-blind grouping is unaffected.
- **Never over the wire, never in the list endpoints.** The host emits opaque
  bytes only; nothing about decode changes what it sends. Decode happens in the
  debugger, in the drill-down paths only.

## Standalone endpoints

| Endpoint                              | Serves                                                    |
| ------------------------------------- | --------------------------------------------------------- |
| `GET /`                               | The inspector page: polls the fragments below.            |
| `GET /op-list?channel=&sort=`         | One server-rendered row per op. `sort` is `recent`, `duration`, `frames`, or `method`; absent keeps arrival order. Payload-blind. |
| `GET /op?id=&channel=&gen=`           | The selected op's drill-down, each frame's value inline.   |
| `GET /view`                           | The drill-down as a standalone fragment, values inline.    |
| `GET /channels`                       | Connected hosts/channels, liveness, codec-mismatch flag.   |
| `GET /stats?channel=`                 | Aggregate roll-up: counts, bytes, durations, health, busiest methods. Payload-blind. |
| `GET /traces`                         | The grouped traces as JSON. Payload-blind — never serializes bytes or values. |
| `GET /frame?id=&i=&channel=`          | One frame's decode as JSON (the programmatic drill-down).  |

Loopback is enforced on more than the bind: a request whose `Host` header is not
a loopback name gets a 403 (DNS-rebinding guard), and a WebSocket upgrade from a
foreign browser `Origin` is refused (CSWSH).

## Run

```bash
npm install   # links @parity/truapi via the workspace
npm run build # tsc -b
npm run serve # bun run src/server.ts — listens on 127.0.0.1:9231, decodes by default

# a different port, or decode off for a demo
TRUAPI_DEBUGGER_PORT=9300 npm run serve
TRUAPI_DEBUGGER_DECODE_VALUES=0 npm run serve
```

Point a host's debugger URL at `ws://127.0.0.1:9231` (the host dials out) and
open `http://127.0.0.1:9231/`; click an op for its drill-down detail.

Use the literal `127.0.0.1`, not `localhost`. Both dial gates accept a `ws://`
URL on a loopback host **only** — `wss://`, certificates, and any non-loopback
target are rejected — and `localhost` passes that check but resolves `::1` first
on macOS, while the server binds `127.0.0.1` alone. A native host then dials an
address nothing is listening on and logs nothing.

For the in-app mount, feed frames straight to the session:

```ts
import { createInAppDebugger } from "@parity/truapi-debugger";

const inspector = createInAppDebugger();
const dispose = inspector.mount(document.getElementById("wire-panel")!);
// from the host's tap, per frame:
inspector.handleFrame(channelId, "out", frameBytes);
```

The exact host↔debugger framing is provisional (envelope spec, track T3);
base64-in-JSON is what the server accepts today.
