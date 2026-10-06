# @parity/truapi-host

WASM-backed TrUAPI host runtime. It embeds the `truapi` Rust core (compiled to WASM)
behind a Web Worker provider, plus per-environment integration entry points. It is the
counterpart to the native Android/iOS host shells.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                               | Provides                                                                                                            |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `@parity/truapi-host`                | Shared runtime types plus generated typed host callback contracts.                                                  |
| `@parity/truapi-host/web`            | Browser pairing host: `createIframeHost` (iframe MessageChannel handshake) and `createWebWorkerPairingHostRuntime`. |
| `@parity/truapi-host/worker-runtime` | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread. |
| `@parity/truapi-host/wasm/web`       | The raw browser `wasm-bindgen` glue, if you need to instantiate the core yourself.                                  |
| `@parity/truapi-host/testing`        | `createMockHost`: the in-memory host seam a product is tested against.                                              |
| `@parity/truapi-host/testing/playwright` | Playwright fixture that boots the test host and embeds the product in an iframe.                                |
| `@parity/truapi-host/testing/server` | Node server that serves the host page and its bundle.                                                              |
| `@parity/truapi-host/testing/client` | `createMockClient`: a product client over the mock with no iframe, for unit tests.                                  |
| `@parity/truapi-host/testing/dev-accounts` | Named dev accounts derived from fixed BIP-39 entropy, which sign for real.                                    |
| `@parity/truapi-host/testing/host-page` | The browser half the fixture drives, for a suite that boots its own page.                                        |
| `@parity/truapi-host/wasm/testing`   | The raw glue for the signing-enabled bundle the test host runs on.                                                  |

`scripts/build-wasm.mjs` builds two WASM bundles, both `--no-default-features`.
`wasm/web` is the production browser host and excludes
`WasmSigningHostRuntime`; `wasm/testing` adds the Rust `wasm-signing-host` and
`test-host` features, which is what lets the test host hold keys and answer
resource allocation as granted without allocating anything.
`ProductRuntimeConfig` configures the pairing host and requires no network
suffix. The signing constructor's configuration requires
`runtimeConfig.networkSuffix` in addition: the bare TLD (`dot`, `paseo`, or
`testnet`) matching the People chain and the wallet's onboarding
configuration.

`runtimeConfig.assetHub` is required by both configurations, pairing and
signing. It is the Asset Hub genesis hash, in the same shape as
`runtimeConfig.people` and `runtimeConfig.bulletin`. Product manifests are read
from the dotNS contracts deployed there, so it is what makes a
`trustedProducts` grant resolvable: without a usable value no manifest resolves,
so every cross-product grant not already cached is refused, and the refusal is
indistinguishable from the other product having granted nothing. A config
omitting it is rejected.

## Bundler requirements

The worker imports the WASM glue by a literal specifier, so every bundler
resolves it statically and emits `truapi_server.js` as a chunk. Whether the
`truapi_server_bg.wasm` payload comes with it depends on the bundler: emitting
it requires treating `new URL("truapi_server_bg.wasm", import.meta.url)` inside
the glue as an asset reference, and not all of them do.

| Bundler             | Emits the `.wasm`? | Host action                                                    |
| ------------------- | ------------------ | -------------------------------------------------------------- |
| Vite                | Yes                | None — no copy step, and don't reach into `dist/wasm/web/`.    |
| webpack 5           | Yes                | None.                                                          |
| Rollup (standalone) | No                 | Add `@web/rollup-plugin-import-meta-assets`, or copy manually. |
| esbuild             | No                 | Copy manually (see below).                                     |
| Bun (`bun build`)   | No                 | Copy manually (see below).                                     |

esbuild and Bun pass `new URL(..., import.meta.url)` through verbatim: the build
succeeds and the glue chunk is emitted, but no `.wasm` is written and the worker
404s at runtime. No flag changes this — `--loader:.wasm=file` only fires on
`import` statements, never on `new URL`. Hosts on those bundlers must copy
`truapi_server_bg.wasm` out of `@parity/truapi-host/dist/wasm/web/` into the same
output directory as the emitted `truapi_server-*.js` chunk, since the glue
resolves the payload relative to its own URL.

