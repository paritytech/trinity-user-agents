# @parity/truapi-host

WASM-backed TrUAPI host runtime. It embeds the `truapi` Rust core (compiled to WASM) behind a Web Worker provider, plus
per-environment integration entry points. It is the counterpart to the native Android/iOS host shells.

## Entry points

The package exposes tree-shakeable subpath exports — import only what your environment needs:

| Import                                     | Provides                                                                                                                             |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| `@parity/truapi-host`                      | Shared runtime types plus generated typed host callback contracts.                                                                   |
| `@parity/truapi-host/web`                  | Browser pairing and signing hosts: `createIframeHost`, `createWebWorkerPairingHostRuntime`, and `createWebWorkerSigningHostRuntime`. |
| `@parity/truapi-host/worker-runtime`       | Web Worker entrypoint (import with your bundler's `?worker` suffix) so the WASM core runs off the page main thread.                  |
| `@parity/truapi-host/wasm/web`             | The raw browser `wasm-bindgen` glue, if you need to instantiate the core yourself.                                                   |
| `@parity/truapi-host/testing`              | `createMockHost`: the in-memory host seam a product is tested against.                                                               |
| `@parity/truapi-host/testing/playwright`   | Playwright fixture that boots the test host and embeds the product in an iframe.                                                     |
| `@parity/truapi-host/testing/server`       | Node server that serves the host page and its bundle.                                                                                |
| `@parity/truapi-host/testing/client`       | `createMockClient`: a product client over the mock with no iframe, for unit tests.                                                   |
| `@parity/truapi-host/testing/dev-accounts` | Named dev accounts derived from fixed BIP-39 entropy, which sign for real.                                                           |
| `@parity/truapi-host/testing/host-page`    | The browser half the fixture drives, for a suite that boots its own page.                                                            |
| `@parity/truapi-host/wasm/testing`         | The raw glue for the signing-enabled bundle the test host runs on.                                                                   |
| `@parity/truapi-host/browser-receiving` | Trusted host-page client for the durable service-worker receiving owner. |
| `@parity/truapi-host/browser-receiving-worker` | Service-worker installation helper with an injected canonical WASM factory. |

`scripts/build-wasm.mjs` builds two WASM bundles, both `--no-default-features`. `wasm/web` is the production browser
host and excludes `WasmSigningHostRuntime`; `wasm/testing` adds the Rust `wasm-signing-host` and `test-host` features,
which is what lets the test host hold keys and answer resource allocation as granted without allocating anything. A real
browser wallet instead needs a web bundle built with `--no-default-features --features wasm-signing-host`, without
`test-host`. That enables native signing and wallet administration without the testing-only allocation shortcuts. Build
that wallet variant with `npm run build:wasm -- --web-only --signing-host`. The default web build remains pairing-only.
Run the package tests against the generated web/testing bundles, then exercise the consuming host's real worker with
the selected variant. `ProductRuntimeConfig` configures the pairing host and requires no network suffix. The signing
constructor's configuration requires `runtimeConfig.networkSuffix` in addition: the bare TLD (`dot`, `paseo`, or
`testnet`) matching the People chain and the wallet's onboarding configuration.

`locale.subscribe` V2 reports the host's BCP 47 language tag and optional actual time-zone identifier.
Implement `locale.localizeTimestamps` with the exported `localizeTimestamps` helper to format batches of up
to 128 Unix-millisecond instants using JavaScript `Intl` and historical daylight-saving rules. Each result
contains a Gregorian `YYYY-MM-DD` grouping key and localized time, date and detailed date/time strings.
Invalid zones and out-of-range instants reject rather than substituting UTC. Publish a new locale
subscription value when the host language or zone changes. A missing time zone means local conversion
is unavailable; it is not UTC. The headless CLI deliberately has no local-time formatting engine.

`runtime.createProvider(product, callbacks?)` optionally binds platform callbacks to one product execution while
retaining the same shared native host. Omit the second argument to use the host's default callbacks. Pass a complete
`WebWorkerHostCallbacks` bundle (not a partial override) when each iframe or worker connection owns its own consent UI.
Dispose that UI scope when the connection closes, provider creation fails, or the whole host retires; a replacement
connection must get a fresh scope. Provider disposal, frame failure, failed creation, and host teardown remove the SDK's
callback route, so stale requests cannot fall through to another connection's callbacks.

