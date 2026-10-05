# TrUAPI iOS host adapter

_Thin Swift shell over the Rust TrUAPI core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge._

The package lives in the truapi repo next to the Rust core it wraps. `Package.swift` sits at the **repo root** (SPM requires that for git-URL dependencies), with all target paths pointing into `ios/truapi-host/`; the build scripts regenerate those target paths from this repo's workspace, because none of them are committed.

## What this package is for

The `TrUAPIHost` SPM package an iOS host app imports directly. It carries:

- [`Sources/TrUAPIHost/TrUAPIHost.swift`](Sources/TrUAPIHost/TrUAPIHost.swift) — the hand-written shell: `TrUAPIHostRuntime`, `TrUAPIProductExecution`, their configuration and bridge protocols, and `LocalhostBridgeBootstrap`.
- [`Sources/TrUAPIHost/ProductScripts.swift`](Sources/TrUAPIHost/ProductScripts.swift) registers the shared container in every frame. Fetch, XHR and remote WebSockets ask Rust directly through the existing private bridge.
- the Rust core as a binary target — a GitHub release asset by default (`publishedBinaryURL` in the root `Package.swift`), or the locally built `Binaries/truapi_server.xcframework` when `useLocalBinary` is flipped to true.
- `Sources/TrUAPIHost/truapi.swift` and `Sources/truapiFFI/include/` — the generated UniFFI bindings.
- [`js/container/`](../../js/container) — the TS lockdown container; built into `Sources/TrUAPIHost/Resources/truapi-container.js` and exposed via `ContainerScriptBundle.load()`.
- `Tests/` contains WS-bridge and WebKit network tests that boot the real Rust core.
- `TestHost/` provides the UIKit app and XcodeGen project for simulator tests.

The generated bindings, the container bundle and the xcframework are all **gitignored** build outputs, so a fresh checkout has no Swift sources for the package's targets. Run `rebuild.sh` before opening it. The xcframework is additionally distributed as a GitHub release asset. Two scripts split the lifecycle:

```bash
./scripts/rebuild.sh            # regenerate xcframework + bindings + container
                                # from this repo (make xcframework at the root)
./scripts/publish.sh <version>  # zip the built xcframework, upload it to the
                                # "@parity/ios-host <version>" GitHub release,
                                # and point the root Package.swift at it
                                # (URL + checksum)
./scripts/tag-release.sh <version>
                                # commit the generated sources plus that
                                # manifest and tag it <version>: the tag a
                                # SwiftPM consumer resolves
```

A consumer pins the plain semver tag, not the `@parity/ios-host@<version>` one,
which SwiftPM cannot see:

```swift
.package(url: "https://github.com/paritytech/trinity-user-agents", exact: "0.12.0")
```

`release-ios.yml` runs all three in order and clones and compiles the tag
before pushing it. Run them by hand only as a fallback.

When only the bindings need refreshing — a Rust surface change with no container
or xcframework impact — skip the full rebuild, which needs Xcode and the iOS
targets:

```bash
# from the repo root
make uniffi && ./ios/truapi-host/scripts/sync-bindings.sh
```

CI's `iOS bindings (uniffi)` job runs the same two commands. With nothing
committed to diff against, what it gates is that bindgen still produces a
binding for every UniFFI-exposed type. It runs on Linux, so it never compiles
Swift.

The hand-written conformers in `TrUAPIHost.swift` and `Tests/` are covered by
the `iOS package (Swift + WebKit)` job instead, which builds a simulator-only
debug XCFramework from the pull request source, compiles the package tests,
and runs the network permission suite in WKWebView. It is path-filtered to pull requests touching `ios/`, `Package.swift`, the
`Makefile`, `js/container/`, or any of the crates the bindings are generated
from (`truapi`, `truapi-provider`); the
filter has to name them explicitly, since a protocol change no longer shows up
as an `ios/` diff.
The Android host job compiles `TrUAPIHost.kt` against generated bindings;
the separate iOS CI workflow builds and tests the embedding app.

