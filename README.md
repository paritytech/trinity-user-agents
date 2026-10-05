# TrUAPI

TrUAPI (Triangle User-Agent Programming Interface) is the API surface that hosts like the Polkadot Desktop Browser expose to the products that run inside them. One Rust crate defines the contract, a code generator produces a typed TypeScript client, and hosts and products implement against the same shared types.

> [!WARNING]
> The following is a prototype, reference implementation, and proof-of-concept. This open source code is provided for research, experimentation, and developer education only. This code has not been audited, is actively experimental, and may contain bugs, vulnerabilities, or incomplete features. Use at your own risk.

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/paritytech/trinity-user-agents/ci.yml?branch=main&style=flat-square&label=ci)](https://github.com/paritytech/trinity-user-agents/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-rustdoc-blue?style=flat-square)](https://paritytech.github.io/trinity-user-agents)
[![Playground](https://img.shields.io/badge/playground-live-success?style=flat-square)](https://truapi-playground.paseo.li/)


## Documentation

- [TrUAPI reference](https://docs.polkadot.com/reference/apps/protocol/truapi/)
- [Rust API reference](https://paritytech.github.io/trinity-user-agents/)

<!-- TODO: Add hero screenshot of the playground showing methods + a live call/response. Capture with a screenshot tool, save to `assets/screenshots/playground.png`, then place it here. -->

## Try it

Browse the published Rust API docs at [paritytech.github.io/trinity-user-agents](https://paritytech.github.io/trinity-user-agents).

The interactive playground lets you browse every method, edit request payloads, and call or subscribe to them live against a connected host. It also drives an end-to-end **Diagnosis** that produces a per-host pass/fail report ([playground/README.md → Diagnosis](playground/README.md#diagnosis)). The explorer aggregates those reports into a cross-host **Compatibility** matrix ([explorer/README.md → Host compatibility matrix](explorer/README.md#host-compatibility-matrix)).

**Live:** [truapi-playground.paseo.li](https://truapi-playground.paseo.li/) (open from inside the Polkadot Desktop Browser)

## Install the CLI

`truapi-host` runs a TrUAPI host on your machine, so you can develop and test a product without a phone or a desktop host build:

```bash
curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash
```

Prebuilt for macOS on Apple silicon and Linux on x86_64 and arm64. No Rust toolchain or checkout needed, and it keeps itself up to date. `/script` opens a persistent TypeScript project with the Product SDK quickstart, pinned published dependencies, and editor types. Use `/script --run` to rerun it or `/script --edit` to edit without running. Projects survive session cleanup. See the [`truapi-host-cli` guide](rust/crates/truapi-host-cli/README.md) for setup and existing project scripts. Release checks install and typecheck the default SDK template against the public registry.

`truapi-host signing-host --session <name>` opens an interactive session and
restores or creates its signer. `/session <name>` switches to the saved account
or resumes an unfinished setup. In `exec` mode, `--session` selects the command's
session; inspection and clearing commands do not create accounts. A username
base such as `workbench` selects the most recently created local session with
that base; `workbench.42` selects that exact session. Creating an account from a
session name requires at least six lowercase ASCII letters after digits and
separators are omitted. A new `/session foo` fails immediately as too short;
existing saved accounts and aliases still restore normally.

The signing host registers its built-in full and lite personhood keys when an
authorized product first lists `peopl.<network suffix>` (for example,
`peopl.paseo`). The first listing reads People-chain metadata; later listings
reuse the saved registrations, including after restart. Registration makes the
handles discoverable; proof creation still checks permission and ring membership.
The `listRingVrfKeys` example checks that both built-in keys are discoverable
under `peopl.paseo` on Paseo.

Product scripts and `truapi-host dev` use the same web API permission checks from `js/container`. Dev loads the container through a blocking script tag in your existing browser. Scripts run in Bun and retain filesystem, environment and process access.

To build from source, run `make headless install` with stable Rust, nightly Rust with rustfmt, Node.js 22 or newer, and
Bun installed. The target installs missing workspace build tools and regenerates the Rust and TypeScript sources before
compiling. CI tests this command in both a fresh checkout and one with stale generated files, then runs a product script
through the installed CLI. Code generation and the workspace documentation check reject rustdoc warnings. These checks
are part of the required `CI Status` gate. CLI packaging tests also build an isolated runner and verify it outside the
source checkout.

## Usage

`@parity/truapi` is the low-level generated protocol client. Product apps should normally use a higher-level product SDK, such as [`paritytech/product-sdk`](https://github.com/paritytech/product-sdk), while SDK and host-integration layers can depend on this package directly.

```bash
npm install @parity/truapi
```

```ts
import {
  createClient,
  createMessagePortProvider,
  createTransport,
} from "@parity/truapi";

const transport = createTransport(createMessagePortProvider(port));
const truapi = createClient(transport);

const result = await truapi.accountManagement.accountGet({
  productAccountId: { dotNsIdentifier: "my-product.dot", derivationIndex: { tag: "Index", value: 0 } },
});
```

The transport retries iframe bootstrap until the host channel arrives and rejects unanswered
requests after a bounded deadline; pass `requestTimeoutMs` to `createTransport` to override it.

See [`js/packages/truapi/README.md`](js/packages/truapi/README.md) for the full client reference.

The [permission model](docs/rfcs/0002-permission-model.md) separates outbound domain access from `OpenUrl` external navigation and requires `Notifications` for push delivery. Hosts preserve the user's `AllowOnce`, `AllowAlways`, or `Deny` choice; Rust owns one-use grants for Rust-backed executions.
Android permission prompts belong to one request and close when it finishes or is cancelled,
including cancellation while the app is backgrounded.

The shared Rust core asks blessed products (`peopl`, `dim2` and `stash`,
on every supported network) only for device permissions and legacy-account signing.
All other operations it handles bypass permission prompts and recorded decisions.

## Repository layout

```
rust/crates/
  truapi/                Rust traits, versioned envelopes, latest payload re-exports, and the
                         host runtime (feature `runtime`): dispatcher, typed SCALE logic,
                         chain signing, WASM surface, host syscall traits
  truapi-codegen/        rustdoc JSON to TypeScript client + Rust dispatcher
  truapi-macros/         TrUAPI wire annotations and inter-host SSO proc macros
  truapi-provider/       Network provider backends (WebSocket RPC or smoldot light-client) and chain-access traits
  truapi-verifiable/     Ring-VRF operations over `verifiable`; a lazily loaded WASM module in the browser
js/packages/
  truapi/                  @parity/truapi TypeScript client
  truapi-host/            @parity/truapi-host: WASM-backed host runtime; entries `.`
                          (shared host types), `/web` (iframe + Web Worker),
                          `/worker-runtime`, and the test host: `/testing`
                          (createMockHost), `/testing/playwright`,
                          `/testing/server`, `/testing/client`,
                          `/testing/dev-accounts`, `/testing/host-page`
  truapi-provider/         @parity/truapi-provider: WASM ChainProvider backends
                          (embedded smoldot light client + remote WebSocket RPC)
js/container/              TS lockdown container for the iOS host web view; bundles into
                           ios/truapi-host/Sources/TrUAPIHost/Resources/truapi-container.js
android/truapi-host/       Kotlin host adapter package over the truapi UniFFI core;
                           published to GitHub Packages as io.parity:truapi-host-android
                           (AAR with per-ABI cdylibs; see android/truapi-host/README.md)
android/truapi-provider/   truapi-provider-android: chain transport AAR (bindings + cdylib)
ios/truapi-host/           Swift host adapter package over the truapi UniFFI core
ios/truapi-provider/       TrUAPIProvider Swift package: chain transport over UniFFI
playground/                Interactive Next.js playground (truapi-playground dotNS label)
hosts/ios/                 iOS host app; resolves the core from this tree
hosts/android/             Android host app
hosts/dotli/               dotli host, vendored as a submodule
hosts/imports.json         Source repository and imported revision per host
docs/                      Design docs, RFCs, feature proposals
scripts/codegen.sh         Regenerate the TS client from the Rust source
scripts/refresh-host-import.sh
                           Refresh a vendored host tree from its source repository
scripts/battery.sh         Run the generated battery against both headless CLI host roles,
                           plus the Pocket phase a Worker execution serves
scripts/bundle-size.mjs    Measure the JS and WASM the truapi-* packages ship, against a baseline
```

Taking a screenshot opens **Report app issue** wherever the shake-opened Debug
menu is, which is every build except the store submission: `DEBUG_TOOLS_ENABLED`
on Android, false only for the `release` build type, and `TESTNET_FEATURE` on
iOS, unset only for the `Release` configuration. Android screenshot detection
requires Android 14+.
The modal includes a snapshot of the app screen, a description, and ZIP logs.
Send uploads the report through [issue-proxy](https://github.com/paritytech/issue-proxy).
Configure these Firebase Remote Config string parameters for each mobile environment:

| Parameter | Value |
| --- | --- |
| `issue_proxy_url` | Full HTTPS endpoint, including `/v1/issues` |
| `issue_proxy_api_key` | The proxy's `ISSUE_PROXY_API_KEY`, sent as a bearer token |

Both hosts use the app's existing Remote Config readiness path before reading
the URL and key. There are no bundled defaults; missing configuration shows an
error. Remote Config values are readable by clients, so the GitHub credential
stays on the proxy and must never be placed here. The thank-you popup appears only after HTTP 201. Uploads
include PNG screenshots up to 10 MiB and ZIP logs, with a 25 MiB limit for the
whole multipart request. The Debug menu and **Share logs** remain available.

See the [proc-macro guide](rust/crates/truapi-macros/README.md) for typed SSO handlers, their shared response envelope, and the macro implementation modules.

The Swift host adapter (the `TrUAPIHost` SPM package over the truapi
UniFFI core) lives under [`ios/truapi-host/`](ios/truapi-host), with its SPM
manifest at the repo root (`Package.swift`) so apps can consume it as a git-URL
dependency. The UniFFI bindings and the container bundle are gitignored build
outputs; `scripts/rebuild.sh` regenerates them along with the xcframework
(`make xcframework` + `make uniffi`); see
[`ios/truapi-host/README.md`](ios/truapi-host/README.md).
The container publishes the shared client and a temporary MessagePort adapter for
older SDKs. The adapter's removal is tracked in [#881](https://github.com/paritytech/trinity-user-agents/issues/881);
CLI and iframe MessagePort transports remain supported.
The [container permission boundary](js/container/README.md) documents the protected
operations and the built-ins that remain mutable for product compatibility.
Native bindings expose the canonical Rust domain and protocol value types;
native-only adapter types are limited to lifecycle and callback behavior.
On iOS, a wallet host that manages its own statement-store SSO session can call
`handleSsoRequest` (routes one decrypted remote message through the core,
returning a typed outcome: response bytes to post back, a disconnect marker, or
ignored; a `Cancel` returns at once, so the wallet passes it on without queueing
it behind the request it withdraws) and `prepareDisconnectRequest` (builds the SCALE-encoded wire message
for a wallet-initiated disconnect) on `TrUAPIHostRuntime`. Response posting and
session-record cleanup remain on the wallet side.
See the core's [inter-host SSO design](rust/crates/truapi/RUNTIME.md#inter-host-sso)
for typed handlers, canonical resource types, and consent bound to the signing session.
Product and SSO signing share canonical payloads and the one-byte `OptionBool`
encoding for `with_signed_transaction`.

### JS Host SDKs

JS hosts integrate the Rust core through [`@parity/truapi-host`](js/packages/truapi-host),
a single package with tree-shakeable subpath entries:

- `@parity/truapi-host` (the `.` entry) exposes shared host runtime types and generated callback contracts.
- `@parity/truapi-host/web` wires the WASM provider into a browser host: the iframe
  MessageChannel handshake (`createIframeHost`) plus `createWebWorkerProvider`.
- `@parity/truapi-host/worker-runtime` is the Web Worker entrypoint so the WASM core can
  run off the page main thread.

### Chain transport

A host that serves chain traffic itself embeds the `truapi-provider` crate: an
embedded smoldot light client plus a bundled chain-spec catalog, addressed by
genesis hash, so the host ships no chain specs and never refreshes them. The light
client holds at most 32 connections at once and refuses a `connect` past that, so a
consumer that leaks them fails instead of growing; closing one hands its slot back.
Connections to a remote node, which only the WASM build compiles, are not counted
against it. A light-client connection holds its requests until the chain first
syncs and then forwards them in order; chain-spec queries, statement-store and
Bitswap calls, and the `lifecycle_unstable_*` subscription that reports the sync are forwarded at
once.
Every artifact exposes the sync progress of a running chain (phase, peer count,
stall verdict) as a watch.
The crate
compiles to one binary artifact per platform, each exposing the same
`ChainProvider` contract, so a consumer needs neither a Rust toolchain nor a
dependency on the crate:

- [`@parity/truapi-provider`](js/packages/truapi-provider) is the WASM build for
  browser and webview hosts, rebuilt by `make wasm` alongside the host bundle.
- [`TrUAPIProvider`](ios/truapi-provider) is the second product of the root
  `Package.swift`, an xcframework plus generated Swift bindings, built by
  `make provider-ios`.
- [`truapi-provider-android`](android/truapi-provider) is an AAR carrying the
  Kotlin bindings and the cdylib per ABI, built by
  `make provider-android-publish-local`.

A light client that starts cold warp syncs from the checkpoint in the chain spec, so
every artifact resumes from stored finalized state instead, including the relay a
parachain syncs through. The provider owns when a blob is read and written; the
host owns where the bytes live. The crate stores nothing itself: a host implements
`StorageClient` over storage it already owns, on web and native alike, so it keeps
control of quota and of whether the bytes are backed up or encrypted.

### Wire debugger

[`@parity/truapi-debugger`](js/packages/truapi-debugger) is the consumer for the
payload-blind frame tap in `truapi`. The core streams raw SCALE frames out
of two choke points; the debugger correlates them into per-operation traces,
decodes envelopes and values behind a `TRUAPI_WIRE_SCHEMA_HASH` match, and renders
them through one of two mounts:

- `startDebugServer(...)` is a standalone Bun WS+HTTP server on `127.0.0.1:9231`
  that hosts dial into, so frames from any host reach one inspector.
- `createInAppDebugger(...)` mounts the same engine inside the host page, with no
  server and no dial.

All decoding lives in this package; `@parity/truapi` has no debug seam. Its
[README](js/packages/truapi-debugger/README.md) carries the endpoint list and the
per-host enablement recipe.

`make debugger` brings up the inspector on `:9231` alongside a dot.li host and the
playground. It builds the host with `NODE_ENV=development` on purpose: the dial
sits behind `import.meta.env.DEV`, which a production bundle replaces with `false`,
so `make dev` leaves the board empty with no error.

## How it works

1. The protocol is defined as Rust traits in [`rust/crates/truapi/`](rust/crates/truapi/), with each trait tagged `#[wire_trait(id = N)]` and each method tagged `#[wire(id = N)]` for a stable byte-level `(trait, method)` dispatch table. Every method's doc comment must carry a ` ```ts ` example, which codegen extracts into the playground's EXAMPLE tab; the build fails if any method is missing one.
2. `truapi-codegen` reads rustdoc JSON for that crate and generates the TypeScript client under git-ignored paths in `js/packages/truapi/`.
3. Higher-level SDKs wrap the typed client; the transport encodes SCALE frames and ships them over WebSocket, `MessagePort`, or `postMessage` in iframe mode to the host.
4. The host decodes the frame, dispatches to the matching trait method, encodes the response, and ships it back.

Wire ids are append-only per trait: a trait id is never reassigned and a method id is never renumbered or reused within its trait, so deployed products stay compatible across protocol revisions. New methods take the next free method ids in their own trait and leave every other trait untouched. Trait 255 is permanently reserved for a correlated protocol error, allowing either peer to reject API messages introduced after it was released instead of leaving the caller pending.

## Develop

Common tasks are wrapped in the top-level `Makefile`. Run `make help` for the full list.

```bash
make setup    # submodules + JS dependencies
make build    # Rust workspace + TypeScript client + @parity/truapi-host
make test     # Rust + TypeScript client + @parity/truapi-host tests
make check    # full suite: build, fmt, clippy, test, TS tests, playground build + lint
make wasm     # rebuild truapi WASM artifacts under js/packages/truapi-host/dist/wasm/
```

CI regenerates the shared bindings before building and testing both npm
packages, so generated client and host callback changes are checked together.

The native `truapi-host` utility runs pairing and signing hosts against the real
SSO transport for local end-to-end work. See [Install the CLI](#install-the-cli)
to get it, and the [`truapi-host-cli` guide](rust/crates/truapi-host-cli/README.md)
for its commands and controls.

CLI reserved identities follow the selected network's dotNS suffix. Old account
and pairing stores are left unused as the CLI starts fresh under its
[versioned state directory](rust/crates/truapi-host-cli/README.md#state-directory).

`scripts/battery.sh` drives that CLI from source over every code-generated
example and writes both committed compatibility reports:
`explorer/diagnosis-reports/spa/signing-host-cli.md` from a direct signing-host
run, and `spa/pairing-host-cli.md` from a pairing host that the script pairs with a
signing host it starts itself.

```bash
scripts/battery.sh                  # both phases
scripts/battery.sh --signing-host   # direct phase only
scripts/battery.sh --pairing-host   # paired phase only
make e2e-signing-cli                # same direct signing-host phase
make e2e-pairing-cli                # same paired pairing-host phase
make e2e-chat-cli                   # chat content screening against a chat signing-host
make e2e-pocket-cli                 # Pocket protocol check against a Pocket signing-host
```

The Pocket phase runs its product as a Worker execution, the only execution
Pocket is served to. It seeds the CLI's in-memory Pocket host from
`TRUAPI_POCKET_CARDS` (`loyalty,humanity:privileged`), which is what makes one
card removable and one privileged, and records every removal it is asked for in
the transcript named by `TRUAPI_POCKET_LOG`. The cases read that transcript, so
a pass means the host and the product agree on what happened rather than
resting on the product's word. The report lands at
`explorer/diagnosis-reports/pocket/signing-host-cli.md` and feeds the explorer's
Pocket compatibility matrix.

To run the playground locally in a plain browser tab, against a signing host on
your own machine:

```bash
cd playground
truapi-host dev -- yarn dev
```

`truapi-host dev` starts a signing host on `127.0.0.1:9955`, waits for its
signer, then runs the wrapped command with the host already live. The product
reaches it through a development-only `<script>` tag:

```jsx
{process.env.NODE_ENV === "development" && (
  <script src="http://127.0.0.1:9955/bootstrap.js" />
)}
```

The host serves that script itself, with no imports or environment variables
needed. It installs the shared client and browser container before product code
runs. Keep the tag before application scripts, without `async` or `defer`.
SDK calls and permission checks share one connection. Updated SDKs reuse the
injected client across reconnects; older SDKs can still start through the
MessagePort adapter but require a page reload after a disconnect.
A visible page retries a failed reconnect after 250 ms, 1 s and 4 s; after that, the next API
call or return to a visible page tries again.
On iOS the host rebinds its localhost listener on the same port each time the app
returns to the foreground, since the system reclaims a suspended app's listening socket.
On every platform the bridge also rebinds the port itself when its listening socket is destroyed,
pausing between failed attempts instead of retrying in a tight loop; other accept errors keep the port.
When WebKit loses its networking process, every MessagePort a page already holds stops
delivering; the container detects this after a disconnect and reloads the page.
On Android, a product whose WebView renderer dies reloads in a fresh WebView with the same
bootstrap, and a running worker whose renderer dies boots again.
The container routes fetch, XHR and WebSocket permission checks to Rust.
WebRTC and camera/microphone access use the same live permission checks.
`/script` shares these wrappers for the APIs available in Bun. CLI permission
checks support development testing; product code can deliberately bypass them.
Native hosts retain their separate authorization protection. TCP frame
connections are accepted only from loopback peers, and browser WebSocket
origins must also name localhost or a loopback IP. WebSocket is not subject to
CORS, and confirmations here are auto-approved.

The CLI owns the wrapped command's process group on Unix. On shutdown it sends
SIGTERM to the group, waits up to five seconds, then sends SIGKILL if a
descendant still remains. This prevents a package-manager child from keeping a
development port open. A natural child exit keeps its status, while SIGINT or
SIGTERM cleanup exits with status 130.

A host that should outlive the dev server, or one whose confirmations you want
to approve by hand in its terminal UI, is the existing command with your dev
server started separately:

```bash
truapi-host signing-host --frame-listen 127.0.0.1:9955 --product-id localhost:3000
```

To run the playground inside a real host instead, start it with `yarn dev` and
open `https://dot.li/localhost:3000` in the Polkadot Desktop Host. See
[`playground/README.md`](playground/README.md) for deployment.

### Bundle size

The `Bundle size` CI job builds the packages and runs
[`.github/actions/bundle-size`](.github/actions/bundle-size/action.yml) on them.
The action's `assets` input lists the groups it measures (raw, gzip and
brotli): the wasm-pack output of the Rust crates (the `@parity/truapi-host` web
bundle and `@parity/truapi-provider`) and the compiled TypeScript of
`@parity/truapi` and `@parity/truapi-host`, without
the test host behind `@parity/truapi-host/testing`. A push to `main` stores the
measurement as the baseline, and every pull request gets one comment comparing
with it. A size change never fails the job. To see the same report locally,
build the assets and pass the job's `assets` list to the script:

```bash
make wasm
npm run build --prefix js/packages/truapi-host
node scripts/bundle-size.mjs --assets "<the job's list>" [--baseline <snapshot.json>]
```

### Refreshing a vendored host tree

The host trees under `hosts/` are snapshots of the repositories they were
imported from, and those repositories keep moving. `hosts/imports.json` records
where each tree came from and at which revision.

```bash
scripts/refresh-host-import.sh status ios     # how far behind, and what differs
scripts/refresh-host-import.sh refresh ios    # take the new tree, re-apply adaptations
```

Changes move one way, from the source into this tree. A change made here is not
sent back: the source is upstream of this repository, not a peer.

`refresh` replaces the tree with the source's, re-applies this repository's
adaptations on top as a three-way patch, then compares every path against the
source by blob hash in both directions. A difference no adaptation accounts for
is upstream work that was dropped; an adaptation that left no difference either
did not apply or has been adopted upstream.

A clean apply is staged for review. A conflicted one is left unmerged, so git
refuses to commit it until someone decides which side is right.

Drift is picked up on a schedule. `.github/workflows/backport-host.yml` opens a
pull request carrying a single `BACKPORT-<host>.md`, which names the range, the
pull requests in it, and what has to be done to finish the work. Completing that
pull request means running the command above and deleting the file.

### Working on the iOS host

`hosts/ios/` is the iOS app, and it resolves the core from this tree rather than
from a published version. The core's bindings, xcframework and FFI headers are
gitignored build outputs, so a fresh clone cannot load the app's package graph
until they exist. Generate them once:

```bash
make ios-bootstrap
```

Then open `hosts/ios/polkadot-app.xcodeproj`. Rerun it after changing anything
the bindings are generated from, which is the `truapi` or `truapi-provider`
crates. `SIM_ONLY=1` halves it by skipping
the device slice, which is enough for Simulator but not for an archive.

Because the app builds against the core in this tree, a core change that breaks
it fails here rather than at the next version bump. Every push to main runs the
jobs below, and so does a pull request touching the app, or the crates its
bindings come from, once it is labelled `ios-simulator-build`. They are macOS
jobs, so a pull request without the label runs none of them. CI's
`iOS package (Swift + WebKit)` job still compiles the TrUAPIHost package against
the core on a pull request touching `ios/` or the core crates, but the app itself
is compiled before merge only with the label. The first time a pull request
touches the iOS or Android app, `build-label-hint.yml` comments with the labels
that build it: `ios-simulator-build`, `ios-device-build` and `android-device-build`.

- `build`, a DevCI compile, failing on any build warning the committed baseline
  does not already have
- `test`, the unit test suite
- `preview`, an installable simulator `.app` attached to the run, stamped with
  the commit it came from in `TrUAPICommit`

Pull requests that build the app carry a comment linking the build for its head
commit, updated in place as the branch moves. To run one:

```bash
gh run download <run-id> --name simulator-preview-<short-sha>
unzip polkadot-app-*.app.zip
xcrun simctl install booted polkadot-app.app
xcrun simctl launch booted io.parity.polkadotapp.develop
```

It is an arm64 simulator slice, so it needs an Apple Silicon Mac and cannot be
installed on a device.

### A build that installs on a phone

Label a pull request `ios-device-build` and `ios-device-preview.yml` produces a
signed ad-hoc archive attached to the run, named for the commit it was built
from. It uploads nowhere: not to Apple, not to any distribution service.

The commit is stamped into the app before the build rather than after, because
editing a signed bundle invalidates its signature and the archive would then
refuse to install. The job checks the stamp survived and that every
seal in the bundle, including the nested extension, still validates.

Installing it needs the device's UDID in the ad-hoc provisioning profile, which
is Apple bookkeeping rather than CI. The workflows that register a device and
regenerate the profile are held until the cutover, tracked on #764; the device
preview itself is tracked on #681.

### An Android build that installs on a phone

Label a pull request `android-device-build` and `android-device-preview.yml`
attaches an installable APK to the run. Android needs no provisioning, so it
installs on any phone rather than only on registered devices, and it is signed
with the shared develop key so a new build replaces the last one rather than
asking to be uninstalled first.

It builds the flavour that ships. The other one substitutes stubs for Google
auth, Firebase auth, push and backup, so a preview built from it cannot sign in.
A pull request opened from a fork cannot reach the configuration and signing key
this needs, and is told so rather than handed a build that misleads. Push the
branch to this repository to get one.

### Android builds that reach testers

Two workflows deliver through Firebase App Distribution, which reaches a named
tester group. The nightly also attaches its APKs to a public GitHub prerelease,
so anything built into a nightly is public.

`android-nightly.yml` runs daily at 22:00 UTC, two hours after the iOS
nightly starts, so the two never overlap. It publishes both flavours on a
GitHub prerelease, `app-gp-nightly.apk` and `app-vanilla-nightly.apk` (without
Google Play services), and sends the gp one to Firebase App Distribution.
Every run on `main` also refreshes the `nightly-android` release, so the latest
build always downloads from the same two links:
`https://github.com/paritytech/trinity-user-agents/releases/download/nightly-android/app-gp-nightly.apk`
and the same path ending in `app-vanilla-nightly.apk`.
Each announcement links both APKs and lists the pull
requests the build carries, with breaking changes, the titles carrying `!`,
listed first and marked `Breaking:`. Both nightlies skip a scheduled night
when `main` has not moved past what their last successful run built. `android-debug-distribution.yml` runs
on every push to `main`, and answers what `main` does right now. It builds the
pushed commit, the one that landed. A push rather than the pull request's merge
event, because the Firebase identity is bound to `main`, and a pull request
event's token never matches that binding.

Both authenticate by federation. The run proves its identity with its OIDC
token and receives a short lived credential, so no long lived key for that
project is stored here. Both check the delivery target before building, since
an hour is an expensive place to discover a renamed tester group. That check
cannot prove the upload will be permitted: listing groups is available to a
role that cannot write, and only an upload proves an upload.

Both verify the certificate that signed the APK rather than only that one did,
because a rotated keystore otherwise produces a build every tester's device
rejects on install, behind a green run.

Release distribution stays in the app repository. It signs with a release
keystore this repository does not hold.

#### What they read

Secrets: `GOOGLE_SERVICES_JSON_BASE64`, `CI_GITHUB_KEYSTORE_KEY_FILE`,
`CI_KEYSTORE_PASS`, `CI_KEYSTORE_KEY_ALIAS`, `CI_KEYSTORE_KEY_PASS`,
`FIRESTORE_DATABASE_ID`, `GOOGLE_OAUTH_ID`, `GOOGLE_PROJECT_ID`,
`NIGHTLY_FUNDING_MNEMONIC`, `SENTRY_DSN`, `ANDROID_FIREBASE_NIGHTLY_APP_ID`,
`ANDROID_FIREBASE_APP_ID`, `GCP_WORKLOAD_IDENTITY_PROVIDER`,
`GCP_SERVICE_ACCOUNT`.

Variables: `APPLICATION_ID`, `APPLICATION_NAME`, `CURRENCY_SYMBOL`,
`LOG_COLLECTION_EMAIL`, `PRIVACY_POLICY_URL`, `TERMS_OF_USE_URL`,
`SENTRY_ORG`, `SENTRY_PROJECT`, `GAME_RESULTS_FALLBACK_URL`,
`REFERRAL_WEB_HOST`, `CONTACT_EMAIL`, `ANDROID_FIREBASE_GROUP`,
`ANDROID_FIREBASE_DEBUG_GROUP`.

`GOOGLE_PROJECT_ID` carries an `L` suffix. It is interpolated into a Java
`long` literal, and a twelve digit project number overflows an `int` without
one. Everything the app needs at runtime beyond these comes from Firebase
Remote Config, keyed on an `environment` signal the build sets.

### Instrumented tests

`android-instrumented-tests.yml` boots an emulator and runs the app module's
connected tests. It starts from the `android-instrumented-tests` label rather
than from every commit, because the runner is macOS and a cold emulator costs
minutes before the first assertion. `workflow_dispatch` runs it without a pull
request to carry the label.

It is not part of the required set, so a red run reports rather than blocks.

### Credentials, checked before a release needs them

Certificates, provisioning profiles and store keys expire, and a release is the
most expensive place to discover it. `validate-signing-credentials.yml` runs on
weekday mornings, authenticates each platform, and proves the credential is
live without building or publishing: Android reuses the delivery check the
nightly runs before it builds, and iOS reads one page of applications through
the store key then fetches the signing material read only.

Each job removes what it materialised, and the last verdict is carried into the
pull request summary, so it is visible before someone starts a release rather
than after. A workflow that has never run reports as never run, not as healthy.

### Building the standalone iOS host app

The targets below build `polkadot-app-ios-v2`, a separate checkout set by
`IOS_HOST`, not `hosts/ios`. To build the playground in Simulator against it:

```bash
make ios-run
```

The target regenerates the UniFFI Swift bindings, builds the matching Rust
simulator library, and builds the sibling `polkadot-app-ios-v2` checkout with the Nightly feature
flags and release Firebase app used by the Nightly TestFlight build. Native
Chat and the Paseo chain catalog come from the same Nightly Remote Config as
TestFlight. The executable keeps the development bundle and app-group identity
so Simulator can reuse its already registered wallet; keychain data cannot be
transferred to the production bundle. The simulator also adds the
`IOS_PASEO_E2E` conveniences needed to start on the real `browse.dot` Browse tab
and activate the embedded signing host. Embedded product host sessions use the
same Paseo People and Bulletin chains selected by the Nightly app
configuration. The launcher starts the playground at `http://localhost:3100`
when needed and uses that local source only after Browse opens
`truapi-playground.dot`. It refuses to launch if the local URL belongs to a
different app. Override the product, URL, or simulator with `IOS_PRODUCT_HOST`,
`IOS_PRODUCT_URL`, or `TRUAPI_IOS_E2E_DEVICE`. The simulator launch reuses the
wallet and registered username already stored by the iOS app. Opening a product
activates the embedded `truapi-host` signing-host session from that wallet; it
does not provision or pair a signer-bot user.

To exercise the shared-core Chat path with the first-party TrUAPI Playground
worker, build and serve the local product, install its worker into the
simulator app's product storage, and open its native Chat application. The
worker drives all five Chat methods and both Renderer methods, so a host
without bot registration reports that row red:

```bash
make ios-chat-run
```

The launcher verifies the Chat connection and runs a correlated Chat-only
diagnosis. The worker proves create-room idempotency, observes the new room on
the live list subscription, posts text and custom messages, receives
`!diagnose` through `chat_action_subscribe`, and serves live renderer trees.
The launcher also verifies that a renderer update reaches native code and that
the final Markdown report reaches CoreData. It writes the host-labelled report
to `playground/test-results/ios-chat/diagnosis-report.md`. The product builds
against the workspace-linked `@parity/truapi`. Override the product source,
identity, SPA URL, room, input, or report path with
`IOS_CHAT_PRODUCT_DIR`, `IOS_CHAT_PRODUCT_HOST`, `IOS_CHAT_PRODUCT_URL`,
`TRUAPI_IOS_E2E_CHAT_ROOM_ID`, `TRUAPI_IOS_E2E_CHAT_MESSAGE`, or
`TRUAPI_IOS_E2E_CHAT_REPORT`.

The same harness can run the legacy Product SDK worker from the sibling
`host-playground` checkout. This target builds the current TrUAPI client, links
it over Host Playground's transitive `@parity/truapi`, then builds and runs
that product:

```bash
make ios-chat-host-playground-run
```

Run both integrations with one iOS build using `make ios-chat-all`.

## Regenerate the TypeScript client

When the Rust trait surface changes:

```bash
make codegen      # regenerate the TS client and refresh the playground snapshot
make playground   # rebuild the playground against the refreshed snapshot
```

This repopulates the ignored generated TS under `js/packages/truapi/`, including the playground metadata.
`make dev` and `make e2e-dotli` run this generation step unconditionally before starting their local stacks.
The full `make e2e-dotli` diagnosis builds and launches the local
`truapi-host signing-host` CLI to answer dotli's pairing QR and auto-approve
remote signing requests. It does not require the external signer-bot service.
When `HOST_CLI_SIGNER_MNEMONIC` is absent, the CLI manages a reusable isolated
test identity under `.e2e-dotli/`. Set `E2E_DOTLI_SIGNING_HOST_BASE_PATH` to
use a different state directory while debugging.

## Protocol versions

- **v0.1**: initial protocol version.
- **v0.2**: See [`docs/design/releases/v0.2.md`](docs/design/releases/v0.2.md) for the rationale behind each change.
- **v0.3**: current protocol version.

## Deploy

Pushes to `main` build and deploy:

- The playground to the dotNS label [`truapi-playground`](https://truapi-playground.paseo.li/), live as `truapi-playground.paseo`, via [`.github/workflows/deploy-playground.yml`](.github/workflows/deploy-playground.yml).
- The Rust API docs to [https://paritytech.github.io/trinity-user-agents](https://paritytech.github.io/trinity-user-agents) via [`.github/workflows/deploy-docs.yml`](.github/workflows/deploy-docs.yml).

## Release

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for how to ship
`@parity/truapi` and `@parity/truapi-host` to npm, and the iOS host and
Android host artifacts alongside them. A release also opens a bump issue on each
repository listed in [`.github/consumers.json`](.github/consumers.json) that pins
one of the published packages.

CI requires changesets for published build inputs and rejects version bumps
with unconsumed changesets, including in the merge queue. The daily
`Registry drift` workflow reports manifest versions missing from npm; see the
release guide for opt-outs and recovery.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for issue reports, feature proposals, and the RFC process.

## License

[MIT](./LICENSE)