Shared authentication, core storage, chain transport, and signing-wallet authority remain owned by the native host; the
optional bundle does not replace them. Raw Wasm consumers can use
`productRuntime(product, coreCallbacks, platformCallbacks?)` for the same execution-local adapters. This is callback
routing isolation, not per-call `AbortSignal` cancellation: hosts must still retire their connection-owned interactive
UI explicitly.

Signing hosts using instance-scoped Coinage must also supply `runtimeConfig.coinageInstanceId` (or
`hostConfig.coinageInstanceId` in the worker factory) from trusted host/network configuration. It is an integer from `0`
through `4294967295`, including zero; strings, fractions and out-of-range values are rejected. Omission preserves legacy
Coinage support, but Coinage operations on an instance-scoped runtime fail closed without it. This asset instance is not
a purse derivation identifier and is never selected by guest product code.

The optional runtime-wide `callbacks.coinageWallet` group registers an existing native main-purse service through
`nativeCoinage(request)`. Omitting the group uses Core's built-in Rust wallet; no explicit backend selector is needed. A
registered native wallet remains authoritative when unavailable, locked, or failing: none of those states enables Rust
custody. Registration is captured when the runtime is constructed and cannot be replaced by product callbacks. Malformed
native groups reject initialization. Native request/response payloads, especially outgoing memos, are Host-private;
infrastructure exceptions are sanitized at the adapter boundary.

`runtimeConfig.assetHub` is required by both configurations, pairing and signing. It is the Asset Hub genesis hash, in
the same shape as `runtimeConfig.people` and `runtimeConfig.bulletin`. Product manifests are read from the dotNS
contracts deployed there, so it is what makes a `trustedProducts` grant resolvable: without a usable value no manifest
resolves, so every cross-product grant not already cached is refused, and the refusal is indistinguishable from the
other product having granted nothing. A config omitting it is rejected.

When a product requests a Statement Store allowance, the signing runtime first uses `confirmUserAction` for host-owned
approval UI, then registers the product-scoped allowance account through the platform's People-chain connection.
Registration requires the activated wallet root to belong to an eligible personhood ring. A randomly generated browser
account can still sign, but it cannot spend another identity's personhood allowance; use a pairing host with the mobile
Account Holder when that identity remains on the phone.

After activating a local signing session, call `refreshLocalIdentity()` to read the configured Asset Hub's dotNS
ownership and install verified username metadata.
`registerLocalLiteUsername(baseUsername, identityBackendBaseUrl, onProgress?)` authenticates to the identity backend
with native UID proofs, submits the registration with the real RFC-0004 X25519 identifier key and Asset Hub time, then
monitors chain ownership until it is confirmed or the activation is disconnected/replaced. Backend acceptance alone is
not registration success. Slow confirmation and transient chain-read failures do not resubmit the claim or require
repeated manual refreshes.

The optional callback receives `LocalIdentityProgress` (exported from `@parity/truapi-host/web`): `checking`,
`authenticating`, `submitting`, `confirming`, or `retrying` with a chain-read `error`. Stages reflect actual work, not
elapsed-time estimates. `confirming` means the backend accepted the request, not that the username is owned yet; only
the resolved promise confirms ownership. A retry does not submit another registration. Observer exceptions do not
interrupt the operation, and settled or disposed requests receive no further progress.
Both methods return `LocalIdentity` (exported from `@parity/truapi-host/web`): the canonical lowercase `0x`-prefixed
`identityAccountId` and an optional verified `liteUsername`. The backend must allow the worker's origin, or the host
must provide an approved same-origin proxy. Secret material stays in the signing runtime. Disconnecting or replacing the
local activation invalidates an in-flight identity operation; concurrent identity operations are rejected.

### Read-only wallet allowance inspection

After activating a local signing session, trusted shell code can call:

```ts
const snapshot = await runtime.getWalletAllowanceSnapshot(["example.paseo"]);
```

The result is a `WalletAllowanceSnapshot` exported from `@parity/truapi-host/web`. It reports Statement Store publishing
slots, PGAS claim capacity and balances, and Bulletin claim capacity and storage quotas. This is a signing-host admin
API, not a product protocol method. A bare local identity can inspect it without registering or refreshing a username.
Inspection never allocates, renews, signs, or changes wallet storage.

