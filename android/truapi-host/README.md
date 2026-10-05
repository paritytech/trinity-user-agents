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

The public surface lives in [`src/main/kotlin/io/parity/truapi/TrUAPIHost.kt`](src/main/kotlin/io/parity/truapi/TrUAPIHost.kt):

- `HostBridge` - callback bundle the embedding app implements. Splits device permissions, remote permissions, navigation, push, feature support, action and permission confirmations, and both storage backends.
- `HostStorage` - product-scoped read/write/clear interface the host backs with its own persistence. Its methods suspend, so a backend can await disk or keystore work.
- `HostCoreStorage` - core-owned read/write/clear interface for auth session, pairing identity, and persisted permission decisions (`key` is a SCALE-encoded `CoreStorageKey`). Its methods suspend, like `HostStorage`'s.
- `LocalhostBridgeBootstrap` - supplies the private WebSocket endpoint to the container.
- `ContainerScriptBundle` - loads the bundled browser container for installation at document start.
- `TrUAPIHostRuntime` - process-owned runtime whose product executions share one authentication session. Open a connection per executable with `openProductExecution`, which returns a `TrUAPIProductExecution` holding its own token on the runtime's shared WS bridge, permission authorization, theme/preimage/chain notifications, and the Chat controls below.
- `ChatHostBridge` - native Chat storage and UI, implemented by hosts that serve the Chat modality and passed to `openProductExecution`. Hosts without it pass nothing and Chat calls answer unsupported.
- `PocketHostBridge` - the host's Pocket card collection, implemented by hosts with a Pocket surface and passed as `pocket` to `openProductExecution`. The execution then offers `notifyPocketCardsChanged`. `removeCard` suspends, and decides and removes together, returning `NativePocketRemoval.Removed`, `Absent` or `Privileged`, so a card cannot be pinned between the check and the removal. Like Chat, Pocket is reachable only from a Worker execution with an active session, so without `activateLocalSession` every Pocket call answers `Denied`. Hosts without the bridge pass nothing and Pocket calls answer unsupported.

## Chat

A host serving the Chat modality implements `ChatHostBridge` (`createRoom`, `registerBot`, `postMessage`, `listRooms`) and opens the execution with `ProductExecutionKind.CHAT`:

```kotlin
import io.parity.truapi.*
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection

// Called from a shared dispatch pool, so the backing store must be
// thread-safe, and a slow call here stalls other product executions.
class MyChatBridge(private val store: ChatStore) : ChatHostBridge {
    override fun createRoom(roomId: String, name: String, icon: String) =
        if (store.putRoom(roomId, name, icon)) ChatRoomRegistrationStatus.NEW
        else ChatRoomRegistrationStatus.EXISTS

    override fun registerBot(botId: String, name: String, icon: String) =
        if (store.putBot(botId, name, icon)) ChatBotRegistrationStatus.NEW
        else ChatBotRegistrationStatus.EXISTS

    override fun postMessage(roomId: String, content: ChatMessageContent): String {
        if (content is ChatMessageContent.File) {
            // Declining a variant is how a host opts out of rendering one.
            throw HostRejection.Rejected("this host cannot render file cards")
        }
        return store.append(roomId, content)
    }

    override fun listRooms(): List<ChatRoom> = store.rooms()
}

val runtime = TrUAPIHostRuntime(
    bridge = bridge,
    runtimeConfig = HostRuntimeConfig(
        hostName = "My Chat Host",
        peopleChainGenesisHash = peopleChainGenesisHash,   // exactly 32 bytes
        bulletinChainGenesisHash = bulletinChainGenesisHash,
        assetHubChainGenesisHash = assetHubChainGenesisHash,
        networkSuffix = "dot",
    ),
)
// Chat needs an active session; without one every Chat call answers `Denied`.
runtime.activateLocalSession(secret)

val execution = runtime.openProductExecution(
    bridge = bridge,
    configuration = ProductExecutionConfig("chat.dot", ProductExecutionKind.CHAT),
    chat = MyChatBridge(store),
)
val endpoint = execution.startWsBridge()
val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
```