Run `rebuild.sh` after changing anything host-visible — the `NativeTrUApiHostRuntime` or `NativeProductExecution` methods, `HostCallbacks`, the native mirror types in `rust/crates/truapi/src/native*`, or `js/container/src` — to refresh your local build outputs. Nothing to commit: CI regenerates them. To publish from a release PR, add `@parity/ios-host <version>` to its `release:` title. After the release commit passes CI, the release workflow rebuilds and simulator-tests the XCFramework on macOS, uploads it, cuts the `<version>` tag, and opens the `Package.swift` follow-up pull request only after the asset is live. `publish.sh` remains available for an ad hoc manual release.

For local iteration without publishing, set `TRUAPI_USE_LOCAL_BINARY=1` so the root `Package.swift` builds against `Binaries/` directly.

The embedding app implements `HostBridge` (defined in `TrUAPIHost.swift`): navigation, OS notifications, permissions, auth state, protected host secrets, chain JSON-RPC, confirmations, preimage, theme, feature support, and the served chain set. UI-decision callbacks are `async` and awaited by the Rust core. `HostCallbackAdapter` translates it to the UniFFI-generated `HostCallbacks` protocol; `TrUAPIHostRuntime` and each product execution retain their own adapter. Conform to `HostBridge` rather than to the generated protocol: its extension defaults the optional callbacks, so a newly added one does not break the build. The `secretStorage` provider protects bytes; Rust owns account, permission, product and notification records in SQLite.

## Integrating in an iOS app

Add the package as an SPM dependency and link the `TrUAPIHost` product into the app target:

```swift
.package(url: "https://github.com/paritytech/trinity-user-agents.git", exact: "0.12.0")
```

```swift
.product(name: "TrUAPIHost", package: "trinity-user-agents")
```

The release workflow publishes the asset under `@parity/ios-host@<version>`,
creates a bare `<version>` tag from a manifest containing its URL and checksum,
and builds that tag from a clean clone before pushing it. It also opens a
manifest PR to keep `main` current. SPM pins the resolved revision in the app's
`Package.resolved`; update it with File > Packages > Update in Xcode or
`xcodebuild -resolvePackageDependencies` after the tag is published.

`HostRuntimeConfig.networkSuffix` is required. Supply the bare TLD (`dot`,
`paseo`, or `testnet`) from the same network configuration used by onboarding
and the People/Bulletin genesis hashes. It must match the People chain's
`NetworkSuffix.NetworkSuffix`. Include this configuration update in the
embedding app's package upgrade.

`HostRuntimeConfig.assetHubChainGenesisHash` is required. Supply the Asset Hub
genesis hash from the same network configuration, as 32 bytes. Product manifests
are read from the dotNS contracts deployed there, so it is what makes a
`trustedProducts` grant resolvable: without a usable value no manifest resolves,
so every cross-product grant not already cached is refused, and the refusal is
indistinguishable from the other product having granted nothing. Pass 32 zero
bytes only to declare deliberately that this host has no Asset Hub. Include this
configuration update in the embedding app's package upgrade.

Run the package tests in their UIKit host on an iOS simulator (the xcframework has no macOS slice). The helper installs pinned XcodeGen under `.agent/tools`, generates the project, and selects an available simulator:

```bash
# from the repo root
./ios/truapi-host/scripts/test.sh
```

## Chat and Pocket workers

The embedding app owns worker engines and opens `ProductExecutionConfig(productId: ..., executionKind: .worker)` through `openProductExecution`, passing `chat:` and `pocket:` adapters for the surfaces it supports. Chat/card content remains in its existing native stores.