Pass at most 32 unique canonical product IDs from trusted shell state. An empty list still inspects wallet-wide slots
and claims. PGAS balances cover each requested product's `Index(0)` account; Bulletin quotas use its distinct storage
allowance account. Other products, derivations, spending and historical attribution are outside this snapshot.

Each section is independently available or unavailable. Available values carry finalized chain provenance and chain
time, with exact integer amounts and runtime-defined limits. PGAS claims also carry `membershipObservation` for the
separate People membership source. Missing metadata stays unknown, and a nonmember's policy limit is not presented as
usable capacity.

Activation changes, disconnect, disposal and worker failure invalidate pending reads. The worker API rejects after 30
seconds rather than leaving the caller pending; native read work may finish later, but cannot populate a replaced
wallet. Shells must additionally fence their selected network/product context.

## Durable browser receiving

Compose `installBrowserReceivingWorker` into the existing root-scoped host service
worker. It owns opaque canonical receiving ledger bytes in IndexedDB, serializes
core access with Web Locks across worker replacement, and uses the generic relay
v2 WebPush endpoints. Products never receive its authority registry, transport
credentials, or raw host hooks.

Transport synchronization matches the complete product, account, environment,
artifact, genesis and generation scope. Disabled records from an older scope do
not revoke or block acknowledgement of a current enrollment. Explicit product
revocation and host logout still revoke every applicable enrollment.

```ts
import init, { WasmNotificationReceiver } from "@parity/truapi-host/wasm/web";
import { installBrowserReceivingWorker } from "@parity/truapi-host/browser-receiving-worker";

const ready = init({
  module_or_path: new URL("/receiving/truapi_server_bg.wasm", self.location.origin),
});
installBrowserReceivingWorker({
  scope: self,
  createReceiver: async callbacks => {
    await ready;
    return new WasmNotificationReceiver(callbacks);
  },
  relayUrl: RECEIVING_RELAY_URL,
  pushOrigin: self.location.origin,
  hostEntryUrl: "/",
  notificationTitle: "New activity",
});
```

Build this entry with the host's existing service-worker build. A classic worker
can be bundled with `esbuild host-sw.ts --bundle --format=iife --platform=browser
--outfile=public/sw.js`; preserve existing cache/install/push handlers. Do not use
dynamic `import()` in a service worker. Copy the matching
`dist/wasm/web/truapi_server_bg.wasm` to the explicit URL above, permit that
same-origin asset and WASM compilation in CSP, and keep its cache revision tied
to the bundled glue. A standalone receiving artifact does not replace a host's
different PolkaVM runtime artifact.

The page imports `createBrowserReceivingClient` from `/browser-receiving` with its
existing `ServiceWorkerRegistration`, a real host `consent(authority, watches)`
prompt, and an `activate(authority, event)` callback. Activation returns true
only after the exact verified product is ready in the correct unlocked account
and environment. It must never silently switch accounts or navigate `event.route`
as a URL. That route is an opaque product token.

Call `setActiveAccount(account, environment, genesis)` only from the host's current,
authenticated account selection, before updating or binding product authority.
Fence asynchronous login callbacks and stale tabs before this call. Replacing
that scope durably revokes mismatched authorities, including products no longer
open. Passing `undefined` pauses authority without discarding consent; do not use
it for ordinary page/product closure, suspension, or a transient network outage.

Read `getAuthority(productId)` to recover the durable generation. It returns the
canonical authority plus `revoked`; reuse a live matching scope's generation
across reloads, and increment it for account/artifact replacement or explicit
re-enrollment after revocation. Call `updateAuthority` only with host-verified
scope, then `bindExecution` with an immutable snapshot. Forward page-core
`receiverCommand` through that execution's `command(action, payload)`, checking
the callback product ID against the trusted execution closure. The worker calls
canonical `commandForExecution`, including its post-consent scope recheck.
Call execution `ready()` once the verified product is ready and `close()` on
suspension. Suspension/lock retains enrollment. Explicit logout/account removal
must await local `revokeAll()` before forgetting identity; it covers closed
products and retains durable remote-deletion intent. Use scoped `revoke(productId)`
for artifact replacement or removing one product. Neither waits for relay deletion.