Running Vite under Bun (`bunx --bun vite build`) uses Vite's bundler and is
unaffected; only `bun build` is.

The literal import makes the worker a code-split chunk, so a Vite host must ask
for ES workers; the default `iife` format cannot code-split and fails the build:

```ts
export default defineConfig({ worker: { format: "es" } });
```

Only the `.wasm` the glue references is emitted, and bundlers content-hash it —
Vite writes `assets/truapi_server_bg-<hash>.wasm`, webpack writes a bare
`<hash>.wasm`. The `.wasm.gz` / `.wasm.br` sidecars under `dist/wasm/web/`
therefore cannot be copied into a host's output: `gzip_static` /
`brotli_static` serve `<request-path>.gz` / `.br`, and the request path now
carries the bundler's hash. Hosts that serve precompressed assets should
generate them from their own build output, after hashing — either a post-build
pass over `dist` (gzip level 9 and brotli max quality reproduce the sidecars
byte for byte) or a bundler plugin:

```ts
import { compression } from "vite-plugin-compression2";

export default defineConfig({
  worker: { format: "es" },
  plugins: [
    compression({ include: [/\.(js|css|html|wasm)$/], algorithms: ["gzip"] }),
    compression({
      include: [/\.(js|css|html|wasm)$/],
      algorithms: ["brotliCompress"],
    }),
  ],
});
```

webpack hosts get the same result from `compression-webpack-plugin`. Skipping
this ships the full 1.4 MB `.wasm` where about 600 kB (gzip) or 470 kB (brotli)
would do — and a server configured with `gzip_static` but no dynamic `gzip on`
has no fallback.

## Optional capabilities

`HostCallbacks` groups are required except those listed on the Rust
`OptionalPlatform` super-trait, which are emitted as optional members. Omit one
and the core answers its product calls with `Unsupported`; supply it and the
whole group must be implemented:

```ts
const callbacks: HostCallbacks = {
  navigation,
  notifications,
  // ...required groups...
  chat, // optional: leave it out and chat products get `Unsupported`
  permissionStatus, // optional: reports live OS permission state
  pocket, // optional: serves the host's Pocket card collection
  contacts, // optional: leave it out and contacts calls get `Unsupported`
};
```

`permissionStatus.devicePermissionStatus` must answer from the OS without
prompting. Supply it and the core revalidates a stored device grant against it
before answering the product, so a capability the OS has since revoked or reset
stops reading as usable. Omit it and a stored grant answers on its own.

`pocket` serves the host's card collection. `subscribePocketCards` emits the
calling product's cards and every later replacement, and `removePocketCard`
takes one out. The host owns the collection: removing an absent card succeeds,
and a card the host pins is refused with `Privileged`.

Under `createWebWorkerPairingHostRuntime` the presence of each optional group is
reported to the worker in its `init` message, so the core sees the same
capability set on both sides of the boundary.

### Product-rendered bodies

A host can ask a product to draw one body — a chat message, an input-widget
candidate, a Pocket card — and send back what the user does with it. Both entry
points live on the product provider and are present only on runtimes holding a
live channel to the core:

```ts
import type { RenderContext } from "@parity/truapi";

// `payload` here is the product-defined body, hex-encoded.
const context: RenderContext = {
  tag: "ChatMessage",
  value: { roomId, messageId, messageType },
};

const stop = provider.render!(
  { context, payload },
  {
    onUpdate: (node) => setTree(node), // complete replacement tree each time
    onComplete: () => setTree(null),
    onError: (error) => console.warn(error),
  },
);

// A button inside the rendered tree was tapped:
await provider.publishRendererAction!({
  context,
  actionId,
  payload: "0x", // a `Button` press carries no data
});

stop(); // stop rendering; safe to call more than once
```