`acquireWorker` and `releaseWorker` report demand through `HostBridge.workerDemandChanged`. Hosts that keep operations alive implement `beginOperation` and `endOperation` using their existing operation service. The SDK defaults return process-local operation IDs and perform no persistence or native engine keep-alive. The imported iOS app retains these defaults on its enabled Rust path; durable worker-operation integration is outside this account-holder change. The core does not install bundles, construct engines or choose a restart policy.

Use the execution to publish Chat actions, subscribe to rendered nodes, dispatch renderer actions and notify room/card changes. An open render stream holds a transient core worker reference. Validate user actions against the current rendered tree. `sessionChatIdentityKey` is sensitive session material and must not be logged or separately persisted.

## Architecture

```text
                 Product app in WKWebView
                 /                     \
          Public calls          Network/media requests
                 |                      |
                 |              Private permission methods
                 \                     /
                   Shared SDK transport
                           |
                   Replaceable WebSocket
                           |  ws://127.0.0.1:<port>/?t=<token>
                   Shared Rust listener
                           |
                   Product execution
```

The bootstrap supplies the execution endpoint to the shared container, which consumes and removes `window.__truapi_localhost` before product scripts run. The container creates one SDK connection for public calls and private permission checks, then exposes its public client through `window.__HOST_API_CLIENT__`. The Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, OS notification registration/cancellation, `devicePermission`, `remotePermission`, `authStateChanged`, protected host secrets, chain JSON-RPC, confirmations, preimage, theme, `featureSupported`) reach the embedder through `HostCallbacks`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does `HostCallbacks`:

- `devicePermission(product:request:)` - product consent for device capabilities (camera, mic, location, push). `request` is a typed `HostDevicePermissionRequest`.
- `remotePermission(product:request:)` - per-product capabilities. `request` is a typed `RemotePermission`.

`product` is the requesting execution's `ProductExecutionConfig`.

Both return `PermissionDecision`: `.allowOnce`, `.allowAlways`, or `.deny`. Preserve the user’s choice; the core keeps one-use grants in memory and consumes them at the authorized operation. OS refusal after app consent should throw instead of returning `.deny`, which records a product denial. The same typed values drive the `TrUAPIProductExecution` permission admin API (`permissionAuthorizationStatus`, `setPermissionAuthorizationStatus`), which reads and updates the persisted decisions without prompting.

Identity and account access reviews use `confirmPermission(review:)`, which also returns `PermissionDecision`. Override it to preserve Allow once. Its compatibility default maps `confirmUserAction`'s Boolean approval to `.allowAlways`; signing and other single-action reviews continue to use that Boolean callback.

Fetch, XHR, WebSocket connections, notification scheduling, external navigation and existing remote-operation gates consume temporary grants. The shared container authorizes each `getUserMedia` call through `authorize_device_permission`, camera before microphone. Each approval consumes its one-use grant for that attempt: a later microphone denial or native capture failure does not restore the camera grant. The returned stream remains usable until stopped; another capture requires new authorization.

The container enforces product consent, while native media delegates resolve OS permission without consuming product consent again. An OS grant does not establish product consent. This boundary requires the container to run before product code in every frame, with its native methods and prototypes locked. SPA and Chat install it at document start. Authorization uses a private transport and response handler with captured browser primitives, so replacing public SDK replies, collection methods or Promise methods cannot approve a pending capture.

## SSO session handling

After wallet activation, call `establishPairing` with the original deeplink. Query `pairedHosts` and run one `resumePairing` task for each peer. Cancel its task and call `removePairedHost` when the user revokes a device. Rust owns SSO correlation, replay and wallet authorization; native applications do not maintain a parallel SSO journal or route remote requests through native product grants.

## Statement-store allowance renewal

Rust tracks and persists allowance renewal targets when pairing or allocating product resources. `establishPairing` verifies both wallet and peer allocation before completing; native apps do not allocate these accounts through a parallel wallet implementation.

After wallet activation, `startStatementAllowanceRenewal` runs the core renewal loop. It pauses without an active wallet. Hosts using an OS background job can instead call `renewStatementAllowances` off the main thread after restoring an unlocked wallet. A locked wallet is not ready for renewal and must not trigger legacy fallback.

