# TrUAPI Android host adapter

*Kotlin wrapper around the TrUAPI Rust core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

Distribution: a Maven AAR published to GitHub Packages by the `release-android` workflow. Each release bundles, built from the same source tree: `libtruapi.so` for arm64-v8a, armeabi-v7a and x86_64, the UniFFI Kotlin bindings (`uniffi.truapi.*`), the Kotlin host adapter (`io.parity.truapi.*`), and the browser container asset. Consumers need no Rust toolchain or NDK.

## Consume

Add the GitHub Packages repository and the artifact to your app's Gradle build (GitHub Packages requires authentication even for public repos — any GitHub account token with `read:packages` works):

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri("https://maven.pkg.github.com/paritytech/trinity-user-agents")
            credentials {
                username = providers.gradleProperty("gpr.user").orNull ?: System.getenv("GITHUB_ACTOR")
                password = providers.gradleProperty("gpr.key").orNull ?: System.getenv("GITHUB_TOKEN")
            }
        }
    }
}
```

```kotlin
// app/build.gradle.kts
dependencies {
    implementation("io.parity:truapi-host-android:0.1.0")
}
```

The package is public, so any authenticated GitHub identity can read it. In GitHub Actions that means the built-in `GITHUB_TOKEN` with a `permissions: packages: read` block, no secret to create or rotate. Locally it means a personal access token with `read:packages`, set once as `gpr.user` / `gpr.key` in `~/.gradle/gradle.properties`. A token without that scope fails with 401 even though the package is public, which is how GitHub Packages treats Maven.

The consuming app must declare `android.permission.INTERNET` — the localhost WebSocket bridge binds a `127.0.0.1` TCP socket, which requires it even for loopback.

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

### Compatibility

- **minSdk**: 29 (Android 10). Aligns with the polkadot-app-android-v2 floor.
- **AGP**: built with 8.5.2; AGP 8.5+ consumers are fine. AAR is forward-compatible with newer AGPs.
- **Kotlin**: built with 1.9.24. Newer Kotlin compilers (2.x) read 1.9 metadata fine.
- **Transitive dependencies**: `net.java.dev.jna:jna:5.14.0` (UniFFI's runtime, ~1.5MB for consumers that don't already use it), `org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0` and `org.jetbrains.kotlin:kotlin-stdlib:1.9.24`.
- **Size**: the AAR is ~20MB, one `libtruapi.so` per ABI. An app bundle ships only the ABI the device needs, so the installed cost is ~9MB on arm64.

## Public surface

- `WalletSecretProvider` reads the selected unlocked root independently of host storage.
- `HostSecretStorage` protects typed host secret records and confirms durable writes/removals.

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Splits device permissions, remote permissions, navigation, push, feature support, action and permission confirmations, and typed protected host storage.
- `LocalhostBridgeBootstrap` - supplies the private WebSocket endpoint to the container.
- `ContainerScriptBundle` - loads the bundled browser container for installation at document start.
- `TrUAPIHostRuntime` - process-owned runtime whose product executions share one authentication session. Open a connection per executable with `openProductExecution`, which returns a `TrUAPIProductExecution` holding its own token on the runtime's shared WS bridge, permission authorization, theme/preimage/chain notifications, and the Chat controls below.
- `ChatHostBridge` - native Chat storage and UI supplied by `WorkerEngineHost.chatBridge`. A missing bridge reports unsupported.
- `PocketHostBridge` - native card collection supplied by `WorkerEngineHost.pocketBridge`. Use the supervisor-issued execution to publish card changes. `removeCard` suspends and checks removal eligibility atomically; a missing bridge reports unsupported.

## Chat and Pocket workers

Implement `WorkerEngineHost` and install it with `setWorkerEngineHost` before activating the wallet. Rust owns the worker roster, references, durable operations, bundle updates and restart policy. The engine adapter fetches complete immutable bundles, starts the supplied execution and awaits native teardown in `stopWorker`. Native apps open visible App or Widget executions directly; Worker executions come only from the core supervisor.

`chatBridge(productId)` supplies the native Chat room/message adapter, and `pocketBridge(productId)` supplies the native card collection adapter. Both belong to the engine factory. Store actual chat/card content in their existing native owners; Rust persists worker reasons and operations. Report installed Chat/Card changes through `notifyWorkerIntent`.

Use the supplied execution to publish Chat actions, subscribe to rendered nodes, dispatch renderer actions and notify room/card changes. An open render stream holds a transient core worker reference. Validate user actions against the current rendered tree. `sessionChatIdentityKey` is sensitive session material and must not be logged or separately persisted.

After native UI approval, `establishPairing` provisions wallet and peer statement allowances, verifies renewal, sends notices and persists the core paired-host roster. Observe `devicePaired` or `runtimeRecordsChanged`, query `pairedHosts`, and run one `resumePairing` task for each peer. Transient subscription errors retain the pairing. Cancel the task and call `removePairedHost` for peer disconnection or user revocation; Rust removes its records and renewal target. Native applications do not keep a parallel pairing or replay journal.

## Architecture

```text
product app in WebView
  Public calls + private permission checks
           |
  One injected SDK client and transport
           |
           v   ws://127.0.0.1:<port>/?t=<token>