Install `bootstrap` and `ContainerScriptBundle` at document start before loading the product, as in the example below.

Chat requires an active session: `openProductExecution` succeeds without one,
but every Chat call then answers `Denied` until `activateLocalSession` or SSO
pairing completes.

The core bounds and screens the product-supplied fields it forwards — ids,
names, icons, message bodies, URLs, and the action and media counts. Ids and
names are also normalized; a message body is bounded and screened but passed
through byte-for-byte, and `ChatFile.size_bytes` is product-asserted and
unverified. Contextual output escaping is the host's job.

`postMessage` receives any `ChatMessageContent` variant; throw from it for one this host cannot render. The id it returns is the correlation key `ActionTrigger.messageId` carries back, so it must name that message for as long as the host stores it.

The runtime answers other devices pairing with it: `notifyPairingAllowanceAllocation` and `notifyPairingFailed` are the two notices a peer gets before the answer, `establishPairing` is the answer, `resumePairing` serves the session for its whole life and belongs in its own coroutine, and `disconnectPairedHost` ends it. Only `ResponderExit.PEER_DISCONNECTED` from `resumePairing` authorises dropping the stored pairing. The host persists the peer between answering and serving, which is why those are separate calls.

Two steps around them are the host's. `establishPairing` signs its answer with this host's own SSO statement identity, so `WalletSso` has to be allocated before it runs, and the peer's device statement account has to be tracked alongside it for the peer to author into the session: `parsePairingDeeplink` reads that account out of the deeplink before any notice goes out, and a pairing that then fails untracks it again unless the device was already paired. `disconnectPairedHost` submits the notice and nothing more, so ending a pairing also means cancelling that peer's `resumePairing` coroutine and untracking its renewal account; dropping the stored pairing alone leaves both running. Which undo a failure owes is the thrown case, not the message: `Rejected` means the peer may already have been reached and its target tracked, while `UndecodableDeeplink` is refused before either happens and leaves nothing to undo.

The core prompts for nothing along the way, so asking the user is the host's too. `parsePairingDeeplink` returns the peer's `metadata` alongside it for that prompt: the host name, version, icon and platform the peer put in its QR, trimmed and stripped of the control characters and bidirectional overrides that would otherwise rewrite the prompt's own text around them, capped at 512 characters, and null where nothing renderable was sent. Safe to render is not verified: nothing signs that metadata, so a prompt built from it says what the peer calls itself, never who it is.

The `NativeAnnouncedPairing` that `notifyPairingAllowanceAllocation` returns holds the responder statement secret its notice was signed with, and nothing consumes it, so `destroy()` it once the pairing settles, on the succeeding path as well as the failing one; wrapping the whole pairing in `use { }` covers both.

`devicePaired` on the runtime bridge reports a device that finished pairing with this signing host, carrying the `PairedSsoPeer` the pairing produced. The core has no chat of its own, so announcing the new device to the user's existing contacts is the host's to do. It fires at least once per pairing, so a device that pairs again reports again; a resumed pairing reports nothing, so the host keeps its own record of which devices it has already seen. It arrives on the thread answering the handshake, so marshal the work off rather than announcing it inline. Defaults to a no-op for a host that answers no pairing.

On the execution: `publishChatAction` delivers a user's action back to the product (buffered until it subscribes), `notifyChatRoomsChanged` republishes the room list, `render` returns a `Flow` of `RendererNode` trees for one render context, `publishRendererAction` delivers a renderer action back to the product, and `sessionChatIdentityKey` reads the session's X25519 chat identity key. An open render stream is one worker reference the core holds on the product's behalf; the transition it causes arrives on the runtime bridge's `workerDemandChanged`, never on the execution's. Two rules the core cannot check are the host's to keep: send a render context only for a surface the product's manifest `includes`, and publish a renderer action only from the current tree of an open render stream.

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

The Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `cancelNotification`, `devicePermission`, `remotePermission`, `authStateChanged`, core storage, chain JSON-RPC, `confirmUserAction`, `confirmPermission`, preimage lookup, theme, `featureSupported`, `storage`) reach the embedder through `HostBridge`. Bulletin preimage build/sign/submit now happens inside the core, so the host only serves `lookupPreimage`.

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