A renewal report contains one status per target. `Registered` and `AlreadyAllocated` confirm allocation; `Failed` requires retry, and `SkippedExhausted` requires reporting insufficient capacity rather than repeatedly retrying in the same period. Use `statementRenewalTargets`, `allowanceRecords` and `statementSlots` to inspect the active owner's durable records. Rust keeps detailed slot priority and period authoritative.

## Example

The embedding app supplies protected host storage and a separate wallet-only root provider. `HostBridge.secretStorage` receives typed Rust keys; use `secretCoreStorageKeyIdentifier` for opaque key names and route `DeviceEncryptionKey` to the existing shared chat identity. Only a missing record returns `nil` or `null`. Reads, committed writes and removals must report errors without plaintext fallback.

Use the app's existing Keychain policy on iOS or committed Keystore-protected preferences on Android. Product/core payloads and runtime journals use Rust-owned SQLite in the active wallet's directory. The installation encryption key stays in protected storage across ordinary logout.

```swift
let runtime = try TrUAPIHostRuntime(
    bridge: bridge,
    walletSecrets: walletSecretProvider,
    runtimeConfig: HostRuntimeConfig(
        hostName: "My Host",
        peopleChainGenesisHash: peopleGenesis,
        bulletinChainGenesisHash: bulletinGenesis,
        assetHubChainGenesisHash: assetHubGenesis,
        networkSuffix: "dot",
        databaseDirectory: databaseBaseDirectory
    )
)
try await runtime.activateWallet(walletId: selectedWalletId, liteUsername: nil)
let execution = try runtime.openProductExecution(
    bridge: productBridge,
    configuration: ProductExecutionConfig(productId: "my-product.dot", executionKind: .app)
)
let endpoint = try execution.startWsBridge()
let bootstrap = LocalhostBridgeBootstrap.script(port: endpoint.port, token: endpoint.token)
```

Install the bootstrap and shared container at document start before loading the product. Wallet activation and teardown are asynchronous because they await protected storage and OS notification work. On wallet switch, await `shutdown`, drop the old runtime, construct another with the same base directory and activate the new identifier. `lockWallet` clears wallet memory while retaining eligible delegated grants; reactivate the matching owner before using them.

Notifications receive Rust-assigned IDs through `scheduleNotification`, `cancelScheduledNotification` and `isScheduledNotificationPending`. Namespace OS identifiers with the runtime's captured wallet identifier and product ID. The core retains failed OS work for reconciliation. `runtimeRecordsChanged` invalidates settings/catalog snapshots; re-query the runtime instead of keeping a second permission or allowance journal.

## Build outputs in detail

`./scripts/rebuild.sh` orchestrates everything; the underlying pieces, should you need one in isolation:

- **xcframework** — `make xcframework` (repo root) builds `truapi` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` and bundles `target/truapi_server.xcframework`; the script copies it into `Binaries/` and strips the per-slice `module.modulemap` (module resolution comes from the `systemLibrary` target; the slice copy collides with other xcframeworks in Xcode's flat include dir).
- **bindings** — `make uniffi` (run automatically by `make xcframework`) emits the Swift bindings into `target/uniffi-swift-out/` via the workspace `uniffi-bindgen-cli`; `scripts/sync-bindings.sh` copies them into `Sources/TrUAPIHost/truapi.swift` and `Sources/truapiFFI/include/`, renaming the emitted `truapiFFI.modulemap` to `module.modulemap` so the SwiftPM `systemLibrary` target picks it up. `rebuild.sh` calls it, and so does the `iOS package (Swift + WebKit)` job, which is what puts Swift sources into the package before `xcodebuild` runs.
- **container** — `npm run build` in `js/container/` (repo root) bundles `src/index.ts` into `Sources/TrUAPIHost/Resources/truapi-container.js`.