TrUAPIProductExecution.startWsBridge()
  → libtruapi.so (tokio WS server)
  → Rust dispatcher
```

The container consumes the bootstrap endpoint before product scripts run and creates one SDK connection for public calls and private permission checks. The updated SDK adopts the injected `window.__HOST_API_CLIENT__` client and keeps it across socket replacement. Interrupted calls and subscriptions fail without replay. Older SDKs use a small `window.__HOST_API_PORT__` adapter and require a page reload after a disconnect.

Android intercepts HTTP requests natively, including scripts and images, and calls `execution.authorizeRemotePermission` on the same Rust permission service used by the SDK. This shares grants and consumes “Allow once” only once. Approval covers the initial URL and any redirects it follows; redirect destinations are intentionally not checked separately. The container uses `nativeHttp: true` to avoid checking fetch and XHR again in JavaScript. Both product pages and hidden worker WebViews use this path.

The main frame retains `window.__HOST_WEBVIEW_MARK__` for deployed products that use it to select native navigation or storage. New products should use the SDK's container detection.

The Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, OS notification registration/cancellation, `devicePermission`, `remotePermission`, `authStateChanged`, protected host secrets, chain JSON-RPC, `confirmUserAction`, `confirmPermission`, preimage lookup, theme, `featureSupported`) reach the embedder through `HostBridge`. Bulletin preimage build/sign/submit now happens inside the core, so the host only serves `lookupPreimage`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does the bridge:

- `devicePermission(product, request)` - OS-scoped grants (camera, mic, location, push). `request` is a typed `HostDevicePermissionRequest`.
- `remotePermission(product, request)` - per-product capabilities. `request` is a typed `RemotePermission`.

`product` is the requesting execution's `ProductExecutionConfig`.

Both return `PermissionDecision` (`ALLOW_ONCE`, `ALLOW_ALWAYS`, or `DENY`). Preserve the choice so the core can consume one-use grants without persisting them. OS refusal after app consent should throw rather than record a product denial. The same typed values drive the `TrUAPIProductExecution` permission admin API (`permissionAuthorizationStatus`, `setPermissionAuthorizationStatus`), which reads and updates the persisted decisions without prompting.

The browser container checks product consent before opening WebSockets or requesting camera and microphone access. After installing it, the WebChromeClient media callback should check only the Android OS permission, so it does not consume product consent twice.

To disable WebRTC, call `execution.setPermissionAuthorizationStatus` with a remote `WebRtc` request and `DENIED` before loading each product. This overrides saved grants and trusted-product auto-grants, which otherwise skip `remotePermission` callbacks.

The vendored Android host retains its native HTTP checks for fetch, XHR and subresources. Its installer adds `nativeHttp: true` to the private bootstrap configuration in every frame before the container runs, disabling duplicate JavaScript HTTP checks. Embedders without native HTTP enforcement must leave this flag unset.

Identity and account access reviews use `confirmPermission(review)`, which also returns `PermissionDecision`. Override it to preserve Allow once. Its compatibility default maps `confirmUserAction`'s Boolean approval to `ALLOW_ALWAYS`; signing and other single-action reviews continue to use that Boolean callback.

## Statement-store allowance renewal

Rust tracks and persists allowance renewal targets when pairing or allocating product resources. `establishPairing` verifies both wallet and peer allocation before completing; native apps do not allocate these accounts through a parallel wallet implementation.

After wallet activation, `startStatementAllowanceRenewal` runs the core renewal loop. It pauses without an active wallet. Hosts using an OS background job can instead call `renewStatementAllowances` off the main thread after restoring an unlocked wallet. A locked wallet is not ready for renewal and must not trigger legacy fallback.

A renewal report contains one status per target. `Registered` and `AlreadyAllocated` confirm allocation; `Failed` requires retry, and `SkippedExhausted` requires reporting insufficient capacity rather than repeatedly retrying in the same period. Use `statementRenewalTargets`, `allowanceRecords` and `statementSlots` to inspect the active owner's durable records. Rust keeps detailed slot priority and period authoritative.

## Example

The embedding app supplies protected host storage and a separate wallet-only root provider. `HostBridge.secretStorage` receives typed Rust keys; use `secretCoreStorageKeyIdentifier` for opaque key names and route `DeviceEncryptionKey` to the existing shared chat identity. Only a missing record returns `nil` or `null`. Reads, committed writes and removals must report errors without plaintext fallback.

Use the app's existing Keychain policy on iOS or committed Keystore-protected preferences on Android. Product/core payloads and runtime journals use Rust-owned SQLite in the active wallet's directory. The installation encryption key stays in protected storage across ordinary logout.

```kotlin
val runtime = TrUAPIHostRuntime(
    bridge,
    walletSecretProvider,
    HostRuntimeConfig(
        hostName = "My Host",
        peopleChainGenesisHash = peopleGenesis,
        bulletinChainGenesisHash = bulletinGenesis,
        assetHubChainGenesisHash = assetHubGenesis,
        networkSuffix = "dot",
        databaseDirectory = databaseBaseDirectory,
    ),
)
check(runtime.setWorkerEngineHost(workerEngines))
runtime.activateWallet(selectedWalletId, null)
val execution = runtime.openProductExecution(
    productBridge,
    ProductExecutionConfig("my-product.dot", ProductExecutionKind.APP),
)
val endpoint = execution.startWsBridge()
val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
```

Install the bootstrap and shared container at document start before loading the product. Wallet activation and teardown are asynchronous because they await protected storage, OS notification work and engine disposal. On wallet switch, await `shutdown`, drop the old runtime, construct another with the same base directory and activate the new identifier. `lockWallet` clears wallet memory while retaining eligible delegated grants; reactivate the matching owner before using them.

`WorkerEngineHost` fetches immutable bundle files and creates or stops native engines for executions supplied by Rust. Install it before activation so saved workers can resume. Send Chat/Card installation changes through `notifyWorkerIntent`; Rust persists their reasons, operations, update state and restart policy. Await engine teardown in `stopWorker`, and report asynchronous startup failures through `notifyWorkerFailed` with the exact execution.

Notifications receive Rust-assigned IDs through `scheduleNotification`, `cancelScheduledNotification` and `isScheduledNotificationPending`. Namespace OS identifiers with the runtime's captured wallet identifier and product ID. The core retains failed OS work for reconciliation. `runtimeRecordsChanged` invalidates settings/catalog snapshots; re-query the runtime instead of keeping a second permission or allowance journal.

## The cdylib

The released AAR bundles `libtruapi.so` for all three ABIs under its `jni/` directory; JNA loads it from there without any consumer setup.

When iterating on the core from a source checkout instead of the published artifact, cross-compile into this module's `jniLibs` with:

```bash
make android-jni    # needs cargo-ndk, the NDK, and the three Android rust targets
```

or point the `mozilla-rust-android-gradle` plugin at `rust/crates/truapi` from the host app's own build (polkadot-app-android-v2 does this while it still builds from a checkout).

## Maintainers: cutting a release

Include `@parity/android-host <version>` in the `release:` PR title, the same flow the npm packages and the iOS host use. On merge, `release.yml` calls `release-android.yml` for the release commit, which cross-compiles the cdylib for all three ABIs, regenerates the Kotlin bindings via the `codegen` cargo profile, and publishes `io.parity:truapi-host-android:<version>` to GitHub Packages.

A manual `release-android` run with a version input reaches the same workflow, as an escape hatch. There is deliberately no tag trigger: a tag push cannot use `release.yml`'s gate on green CI, so it would be an unverified path to the registry.

The version lives only in the release subject. Nothing in the tree records it, so there is no committed version to keep in sync.

For local development, publish into `~/.m2`:

```bash
make android-jni            # optional: bundle the cdylibs into the local AAR
make android-publish-local
```

The artifact lands under `~/.m2/repository/io/parity/truapi-host-android/0.0.0-local/`; consumers pointing at `mavenLocal()` resolve it as `io.parity:truapi-host-android:0.0.0-local`.

## Regenerating the UniFFI bindings

The ignored Kotlin bindings under `src/main/kotlin/generated/uniffi/` are produced from the workspace `uniffi-bindgen-cli`. Regenerate them before building or publishing the Android host package:

```bash
make uniffi-kotlin
```

`make uniffi-kotlin` builds the host cdylib with the `codegen` profile and runs
the generator. The `codegen` profile is required because uniffi-bindgen scans
the cdylib's exported metadata symbols, which the `release` profile strips — a
plain `--release` build produces a stripped library and no bindings. (`make
uniffi` regenerates the Swift bindings; use `make uniffi-kotlin` for Android.)

No CI job compiles this package. After changing `TrUAPIHost.kt` or the UniFFI
surface it wraps, run `make android-check` locally — it regenerates the Kotlin
bindings and compiles the module against them.