Call `enableWebPush(vapidPublicKey)` directly from a user gesture. Obtain the
trusted relay's VAPID key from `GET /v2/config`, not a product-provided URL.
`refresh()` refreshes the browser destination; online, push, worker activation
and supported background/periodic sync events retry durable transport work.
Network requests have a ten-second abort deadline and bounded responses; retry
backoff and pending deletion survive worker termination. Standard WebPush
subscriptions use `userVisibleOnly: true`. Invalid, stale, revoked or
foreground-receipted wakes never cause a fabricated notification.

The core alone gates authenticated ingress, foreground receipts, reservations
and click activation. The worker confirms display only after `showNotification`
resolves and cancels reservations only on explicit display failure. Retained
v2 responses contain a carrier and actual source metadata, so this adapter uses
`receivingIngest`, not `receivingIngestStatement`.

Relay watches retain each original core-consent expiry, at most 30 days, without
a separate 24-hour lease or dependence on periodic browser execution. Transport
rotation and retries never extend that expiry; renewal beyond it requires fresh
core consent. Event/header freshness remains independently bounded to 24 hours.
Browser background scheduling is not guaranteed. Unsupported Web Locks, WebPush, Ed25519
WebCrypto, denied notification permission, or unavailable WASM are not simulated
as successful support. Physical page-closed delivery still requires a secure
origin, valid relay/VAPID configuration and browser/provider qualification.

## Bundler requirements

The worker imports the WASM glue by a literal specifier, so every bundler resolves it statically and emits
`truapi_server.js` as a chunk. Whether the `truapi_server_bg.wasm` payload comes with it depends on the bundler:
emitting it requires treating `new URL("truapi_server_bg.wasm", import.meta.url)` inside the glue as an asset reference,
and not all of them do.

| Bundler             | Emits the `.wasm`? | Host action                                                    |
| ------------------- | ------------------ | -------------------------------------------------------------- |
| Vite                | Yes                | None — no copy step, and don't reach into `dist/wasm/web/`.    |
| webpack 5           | Yes                | None.                                                          |
| Rollup (standalone) | No                 | Add `@web/rollup-plugin-import-meta-assets`, or copy manually. |
| esbuild             | No                 | Copy manually (see below).                                     |
| Bun (`bun build`)   | No                 | Copy manually (see below).                                     |

esbuild and Bun pass `new URL(..., import.meta.url)` through verbatim: the build succeeds and the glue chunk is emitted,
but no `.wasm` is written and the worker 404s at runtime. No flag changes this — `--loader:.wasm=file` only fires on
`import` statements, never on `new URL`. Hosts on those bundlers must copy `truapi_server_bg.wasm` out of
`@parity/truapi-host/dist/wasm/web/` into the same output directory as the emitted `truapi_server-*.js` chunk, since the
glue resolves the payload relative to its own URL.

Running Vite under Bun (`bunx --bun vite build`) uses Vite's bundler and is unaffected; only `bun build` is.

The literal import makes the worker a code-split chunk, so a Vite host must ask for ES workers; the default `iife`
format cannot code-split and fails the build:

```ts
export default defineConfig({ worker: { format: "es" } });
```

Only the `.wasm` the glue references is emitted, and bundlers content-hash it — Vite writes
`assets/truapi_server_bg-<hash>.wasm`, webpack writes a bare `<hash>.wasm`. The `.wasm.gz` / `.wasm.br` sidecars under
`dist/wasm/web/` therefore cannot be copied into a host's output: `gzip_static` / `brotli_static` serve
`<request-path>.gz` / `.br`, and the request path now carries the bundler's hash. Hosts that serve precompressed assets
should generate them from their own build output, after hashing — either a post-build pass over `dist` (gzip level 9 and
brotli max quality reproduce the sidecars byte for byte) or a bundler plugin:

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

webpack hosts get the same result from `compression-webpack-plugin`. Skipping this ships the full 1.4 MB `.wasm` where
about 600 kB (gzip) or 470 kB (brotli) would do — and a server configured with `gzip_static` but no dynamic `gzip on`
has no fallback.

## Optional capabilities

`HostCallbacks` groups are required except those listed on the Rust `OptionalPlatform` super-trait, which are emitted as
optional members. Omit one and the core answers its product calls with `Unsupported`; supply it and the whole group must
be implemented:

```ts
const callbacks: HostCallbacks = {
  navigation,
  notifications,
  // ...required groups...
  chat, // optional: leave it out and chat products get `Unsupported`
  permissionStatus, // optional: reports live OS permission state
  pocket, // optional: serves the host's Pocket card collection
  profile, // optional: shows profiles and draws contact avatars in host UI
  contacts, // optional: leave it out and contacts calls get `Unsupported`
};
```

`permissionStatus.devicePermissionStatus` must answer from the OS without prompting. Supply it and the core revalidates
a stored device grant against it before answering the product, so a capability the OS has since revoked or reset stops
reading as usable. Omit it and a stored grant answers on its own.

`pocket` serves the host's card collection. `subscribePocketCards` emits the calling product's cards and every later
replacement, and `removePocketCard` takes one out. The host owns the collection: removing an absent card succeeds, and a
card the host pins is refused with `Privileged`.

`profile.presentProfile` shows the profile a product references in host-owned UI and resolves once it is shown, not
when the user dismisses it. The reference is a bearer capability: the host fetches, decrypts and renders it, and the
profile's bytes never return to the product. The core forwards only references that are non-empty, at most 2048 bytes
and printable ASCII without whitespace; parsing the format is the host's.

`profile.presentContactProfile(product, presented)` opens host-owned contact profile UI when a product calls
`profile.presentContact`. `presented` carries the `peerIdentity`, an optional `shared` record containing the bearer
`reference` and `sharedAt` freshness timestamp (`bigint`), and the contact's optional verified `username`.
An absent `shared` requests friendly empty-profile feedback, not a fetch or an error. The username is the one the core's
Chat roster verified for that contact, else the contact's verified dotNS name, looked up for at most 2 seconds; never a
name from the product. Without one, name the contact generically, never by address. It names who sent the reference,
not whose profile it is: the record is not signed by its owner, and a contact can forward someone else's. Same contract
as `presentProfile` otherwise. The default adapter can present a shared reference through `presentProfile`;
hosts implement `presentContactProfile` to show empty-profile feedback. V2 never reports availability or rendering
failures to the product. A failed reference lookup is not represented as an empty profile.

`profile.placeContactAvatars(product, placed)` draws contacts' avatars over a chat product. `placed` carries the
product's surface size and, per avatar, the product's `slot` id, a square `rect`, the `clip` region it is cut to, all in
surface units (framebuffer pixels for a PolkaVM product, CSS pixels of the viewport for a web product), and the
`reference` that contact disclosed, so the host can draw their photo and mood ring, with a `sharedAt` freshness token
(`bigint`). Contact tokens use Unix ms, advanced monotonically for personal revisions across relay actors; the own
avatar uses the disclosure revision, not a date. A changed token invalidates cached contents. Each call replaces what was
drawn for the product; an empty `avatars` clears it. The core calls it again with the same geometry when a contact
shares, re-shares or withdraws a profile, and with no avatars when the product's connection goes away. Draw on a layer
the product cannot
read that lets pointer input through, and never tell the product what was drawn. The host runtimes take
`RequiredHostCallbacks`, so a `profile` group implements it and `presentContactProfile` alongside `presentProfile`.

`profile.disclose` needs no `profile` group, but the first call from a product asks the user through
`userConfirmation.confirmPermission` with a `ProfileDisclosure` review naming that product. V1 shares app-scoped
references with every ready Chat contact; V2 can select apps or opaque Contacts handles. Personal grants are
host-renderable across recipient apps. The answer is kept like any other permission, as `ProfileDisclosure`.
Audience mutations currently reuse that product-level consent. A host that cannot render the
review should reject the call rather than answer `Deny`: the product is refused, but no refusal is remembered.

`presentContact` V2 accepts peer or Contacts-handle selectors and hides sharing availability; V1 remains app-only.
`placeContactAvatars` V3 accepts those selectors alongside the V2 own slot. V1/V2 placement bytes remain compatible.
Hosts must call `notifyContactsChanged()` after directory changes so stale handle resolution and overlays clear.
These APIs do not create a Chat channel or a group editor. See the
[Profile RFC](../../../docs/rfcs/profile-disclosure.md) for audience, transport and withdrawal semantics.

Under `createWebWorkerPairingHostRuntime` the presence of each optional group is reported to the worker in its `init`
message, so the core sees the same capability set on both sides of the boundary.

### Product-rendered bodies