Statement-store allowances are granted per period, so a host has to re-register the accounts it wants to keep writing. They are not revoked the moment the period ends: `Resources.StmtStoreGraceWindow` keeps an ended period's allowances active until cleanup catches up, 48 hours on `paseo-next-v2`. The core owns the ledger and the registration; the app owns only the schedule.

Record the accounts to keep allowed. This needs an active session, so call it after `activateLocalSession` or after pairing, not at construction:

```kotlin
runtime.trackStatementRenewalTargets(
    listOf(
        StatementRenewalTarget.WalletSso,
        StatementRenewalTarget.Account(deviceStatementKey, "device"),
    ),
)
```

The ledger persists across launches, and an entry is dropped when the identity that promised it changes. `WalletSso` and `ProductStatementAllowance` are derivation recipes and survive that; `Account` carries a fixed account id and does not. A dropped target is listed in `report.pruned`, which is how a host learns to re-track one and keep renewal covering it. Re-tracking is idempotent, so the safe habit is to re-track the full set after every identity change rather than trying to reason about what survived.

`statementRenewalTargets()` lists what the ledger holds, in the order it was tracked. It needs no active session, so a `WorkManager` worker can read it on a cold start before deciding whether the pass is worth running. Each entry carries an `owner`: a recipe has none and resolves under whichever identity is active, while a fixed account records the root key that promised it. `statementRenewalOwnerKey()` returns that key for the active identity, and needs a session. An entry whose owner is that key, or which has no owner, is one the next pass will renew; any other is one it will prune.

`untrackStatementRenewalAccount(accountId)` drops one fixed account and reports whether the ledger held it. It is scoped to the active identity and so needs a session, and it never removes an entry another identity promised. A stale entry does not deny you a slot forever, since registration replaces the oldest slot past its cooldown once a period is full, but it does cost an allocation attempt every period and keeps churning the slot table, which is what untracking it saves.

Only `Account` can be untracked. `WalletSso` and `ProductStatementAllowance` are recipes with no removal path, so a product you no longer run keeps being resolved and renewed until the promising identity changes.

Then run a pass from a `WorkManager` worker. It submits extrinsics and blocks until they are included, so keep it off the main thread. It needs an active session too, which is the whole difficulty here: a worker on a cold start has none until you restore one, and the pass then fails with the bare reason `Disconnected`. Restore the session first, and read that reason as "not ready" rather than as a renewal failure. `startStatementAllowanceRenewal()` does not need this care, since its loop skips a tick with no session and retries.

```kotlin
val report = runtime.renewStatementAllowances()
report.outcomes.forEach { Log.i(TAG, "${it.label}: ${it.status}") }
report.pruned.forEach {
    // Promised by a previous identity and discarded; re-track to keep it renewed.
    Log.w(TAG, "dropped: $it")
}
if (report.slotsExhausted) {
    // Every slot for this period is taken and none was replaceable.
}
```

One scheduled pass per period is enough, with room to spare: an allowance stays usable for `Resources.StmtStoreGraceWindow` past its boundary, which is 48 hours on `paseo-next-v2`, so a missed run is recoverable rather than fatal. `nextStatementRenewalDelay()` reports the in-process loop's retry cadence, capped at an hour; a worker scheduling one run per period should read a value under an hour as the boundary approaching rather than waking hourly.

### Answering the scheduler

A pass reports per target and only throws when it could not run at all, so decide from the report rather than from the absence of an exception:

- every status `Registered` or `AlreadyAllocated`: `Result.success()`.
- any status `Failed`: `Result.retry()`. The grace window means the retry can wait for the worker's own backoff rather than a tight loop.
- any status `SkippedExhausted`, or `report.slotsExhausted`: `Result.success()`. Retrying cannot free a slot, only time or a replacement can, so a retry here only burns the worker's budget. It does mean an allowance went unrenewed, so tell the person rather than only logging it.
- an exception carrying `Disconnected` before a session is restored: not ready rather than failed. Restore a session and let the next run take it.