A `TextField` value change instead carries the UTF-8 bytes of the new value,
with no length prefix.

`render` reports failure through `onError` rather than throwing, so one dead
render cannot take the surrounding surface with it. Exactly one terminal fires
per render: `onComplete` means the last tree delivered stands, `onError` means
it is partial and must not be shown as final. A product that declines the
render, a tree that fails to decode, a closed connection, and a throwing
renderer all arrive as `onError`. An open render holds one worker reference for
the provider's product, released when the stream ends or the disposer runs, so
the product's worker stays up for as long as something is being drawn.

`publishChatAction` is the path for posted messages, commands and host-drawn
`Actions` buttons. Each action entry point sits behind its own service's access
policy: the renderer refuses a connection that is not a `Worker` execution, and
chat additionally requires a live session.

Two rules the core cannot check are the host's to keep: send a render context
only for a surface the product's manifest `includes`, and publish a renderer
action only from the current tree of an open render stream.

## Product account addresses

A host that stores the core's `ProductSubtree` slot can name the account a
review will sign with, so the review can carry an address and a fee rather than
a bare derivation path. Both calls are pure and need no runtime or session, so
`default()` alone is enough:

```ts
import init, {
  deriveProductAccountPublicKey,
  productAccountAddress,
} from "@parity/truapi-host/wasm/web";
import { DerivationIndex } from "@parity/truapi";

await init();

// `subtreePublicKey` is the 32 bytes read from the host's own
// `ProductSubtree { sessionId, productId }` slot.
const publicKey = deriveProductAccountPublicKey(
  subtreePublicKey,
  DerivationIndex.enc(account.derivationIndex),
);
const address = productAccountAddress(publicKey);
```

The index crosses as a SCALE-encoded `DerivationIndex`, the same value a review
already carries, so the 32-byte chain code behind it stays core-owned and a host
never reconstructs it. `productAccountAddress` applies the prefix host-spec C.6
fixes, rather than leaving each host to choose one.

### The same name in a test suite

`@parity/truapi-host/testing/playwright` exports a second `productAccountAddress`.
It is asynchronous, it takes the account and product to derive rather than a
public key, and it loads the testing WASM bundle itself:

```ts
import { productAccountAddress } from "@parity/truapi-host/testing/playwright";

const address = await productAccountAddress({
  account: "bob", // a dev account name, or a `DevAccount`
  productId: "tx-demo.dot",
  index: 0, // optional, defaults to 0
});
```

Which one to reach for:

- `productAccountAddress(publicKey)` from `@parity/truapi-host/wasm/web` is
  synchronous and formats a subtree-derived public key the host already holds.
  This is the one a host ships.
- `productAccountAddress(query)` from `@parity/truapi-host/testing/playwright`
  is asynchronous and runs the whole derivation from a dev account's session
  root. It needs the built testing bundle, so it is for suites only. Because
  the address depends on nothing but the root, the product id and the index, a
  suite can work it out in a `globalSetup` and fund it once rather than per
  test.

A running fixture answers the same address through
`testHost.getProductAccountAddress(productId?, index?)`, which reads the session
the host actually holds and so returns `undefined` while it is signed out.

`contacts` needs both callbacks, or the group counts as absent. `pickContact`
draws the picker and returns the chosen account, or `NoContacts` when there is
nobody to show. `contacts({ handleKey, handles })` resolves the handles a
transaction names: one entry per handle, in order, the account or `undefined`.
A contact's handle is BLAKE2b-256 keyed with `handleKey` over its 32-byte
account (`blake2b(account, { key: handleKey, dkLen: 32 })` in `@noble/hashes`).
The core re-checks every account returned. It caches what it resolves, so call
`notifyContactsChanged()` whenever a contact is removed or blocked. Omit blocked
contacts from both. See the contacts RFC (`docs/rfcs/contacts-api.md`).

## Generated WASM artefacts