A host can ask a product to draw one body — a chat message, an input-widget candidate, a Pocket card — and send back
what the user does with it. Both entry points live on the product provider and are present only on runtimes holding a
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

A `TextField` value change instead carries the UTF-8 bytes of the new value, with no length prefix.

`render` reports failure through `onError` rather than throwing, so one dead render cannot take the surrounding surface
with it. Exactly one terminal fires per render: `onComplete` means the last tree delivered stands, `onError` means it is
partial and must not be shown as final. A product that declines the render, a tree that fails to decode, a closed
connection, and a throwing renderer all arrive as `onError`. An open render holds one worker reference for the
provider's product, released when the stream ends or the disposer runs, so the product's worker stays up for as long as
something is being drawn.

`publishChatAction` is the path for posted messages, commands and host-drawn `Actions` buttons. Each action entry point
sits behind its own service's access policy: the renderer refuses a connection that is not a `Worker` execution, and
chat additionally requires a live session.

Two rules the core cannot check are the host's to keep: send a render context only for a surface the product's manifest
`includes`, and publish a renderer action only from the current tree of an open render stream.

## Product account addresses

A host that stores the core's `ProductSubtree` slot can name the account a review will sign with, so the review can
carry an address and a fee rather than a bare derivation path. Both calls are pure and need no runtime or session, so
`default()` alone is enough:

```ts
import init, { deriveProductAccountPublicKey, productAccountAddress } from "@parity/truapi-host/wasm/web";
import { DerivationIndex } from "@parity/truapi";

await init();

// `subtreePublicKey` is the 32 bytes read from the host's own
// `ProductSubtree { sessionId, productId }` slot.
const publicKey = deriveProductAccountPublicKey(subtreePublicKey, DerivationIndex.enc(account.derivationIndex));
const address = productAccountAddress(publicKey);
```

The index crosses as a SCALE-encoded `DerivationIndex`, the same value a review already carries, so the 32-byte chain
code behind it stays core-owned and a host never reconstructs it. `productAccountAddress` applies the prefix host-spec
C.6 fixes, rather than leaving each host to choose one.

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

The optional `contacts` group resolves handles through `contacts({ handleKey, handles })`:
one entry per handle, in order, the account or `undefined`. `pickContact` draws a single
picker and returns the chosen account. `pickContacts(product, { selected })` edits a
complete selection of at most 256 resolved accounts, returning `Picked { accounts }`,
`Dismissed`, or `NoContacts`. A confirmed empty array is `Picked`, not dismissal.
Missing picker callbacks answer `Unsupported`.
A contact's handle is BLAKE2b-256 keyed with `handleKey` over its 32-byte
account (`blake2b(account, { key: handleKey, dkLen: 32 })` in `@noble/hashes`).
The core re-checks every account returned. It caches what it resolves, so call
`notifyContactsChanged()` whenever a contact is removed or blocked. Omit blocked
contacts from both. See the contacts RFC (`docs/rfcs/contacts-api.md`).

`placeContactLabels(product, placed)` receives surface dimensions and
`labels: [{ slot, account, rect, clip }]`. Draw names from the host's contact directory,
using an account fallback when no username exists. Profile-photo absence must not
hide a name. Keep this UI host-owned: return no label or per-slot availability.
Return `true` when the host supports label placement, even when no contact resolves.
Return `false` when that UI is unsupported; the adapter supplies this answer when
the callback is omitted. This capability acknowledgment never reports individual
contact availability. Background Workers are denied label placement.
Empty placements clear the previous names and cancel queued refreshes. Clear names
and cancel pending work on frame load, navigation or disconnect. On same-wallet
directory invalidation, clear stale names and refresh the latest live placement
without waiting for the product to resend it. The core serializes placements per
connection and rejects selections from changed sessions.

Browser signing hosts can back this UI with `runtime.getNativeChatContacts()`. It returns
`{ walletPublicKey, genesisHash, contacts: [{ peerIdentity, username? }] }` to trusted host code only.
The directory restores encrypted native Chat actors, checks their current authorization, includes only authenticated
ready peers, and deduplicates identities. Conflicting verified names are omitted. It is not a product or SSO API;
pairing hosts reject it. Bind the result to the active signing public key and People genesis, never to a username.
Native departures/revocations remove readiness; product-private block lists are not a separate directory source.