Scheduling is one of three layers, and only the first needs the OS:

1. a `WorkManager` run, which is the only one that covers an app nobody opens.
2. a pass on session activation, which covers an app somebody does.
3. on-demand allocation, which registers a product's own account for the current period when that product asks for a statement-store allowance and none is held. That covers the asking product, not the rest of the ledger, so it narrows the window rather than closing it.

`lastStatementRenewalReport()` returns the most recent pass the in-process loop ran, or `null` if none has, which is "not yet" rather than healthy. The loop returns nothing to its caller, so this is where a host driving it reads what it achieved; checking on resume is enough to catch an exhausted period. A direct `renewStatementAllowances()` hands back its own report and does not write here.

`startStatementAllowanceRenewal()` runs the same pass on an in-process loop instead, for a host that stays resident. A pass has no cancellation, so several targets can outlast a constrained worker budget; targets registered before the process is killed are not lost and read back as already allocated.

An account id must be exactly 32 bytes. Anything else is rejected where the bindings convert it, before any chain work happens.

## Example

> **Threading:** the Rust core invokes every `HostBridge` callback on a
> background thread it owns, never the UI thread. Marshal any UI work
> (navigation, prompts, notifications, touching the `WebView`) onto the main
> thread with `Handler(Looper.getMainLooper())` or a `Dispatchers.Main`
> `CoroutineScope`. The `suspend` callbacks (`navigateTo`, `pushNotification`,
> `devicePermission`, `remotePermission`, `featureSupported`,
> `confirmUserAction`, `confirmPermission`, `lookupPreimage`) are awaited by the core, so an
> implementation may suspend for as long as the user takes to decide (e.g.
> `withContext(Dispatchers.Main)` around a prompt); other TrUAPI traffic keeps
> flowing while you wait. The remaining callbacks (auth state, storage, core
> storage, chain, theme, and `cancelNotification`) run inline on the dispatcher
> thread and must return promptly without blocking.