The ignored bundle under `dist/wasm/web/` is built with host-owned chain access.
Hosts wire their JSON-RPC provider through `chainConnect`; if they omit it,
chain calls fail with the core's standard unavailable error. Release builds use
the workspace size-optimized Rust profile plus `wasm-opt -Oz`, validate that
debug/name/producers custom sections were stripped, and emit `.wasm.gz` and
`.wasm.br` sidecars for hosts that serve precompressed assets.

Build them after editing `rust/crates/truapi` and before packaging, publishing, or running
tests that load the raw WASM bundle (requires `wasm-pack` on PATH):

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host/worker-runtime?worker";
import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";

const runtime = await createWebWorkerPairingHostRuntime(
  new HostWorker(),
  callbacks,
  {
    hostConfig,
  },
);

const firstProvider = await runtime.createProvider({ productId: "first.dot" });
const secondProvider = await runtime.createProvider({
  productId: "second.dot",
});
```

`@parity/truapi-host/web` also exports `createIframeHost` for the
protocol-iframe MessageChannel handshake. Host code creates one worker runtime
and then opens one provider per product id.

## Session lifecycle

The core owns the session; the host owns persistence. At boot the core restores
the `AuthSession` slot on its own and reports the outcome through the `auth`
callback, `Disconnected` included, so a host waits for the first
`authStateChanged` instead of treating silence as "signed out". Every
transition below reports the resulting `AuthState` the same way.

| Runtime method                  | Use it to                                                                    |
| ------------------------------- | ---------------------------------------------------------------------------- |
| `activateStoredSession()`       | Await the restore of the `AuthSession` slot before opening providers.        |
| `activateExternalSession(blob)` | Install a session the host holds itself, without writing it to core storage. |
| `notifySessionStoreChanged()`   | Tell the core the persisted blob may have changed; it re-reads it.           |
| `notifyContactsChanged()`       | Tell the core a contact was removed or blocked; it drops cached handles.     |
| `disconnectSession()`           | Log out: clears the session and notifies the peer.                           |
| `resetSessionState()`           | Drop the local session without notifying the peer.                           |

The boot order is create the runtime, restore, then open providers:

```ts
const runtime = await createWebWorkerPairingHostRuntime(
  new HostWorker(),
  callbacks,
  { hostConfig },
);

// Resolves once product frames may use the restored session; rejects when
// there was nothing to restore.
await runtime.activateStoredSession().catch(() => {});

const provider = await runtime.createProvider({ productId: "first.dot" });
```

### Statement-store traffic of the host's own

A host that runs its own statement-store traffic — P2P chat, device sync — signs
with the account its device advertises in the pairing QR. The paired wallet
registers that account's statement-store allowance while answering the
handshake, so it is the only account whose statements the network accepts from
this host. A locally minted key gets no allowance, and every submission fails
with `no allowance set for account`.

| Value                                    | Where                                  |
| ---------------------------------------- | -------------------------------------- |
| `SessionUiInfo.deviceStatementAccountId` | On every `AuthState.Connected`         |
| `getDeviceStatementKey()`                | Runtime method, 64-byte sr25519 secret |

Peers are told to address that same account, so it is also what a host derives
its own statement topics from. Both are `undefined` without an active session,
and the account is rotated per login: read it from the current session rather
than caching it across sign-ins.

## Worker lifecycle

A product has one worker, and the core keeps one reference count per worker. The host takes a reference while a
modality holder is on screen or in flight, such as a chat room the product serves, and releases it when the holder
leaves. The host runs and stops the worker executable itself; the runtime only tells it when demand crosses zero.

| Runtime method                    | Use it to                                                        |
| --------------------------------- | ---------------------------------------------------------------- |
| `acquireWorker(productId)`        | Take one reference while a holder is on screen or in flight.     |
| `releaseWorker(productId)`        | Release one reference; with none held it is a no-op.             |
| `subscribeWorkerDemand(listener)` | Learn which workers to run: current set first, then each change. |

```ts
const stop = runtime.subscribeWorkerDemand(({ productId, wanted }) => {
  if (wanted) startProductWorker(productId);
  else stopProductWorker(productId);
});