Actors are indexed when opened. Historical unindexed products must be opened once on the upgraded host; private storage
has no enumeration API. Directory reads share native commit gates, and native state/session/permission changes invalidate
cached handles. A browser adapter must also cancel pending picker/lookup work when its wallet, network, provider or
directory generation changes. Provider-scoped `contacts` callbacks control that provider's UI; absent overrides inherit
the runtime-wide Contacts adapter. Keep the runtime-wide source alive for host-owned rendering until the owner closes.

## Generated WASM artefacts

The ignored bundle under `dist/wasm/web/` is built with host-owned chain access. Hosts wire their JSON-RPC provider
through `chainConnect`; if they omit it, chain calls fail with the core's standard unavailable error. Release builds use
the workspace size-optimized Rust profile plus `wasm-opt -Oz`, validate that debug/name/producers custom sections were
stripped, and emit `.wasm.gz` and `.wasm.br` sidecars for hosts that serve precompressed assets.

The core stays an `rlib` for Rust/no_std consumers. This build explicitly requests a `cdylib` with `cargo rustc`, then
runs `wasm-bindgen` and `wasm-opt` using the profile settings in the core's Cargo metadata. The separate
`truapi-verifiable` module still builds with `wasm-pack`; its optimized hash is embedded into both cores.
`TRUAPI_WASM_PROFILE` accepts `release` (default), `dev`, or `profiling`, with the same profile behavior as wasm-pack.

Prerequisites on PATH:

- Rust with `rustup target add wasm32-unknown-unknown`.
- `wasm-pack` 0.14.0 (`cargo install wasm-pack --version 0.14.0 --locked`).
- `wasm-bindgen-cli` at the exact resolved `wasm-bindgen` version in `Cargo.lock`
  (`cargo install wasm-bindgen-cli --version <resolved-version> --locked --force`). The build rejects a mismatched CLI
  and prints the required install command.