```kt
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.parity.truapi.ContainerScriptBundle
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import io.parity.truapi.HostStorage
import io.parity.truapi.LocalhostBridgeBootstrap
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import io.parity.truapi.TrUAPIHostRuntime
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.truapi.AuthState
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostThemeSubscribeItem
import uniffi.truapi.ThemeName
import uniffi.truapi.ThemeVariant
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.PermissionDecision
import uniffi.truapi.HostPushNotificationRequest

class MyStorage : HostStorage {
    private val map = mutableMapOf<String, ByteArray>()
    override suspend fun read(key: String) = map[key]
    override suspend fun write(key: String, value: ByteArray) { map[key] = value }
    override suspend fun clear(key: String) { map.remove(key) }
}

// Core-owned storage: keyed by SCALE-encoded CoreStorageKey bytes. Back it with
// real persistence (e.g. EncryptedSharedPreferences); an in-memory map is shown
// for brevity.
class MyCoreStorage : HostCoreStorage {
    private val map = HashMap<String, ByteArray>()
    private fun k(key: ByteArray) = key.joinToString("") { "%02x".format(it) }
    override suspend fun read(key: ByteArray) = map[k(key)]
    override suspend fun write(key: ByteArray, value: ByteArray) { map[k(key)] = value }
    override suspend fun clear(key: ByteArray) { map.remove(k(key)) }
}

class MyBridge(private val webView: WebView) : HostBridge {
    private val main = Handler(Looper.getMainLooper())

    override val storage = MyStorage()
    override val coreStorage = MyCoreStorage()

    override suspend fun navigateTo(url: String) {
        withContext(Dispatchers.Main) { /* startActivity(Intent(ACTION_VIEW, Uri.parse(url))) */ }
    }

    override suspend fun pushNotification(request: HostPushNotificationRequest): UInt {
        val id = 1u
        withContext(Dispatchers.Main) { /* show request.text / request.deeplink */ }
        return id
    }

    override fun cancelNotification(id: UInt) {
        main.post { /* cancel notification */ }
    }

    override suspend fun devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest,
    ): PermissionDecision {
        // Awaited by the core: present the prompt for the requested capability
        // (CAMERA, MICROPHONE, ...) and suspend until the user decides. Other
        // TrUAPI traffic keeps flowing while suspended.
        return withContext(Dispatchers.Main) { /* show prompt; */ PermissionDecision.DENY }
    }

    override suspend fun remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission,
    ): PermissionDecision = PermissionDecision.DENY
    override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean = false

    // Core-owned auth state stream: render AuthState.Pairing as the pairing
    // QR sheet, connected/disconnected as the account badge, and login-failed
    // as a retryable error, unless its kind is
    // LoginFailureKind.NoFreeAllowanceSlots, which is unlikely to succeed
    // before the period rolls over, so retry should not be the primary action.
    override fun authStateChanged(state: AuthState) {
        main.post { /* render the state */ }
    }

    override fun chainConnect(genesisHash: ByteArray): UInt? {
        val id = 1u
        main.post { /* open JSON-RPC connection, forward responses via runtime.notifyChainResponse */ }
        return id
    }

    override fun chainSend(connectionId: UInt, request: String) {
        /* send JSON-RPC request on the host connection */
    }

    override fun chainClose(connectionId: UInt) {
        /* close host connection */
    }

    // Switch on the action review variant (SignPayload / SignRaw / CreateTransaction /
    // ResourceAllocation / PreimageSubmit / ...) to render the prompt with its
    // typed fields.
    override suspend fun confirmUserAction(review: UserConfirmationReview): Boolean {
        return withContext(Dispatchers.Main) { /* show prompt; */ false }
    }

    override suspend fun confirmPermission(review: UserConfirmationReview): PermissionDecision {
        return withContext(Dispatchers.Main) { /* show permission prompt; */ PermissionDecision.DENY }
    }
}

val webView: WebView = existingWebView
val bridge = MyBridge(webView)
val runtimeConfig = HostRuntimeConfig(
    hostName = "My Host",
    hostIcon = "https://host.example/icon.png",
    peopleChainGenesisHash = ByteArray(32),
    bulletinChainGenesisHash = ByteArray(32),
    // Stand-in for a real Asset Hub genesis hash. Non-zero on purpose:
    // all-zero is the "no Asset Hub" sentinel and refuses every cross-product
    // `trustedProducts` grant.
    assetHubChainGenesisHash = ByteArray(32) { 1.toByte() },
    networkSuffix = "dot",
    // Optional: activate a local signing session from host-held BIP-39 entropy
    // (no SSO pairing). Omit for the QR pairing flow.
    localSessionSecret = null,
)
val runtime = TrUAPIHostRuntime(bridge, runtimeConfig)
check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
    "WebView lacks DOCUMENT_START_SCRIPT"
}
val container = ContainerScriptBundle.load(webView.context)
val execution = runtime.openProductExecution(
    bridge = bridge,
    configuration = ProductExecutionConfig("my-product.dot", ProductExecutionKind.APP),
)
val endpoint = execution.startWsBridge()

// Call these from host/platform observers so native subscriptions see updates
// after their immediate current item.
execution.notifyThemeChanged(HostThemeSubscribeItem(ThemeName.Default, ThemeVariant.DARK))
execution.notifyPreimageChanged(preimageKey, preimageBytesOrNull)
runtime.notifyChainResponse(chainConnectionId, jsonRpcResponse)
runtime.notifyChainClosed(chainConnectionId)

// Install before loading: evaluateJavascript targets the current document,
// which loadUrl replaces. Only the product main frame receives its endpoint;
// every frame needs the container so child frames cannot bypass its gates.
val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
main.post {
    val productUrl = "https://your-product.example/"
    WebViewCompat.addDocumentStartJavaScript(
        webView,
        "if (window === window.top) {\n$bootstrap\n}",
        setOf("https://your-product.example"),
    )
    WebViewCompat.addDocumentStartJavaScript(webView, container, setOf("*"))
    webView.loadUrl(productUrl)
}

// On logout:
runtime.disconnect()
```

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