runtime.acquireWorker("chat-bot.dot"); // a room it serves came on screen
runtime.releaseWorker("chat-bot.dot"); // the room left the screen
```

Two holders of one product hold one worker: the listener hears `wanted: true` once, on the first, and `wanted: false`
once, after the last. A `wanted: false` is permission to stop, not an order: a host may keep the worker warm.

## Debugging (dev-only)

The worker can stream every product↔core wire frame to the wire debugger. It is
off by default and the embedding host decides; the product needs no changes.
Two conditions must **both** hold or nothing dials and the core installs no tap:

1. **The host page is a dev build.** The dial sits behind a hard
   `import.meta.env.DEV` gate, which bundlers replace with a boolean literal: in
   a production bundle that gate is false, so no option can turn the tap on. A
   production build that shows no frames is this gate, not a broken debugger.
   `NODE_ENV=development` is what opens the gate under Vite.
2. **A `ws://` loopback URL reaches the runtime**, from one of two places. The
   host's own value wins over the build's, so the build's is a default and never
   an override:

   ```ts
   // 1. the host passes it, the normal path, where the host stays in control
   await createWebWorkerPairingHostRuntime(worker, callbacks, {
     hostConfig,
     debugger: "ws://127.0.0.1:9231", // null or "" refuses the dial outright
   });
   ```

   ```bash
   # 2. or compile a default in, which is what a local stack does: every
   #    browser profile that opens the build dials, with nothing to switch on
   VITE_TRUAPI_DEBUGGER_URL=ws://127.0.0.1:9231 vite build
   ```

   Passing `null` or `""` is how a host refuses the dial even when the build
   carries one; omitting the field takes the build's value.

   While a dial is live the host shows a small fixed-position badge naming every
   endpoint frames are going to, so a tap left on from an earlier session is
   visible rather than buried in a console line. Pass `debuggerIndicator: false`
   to suppress it, and only when the host renders its own signal, since the
   point is that a host streaming frames is never silent about it. One runtime
   suppressing the badge leaves another runtime's badge alone.

   The dial is resolved once, when the runtime is created, and cannot be changed
   from the page afterwards, so whether this session is observed is a property
   of the build and the host, not of anything typed into a console later.

Run the debugger at the other end (`@parity/truapi-debugger`, `npm run serve`,
`127.0.0.1:9231`). On the next runtime boot the worker dials that URL and (via
the Rust core's `DebugSink` tap) sends each frame as `{ channelId, dir, frame }`.

The URL must be `ws://` on a loopback host. Anything else — `wss://`, `http://`,
a LAN or public address, a non-loopback hostname — yields an inert link and a
`wire debugger URL rejected` console warning; there is no certificate or `wss`
path. Prefer the literal `127.0.0.1` over `localhost`: `localhost` passes the
gate, but it resolves `::1` first on macOS while the debugger binds `127.0.0.1`
alone, so the same URL handed to a native host (`truapi`'s `WsDebugSink`
dials the first resolved address) silently never connects.

The debugger owns all decoding and decodes every frame it can, including signing
and payment payloads; its safety is the dev-build gate above, not redaction. See
`js/packages/truapi-debugger/README.md` for the tap, the envelope, and the
host-dials-debugger topology.

## Publishing

This package is published by the root `Release` workflow through
`paritytech/npm_publish_automation`. Do not run `npm publish` locally. Cut a
`release:` PR with a changeset for `@parity/truapi-host`; the workflow builds
the generated host bindings, the browser WASM bundle, packs the tarball, and
publishes it when the `@parity/truapi-host@<version>` tag does not already
exist.

## Architecture

```text
JS host code
  protocol handlers / typed callbacks
  (types from @parity/truapi-host)
       |
       v
createWebWorkerPairingHostRuntime
  shared worker runtime: pairing session, chain runtime, WASM instance
       |
       +-- createProvider({ productId }) -> product core / WireProvider
       |
       +-- createProvider({ productId }) -> product core / WireProvider
```