- Binaryen 117's `wasm-opt`, matching wasm-pack 0.14.0's optimizer. Download the appropriate
  [Binaryen version_117 archive](https://github.com/WebAssembly/binaryen/releases/tag/version_117) and add its `bin`
  directory to PATH.

From the repository root, `bash scripts/install-wasm-artifact-tools.sh` installs the matching bindgen CLI and
checksum-verified Binaryen archive, then prints the directory to add to PATH. CI uses the same installer.

Build after editing `rust/crates/truapi` and before packaging, publishing, or running tests that load the raw WASM
bundle:

```bash
npm run build:wasm   # or `make wasm` from the repo root
```

## Example — browser (Web Worker)

```ts
import HostWorker from "@parity/truapi-host/worker-runtime?worker";
import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";

const runtime = await createWebWorkerPairingHostRuntime(new HostWorker(), callbacks, {
  hostConfig,
});

const firstProvider = await runtime.createProvider({ productId: "first.dot" });
const secondProvider = await runtime.createProvider({
  productId: "second.dot",
});
```

`@parity/truapi-host/web` also exports `createIframeHost` for the protocol-iframe MessageChannel handshake. Host code
creates one worker runtime and then opens one provider per product id.

When UI callbacks capture a product label, pass that product's typed callbacks as the second argument to
`runtime.createProvider(product, callbacks)`. The worker routes these callbacks to that execution without replacing the
shared signing authority, main purse, native Chat authority, or private core storage. Wallet callbacks always use the
runtime-wide bundle, even when a provider supplies overrides. Disposing a provider does not dispose the owner runtime. A
signing host keeps one owner alive for wallet recovery, not ordinary Chat reception, and must not run independent
signing runtimes against the same purse inventory.

`createBrowserNativeChatFilesHost(sourceStore?)` accepts an optional `BrowserNativeChatFileSourceStore` with
`putSources`, `readSource` and `releaseSource`. Use it when core custody spans host origins: immutable file sources must
remain reachable wherever the persisted actor is restored. `putSources` must commit all supplied Blob snapshots durably
before resolving; `readSource` returns that snapshot, not a mutable filesystem reference. Picker/export consent and
bounded reads remain in the SDK. The default source store is origin-local IndexedDB with private immutable Blobs, not
application encryption at rest. An embedder owns disposal of a file host it supplies.

## Session lifecycle

The core owns the session; the host owns persistence. At boot the core restores the `AuthSession` slot on its own and
reports the outcome through the `auth` callback, `Disconnected` included, so a host waits for the first
`authStateChanged` instead of treating silence as "signed out". Every transition below reports the resulting `AuthState`
the same way.

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
const runtime = await createWebWorkerPairingHostRuntime(new HostWorker(), callbacks, { hostConfig });

// Resolves once product frames may use the restored session; rejects when
// there was nothing to restore.
await runtime.activateStoredSession().catch(() => {});

const provider = await runtime.createProvider({ productId: "first.dot" });
```

### Statement-store traffic of the host's own

A host that runs its own statement-store traffic — P2P chat, device sync — signs with the account its device advertises
in the pairing QR. The paired wallet registers that account's statement-store allowance while answering the handshake,
so it is the only account whose statements the network accepts from this host. A locally minted key gets no allowance,
and every submission fails with `no allowance set for account`.

| Value                                    | Where                                  |
| ---------------------------------------- | -------------------------------------- |
| `SessionUiInfo.deviceStatementAccountId` | On every `AuthState.Connected`         |
| `getDeviceStatementKey()`                | Runtime method, 64-byte sr25519 secret |

Peers are told to address that same account, so it is also what a host derives its own statement topics from. Both are
`undefined` without an active session, and the account is rotated per login: read it from the current session rather
than caching it across sign-ins.

## Worker lifecycle

A product has one worker, and the core keeps one reference count per worker. The host takes a reference while a modality
holder is on screen or in flight, such as a chat room the product serves, and releases it when the holder leaves. The
host runs and stops the worker executable itself; the runtime only tells it when demand crosses zero.

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

The worker can stream every product↔core wire frame to the wire debugger. It is off by default and the embedding host
decides; the product needs no changes. Two conditions must **both** hold or nothing dials and the core installs no tap:

1. **The host page is a dev build.** The dial sits behind a hard `import.meta.env.DEV` gate, which bundlers replace with
   a boolean literal: in a production bundle that gate is false, so no option can turn the tap on. A production build
   that shows no frames is this gate, not a broken debugger. `NODE_ENV=development` is what opens the gate under Vite.
2. **A `ws://` loopback URL reaches the runtime**, from one of two places. The host's own value wins over the build's,
   so the build's is a default and never an override:

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

   Passing `null` or `""` is how a host refuses the dial even when the build carries one; omitting the field takes the
   build's value.

   While a dial is live the host shows a small fixed-position badge naming every endpoint frames are going to, so a tap
   left on from an earlier session is visible rather than buried in a console line. Pass `debuggerIndicator: false` to
   suppress it, and only when the host renders its own signal, since the point is that a host streaming frames is never
   silent about it. One runtime suppressing the badge leaves another runtime's badge alone.

   The dial is resolved once, when the runtime is created, and cannot be changed from the page afterwards, so whether
   this session is observed is a property of the build and the host, not of anything typed into a console later.

Run the debugger at the other end (`@parity/truapi-debugger`, `npm run serve`, `127.0.0.1:9231`). On the next runtime
boot the worker dials that URL and (via the Rust core's `DebugSink` tap) sends each frame as
`{ channelId, dir, frame }`.

The URL must be `ws://` on a loopback host. Anything else — `wss://`, `http://`, a LAN or public address, a non-loopback
hostname — yields an inert link and a `wire debugger URL rejected` console warning; there is no certificate or `wss`
path. Prefer the literal `127.0.0.1` over `localhost`: `localhost` passes the gate, but it resolves `::1` first on macOS
while the debugger binds `127.0.0.1` alone, so the same URL handed to a native host (`truapi`'s `WsDebugSink` dials the
first resolved address) silently never connects.

The debugger owns all decoding and decodes every frame it can, including signing and payment payloads; its safety is the
dev-build gate above, not redaction. See `js/packages/truapi-debugger/README.md` for the tap, the envelope, and the
host-dials-debugger topology.

## Publishing

This package is published by the root `Release` workflow through `paritytech/npm_publish_automation`. Do not run
`npm publish` locally. Cut a `release:` PR with a changeset for `@parity/truapi-host`; the workflow builds the generated
host bindings, the browser WASM bundle, packs the tarball, and publishes it when the `@parity/truapi-host@<version>` tag
does not already exist.

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
