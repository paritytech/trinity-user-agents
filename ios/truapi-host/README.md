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

The embedding app implements `HostBridge` (defined in `TrUAPIHost.swift`): navigation, push, permissions, auth state, scoped + core storage, chain JSON-RPC, confirmations, preimage, theme, feature support, and the served chain set. UI-decision callbacks are `async` and awaited by the Rust core. `HostCallbackAdapter` translates it to the UniFFI-generated `HostCallbacks` protocol; `TrUAPIHostRuntime` and each product execution retain their own adapter. Conform to `HostBridge` rather than to the generated protocol: its extension defaults the optional callbacks, so a newly added one does not break the build. Storage arrives as the `storage` and `coreStorage` sub-objects, which the adapter flattens.

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

## Chat

A host serving the Chat modality implements `ChatHostBridge` and opens the
execution with `ProductExecutionKind.chat`. Hosts without it pass nothing and
Chat calls answer unsupported.

```swift
// Called from a shared dispatch pool, so the backing store must be
// thread-safe, and a slow call here stalls other product executions.
final class MyChatBridge: ChatHostBridge, @unchecked Sendable {
    private let store: ChatStore

    init(store: ChatStore) { self.store = store }

    func createRoom(roomId: String, name: String, icon: String) throws
        -> ChatRoomRegistrationStatus
    {
        store.putRoom(roomId, name: name, icon: icon) ? .new : .exists
    }

    func registerBot(botId: String, name: String, icon: String) throws
        -> ChatBotRegistrationStatus
    {
        store.putBot(botId, name: name, icon: icon) ? .new : .exists
    }

    func postMessage(roomId: String, content: ChatMessageContent) throws -> String {
        if case .file = content {
            // Declining a variant is how a host opts out of rendering one.
            // Throw `HostRejection.Rejected` (or a `LocalizedError`) so the
            // product receives your reason rather than a bare type name.
            throw HostRejection.Rejected(reason: "this host cannot render file cards")
        }
        return store.append(roomId, content: content)
    }

    func listRooms() throws -> [ChatRoom] { store.rooms() }
}

let runtime = try TrUAPIHostRuntime(
    bridge: bridge,
    runtimeConfig: HostRuntimeConfig(
        hostName: "My Chat Host",
        peopleChainGenesisHash: peopleChainGenesisHash,   // exactly 32 bytes
        bulletinChainGenesisHash: bulletinChainGenesisHash,
        assetHubChainGenesisHash: assetHubChainGenesisHash,
        networkSuffix: "dot"
    )
)
// Chat needs an active session; without one every Chat call answers denied.
try runtime.activateLocalSession(secret: secret)

let execution = try runtime.openProductExecution(
    bridge: bridge,
    configuration: ProductExecutionConfig(productId: "chat.dot", executionKind: .chat),
    chat: MyChatBridge(store: store)
)
let endpoint = try execution.startWsBridge()
```

The core bounds and screens the product-supplied fields it forwards — ids,
names, icons, message bodies, URLs, and the action and media counts. Ids and
names are also normalized; a message body is bounded and screened but passed
through byte-for-byte, and `ChatFile.sizeBytes` is product-asserted and
unverified. Contextual output escaping is the host's job.

The id `postMessage` returns is the correlation key `ActionTrigger.messageId`
carries back, so it must name that message for as long as the host stores it.
Ids arriving _in_ a `Reaction` or `ReactionRemoved` are product-chosen and
untrusted: they may name a message in another room, or one that never existed.

## Pocket

A host with a Pocket surface owns the card collection and implements
`PocketHostBridge`, passed as `pocket:` to `openProductExecution`. Pocket is
reachable only from a Worker execution with an active session, so a product
on a signed-out host is denied before the bridge is consulted. Hosts without
the bridge pass nothing and Pocket calls answer unsupported.

```swift
final class MyPocketBridge: PocketHostBridge, @unchecked Sendable {
    private let store: PocketStore

    init(store: PocketStore) { self.store = store }

    // `privileged` marks a card this host pinned, which the product sees and
    // cannot remove.
    func listCards() throws -> [PocketCard] { store.cards() }

    // Decide and remove together so a card cannot be pinned in between.
    func removeCard(cardId: String) throws -> NativePocketRemoval {
        store.removeIfRemovable(cardId)
    }
}

let execution = try runtime.openProductExecution(
    bridge: bridge,
    configuration: ProductExecutionConfig(productId: "game.dot", executionKind: .worker),
    pocket: MyPocketBridge(store: pocketStore)
)

// Pocket needs an active session too: without `activateLocalSession` every
// Pocket call answers denied, whatever this bridge holds.

// Republish after the host's own collection changes.
execution.notifyPocketCardsChanged(cards: pocketStore.cards())
```

A card's face does not cross this bridge. The host keeps each card's newest
face itself: that is what the card shows while the worker is down, and at cold
start before the worker answers.

On the execution: `publishChatAction` delivers a user's action back to the
product, buffering up to 64 before it subscribes; `notifyChatRoomsChanged`
republishes the room list; `render` returns a stream of `RendererNode` trees
for one render context; `publishRendererAction` delivers a renderer action
back to the product; and `sessionChatIdentityKey` reads the session's X25519
chat identity private key, which must not be logged or persisted. An open
render stream is one worker reference the core holds on the product's behalf;
the transition it causes arrives on the runtime bridge's
`workerDemandChanged`, never on the execution's. Two rules the core
cannot check are the host's to keep: send a render context only for a surface
the product's manifest `includes`, and publish a renderer action only from the
current tree of an open render stream.

The runtime answers other devices pairing with it:
`notifyPairingAllowanceAllocation(deeplink:)` and
`notifyPairingFailed(announced:reason:)` are the two notices a peer gets before
the answer, `establishPairing(deeplink:)` is the answer,
`resumePairing(peer:)` serves the session for its whole life and belongs in its
own task, and `disconnectPairedHost(peer:)` ends it. Only
`.peerDisconnected` from `resumePairing` authorises dropping the stored
pairing. The host persists the peer between answering and serving, which is why
those are separate calls.

Two steps around them are the host's. `establishPairing` signs its answer with
this host's own SSO statement identity, so `.walletSso` has to be allocated
before it runs, and the peer's device statement account has to be tracked
alongside it for the peer to author into the session:
`parsePairingDeeplink(deeplink:)` reads that account out of the deeplink before
any notice goes out, and a pairing that then fails untracks it again unless the
device was already paired. `disconnectPairedHost` submits the notice and nothing more, so ending
a pairing also means cancelling that peer's `resumePairing` task and untracking
its renewal account; dropping the stored pairing alone leaves both running.

Which undo a failure owes is the thrown case, not the message: `.rejected`
means the peer may already have been reached and its target tracked, while
`.undecodableDeeplink` is refused before either happens and leaves nothing to
undo.

The core prompts for nothing along the way, so asking the user is the host's
too. `parsePairingDeeplink` returns the peer's `metadata` alongside it for that
prompt: the host name, version, icon and platform the peer put in its QR,
trimmed and stripped of the control characters and bidirectional overrides that
would otherwise rewrite the prompt's own text around them, capped at 512
characters, and `nil` where nothing renderable was sent. Safe to render is not
verified: nothing signs that metadata, so a prompt built from it says what the
peer calls itself, never who it is.

The handle `notifyPairingAllowanceAllocation` returns holds the responder
statement secret its notice was signed with, and nothing consumes it, so drop
the last reference once the pairing settles rather than holding it for the life
of the session.

`devicePaired` on the runtime bridge reports a device that finished pairing
with this signing host, carrying the `PairedSsoPeer` the pairing produced. The
core has no chat of its own, so announcing the new device to the user's
existing contacts is the host's to do. It fires at least once per pairing, so
a device that pairs again reports again; a resumed pairing reports nothing, so
the host keeps its own record of which devices it has already seen. It arrives
on the thread answering the handshake, so hand the device off rather than
announcing it inline. Defaults to a no-op for a host that answers no pairing.

## Game

A host that can hold reminders implements `GameHostBridge`, passed as `game:`
to `openProductExecution`. Hosts without the bridge pass nothing and Game
calls answer unsupported, as they do for every product but the game product,
`dim2`.

```swift
final class MyGameBridge: GameHostBridge, @unchecked Sendable {
    private let reminders: ReminderStore

    init(reminders: ReminderStore) { self.reminders = reminders }

    func scheduleReminder(startsAt: UInt64) async throws {
        try await reminders.hold(startsAt: startsAt)
    }

    func cancelReminder() async throws {
        reminders.drop()
    }
}

let execution = try runtime.openProductExecution(
    bridge: bridge,
    configuration: ProductExecutionConfig(productId: "dim2.dot", executionKind: .worker),
    game: MyGameBridge(reminders: reminderStore)
)
```

The host holds one reminder per product: a `scheduleReminder` replaces the
reminder the same product already holds. The core asks for no per-product
consent: the host asks the OS for what it needs, rings an alarm where the OS
allows one and delivers an ordinary notification otherwise, may add the game to
the user's calendar, keeps the reminder across app kill and device reboot, and
drops it once the game has started. A `scheduleReminder` that throws reaches
the product as a host failure carrying its reason.

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

The bootstrap supplies the execution endpoint to the shared container, which consumes and removes `window.__truapi_localhost` before product scripts run. The container creates one SDK connection for public calls and private permission checks, then exposes its public client through `window.__HOST_API_CLIENT__`. The Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `cancelNotification`, `devicePermission`, `remotePermission`, `authStateChanged`, core storage, chain JSON-RPC, confirmations, preimage, theme, `featureSupported`, `storage`) reach the embedder through `HostCallbacks`.

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

`TrUAPIHostRuntime` exposes two methods for wallet-owned SSO sessions. Meaningful request answering requires `activateLocalSession` to have been called first; `prepareDisconnectRequest` needs no session.

```swift
func handleSsoRequest(message: Data) async throws -> SsoRequestOutcome
func prepareDisconnectRequest() -> Data
```

`handleSsoRequest(message:)` takes one SCALE-encoded `RemoteMessage` exactly as decrypted from the statement-store session and routes it through the Rust core. The returned `SsoRequestOutcome` is the generated UniFFI enum (no Swift mirror):

- `.response(message:)` — SCALE-encoded reply; post it back over the same session.
- `.disconnected` — the peer ended the session; tear down the transport and records on the wallet side.
- `.ignored` — the message was not a request; nothing to post.

Confirmation-gated requests suspend on `confirmUserAction` or `confirmPermission`, so `handleSsoRequest` can take arbitrarily long. Always call it from a `Task`, never the main thread.

`prepareDisconnectRequest()` returns the SCALE-encoded `Disconnected` message to post when the wallet is ending the session. Posting and record cleanup (host entry, device record, device-removed broadcast) stay with the wallet.

## Statement-store allowance renewal

Statement-store allowances are granted per period, so a host has to re-register the accounts it wants to keep writing. They are not revoked the moment the period ends: `Resources.StmtStoreGraceWindow` keeps an ended period's allowances active until cleanup catches up, 48 hours on `paseo-next-v2`. The runtime owns the ledger and the registration; the app owns only the schedule.

Record the accounts to keep allowed. This needs an active session, so call it after `activateLocalSession` or after pairing, not at construction:

```swift
try runtime.trackStatementRenewalTargets([
    .walletSso,
    .account(accountId: deviceStatementKey, label: "device"),
])
```

The ledger persists across launches, and an entry is dropped when the identity that promised it changes. `.walletSso` and `.productStatementAllowance` are derivation recipes, so they survive that; `.account` carries a fixed account id and does not. A dropped target is listed in `report.pruned`, which is how a host learns to re-track one and keep renewal covering it. Re-tracking is idempotent, so the safe habit is to re-track the full set after every identity change rather than trying to reason about what survived.

`statementRenewalTargets()` lists what the ledger holds, in the order it was tracked. It needs no active session, so a `BGTaskScheduler` wake can read it on a cold start before deciding whether the pass is worth running. Each entry carries an `owner`: a recipe has none and resolves under whichever identity is active, while a fixed account records the root key that promised it. `statementRenewalOwnerKey()` returns that key for the active identity, and needs a session. An entry whose owner is that key, or which has no owner, is one the next pass will renew; any other is one it will prune.

`untrackStatementRenewalAccount(accountId:)` drops one fixed account and reports whether the ledger held it. It is scoped to the active identity and so needs a session, and it never removes an entry another identity promised. A stale entry does not deny you a slot forever, since registration replaces the oldest slot past its cooldown once a period is full, but it does cost an allocation attempt every period and keeps churning the slot table, which is what untracking it saves.

Only `.account` can be untracked. `.walletSso` and `.productStatementAllowance` are recipes with no removal path, so a product you no longer run keeps being resolved and renewed until the promising identity changes.

Then run a pass from a background task, off the main thread. It needs an active session too, which is the whole difficulty here: a `BGTaskScheduler` wake on a cold start has none until you restore one, and the pass then fails with the bare reason `Disconnected`. Restore the session first, and read that reason as "not ready" rather than as a renewal failure. `startStatementAllowanceRenewal()` does not need this care, since its loop skips a tick with no session and retries.

```swift
let report = try runtime.renewStatementAllowances()
for outcome in report.outcomes {
    log("\(outcome.label): \(outcome.status)")
}
for label in report.pruned {
    // Promised by a previous identity and discarded; re-track to keep it renewed.
    log("dropped: \(label)")
}
if report.slotsExhausted {
    // Every slot for this period is taken and none was replaceable.
}
```

One scheduled pass per period is enough, with room to spare: an allowance stays usable for `Resources.StmtStoreGraceWindow` past its boundary, which is 48 hours on `paseo-next-v2`, so a missed wake-up is recoverable rather than fatal. `nextStatementRenewalDelay()` reports the in-process loop's retry cadence, capped at an hour; a `BGTaskScheduler` host should read a value under an hour as the boundary approaching rather than requesting a wake-up every hour for a pass that will almost always report `alreadyAllocated`.

### Answering the scheduler

A pass reports per target and only throws when it could not run at all, so decide from the report rather than from the absence of an error:

- every status `Registered` or `AlreadyAllocated`: completed successfully.
- any status `Failed`: complete unsuccessfully and submit a fresh request, since iOS does not reschedule one for you. The grace window means that request can wait for the next opportunistic wake rather than a tight loop.
- any status `SkippedExhausted`, or `report.slotsExhausted`: completed successfully. Retrying cannot free a slot, only time or a replacement can, so a retry here only burns background budget. It does mean an allowance went unrenewed, so tell the person rather than only logging it.
- a throw carrying `Disconnected` before a session is restored: not ready rather than failed. Restore a session and let the next wake run the pass.

Scheduling is one of three layers, and only the first needs the OS:

1. a `BGTaskScheduler` wake, which is the only one that covers an app nobody opens.
2. a pass on session activation, which covers an app somebody does.
3. on-demand allocation, which registers a product's own account for the current period when that product asks for a statement-store allowance and none is held. That covers the asking product, not the rest of the ledger, so it narrows the window rather than closing it.

`lastStatementRenewalReport()` returns the most recent pass the in-process loop ran, or `nil` if none has, which is "not yet" rather than healthy. The loop returns nothing to its caller, so this is where a host driving it reads what it achieved; checking on resume is enough to catch an exhausted period. A direct `renewStatementAllowances()` hands back its own report and does not write here.

`startStatementAllowanceRenewal()` runs the same pass on an in-process loop instead. It suits a host that stays resident; on iOS a suspended app stops ticking, so prefer `BGTaskScheduler` driving the one-shot call. A pass has no cancellation, so several targets can outlast a short background budget; targets registered before the process is killed are not lost, and read back as already allocated next time.

An account id must be exactly 32 bytes. Anything else is rejected where the bindings convert it, before any chain work happens.

## Example

> **Threading:** the Rust core invokes every `HostCallbacks` method on a
> background thread it owns, never the main thread. Hop to the main thread
> (`MainActor` / `DispatchQueue.main`) before touching UIKit, WebKit, or the
> `WKWebView`. The `async` callbacks (`navigateTo`, `pushNotification`,
> `devicePermission`, `remotePermission`, `featureSupported`,
> `confirmUserAction`, `confirmPermission`, `lookupPreimage`) are awaited by the core, so an
> implementation may suspend for as long as the user takes to decide (e.g.
> `await MainActor.run { ... }` or an `withCheckedContinuation` around a
> prompt); other TrUAPI traffic keeps flowing while you wait. The remaining
> sync callbacks (auth state, storage, core storage, chain, theme,
> `cancelNotification`) run inline on the dispatcher thread and must return
> promptly without blocking.

```swift
import Foundation
import WebKit
import TrUAPIHost

final class MyStorage: HostStorageBackend, @unchecked Sendable {
    private var values: [String: Data] = [:]

    func read(key: String) throws -> Data? { values[key] }
    func write(key: String, value: Data) throws { values[key] = value }
    func clear(key: String) throws { values.removeValue(forKey: key) }
}

final class MyCoreStorage: HostCoreStorageBackend, @unchecked Sendable {
    private var values: [Data: Data] = [:]

    func read(key: Data) throws -> Data? { values[key] }
    func write(key: Data, value: Data) throws { values[key] = value }
    func clear(key: Data) throws { values.removeValue(forKey: key) }
}

final class MyBridge: HostBridge, @unchecked Sendable {
    let storage: HostStorageBackend = MyStorage()
    let coreStorage: HostCoreStorageBackend = MyCoreStorage()

    func onCoreLog(marker: String, detail: String) { /* log */ }

    func navigateTo(url: String) async throws {
        await MainActor.run { /* UIApplication.shared.open(...) */ }
    }

    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32 {
        let id: UInt32 = 1
        await MainActor.run { /* schedule request.text / request.deeplink / request.scheduledAt */ }
        return id
    }

    func cancelNotification(id: UInt32) throws {
        DispatchQueue.main.async { /* cancel notification */ }
    }

    func devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest
    ) async throws -> PermissionDecision {
        // Awaited by the core: present the prompt and suspend until the user
        // decides. Other TrUAPI traffic keeps flowing while suspended.
        await MainActor.run { /* show prompt for request (.camera, .microphone, ...); */ PermissionDecision.deny }
    }

    func remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission
    ) async throws -> PermissionDecision {
        await MainActor.run { /* show prompt for request (.chainSubmit, .remote(domains:), ...); */ PermissionDecision.deny }
    }

    // Core-owned auth state stream: render `.connected`/`.disconnected` as the
    // account badge and `.loginFailed` as a retryable error, unless its `kind`
    // is `.noFreeAllowanceSlots`, which is unlikely to succeed before the
    // period rolls over, so retry should not be the primary action. This native
    // runtime is a signing host, so `.pairing` and `.authenticating` are not
    // emitted. Activate the session with `runtime.activateLocalSession(...)`.
    func authStateChanged(state: AuthState) {
        DispatchQueue.main.async { /* render the state */ }
    }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        let id: UInt32 = 1
        DispatchQueue.main.async { /* open JSON-RPC connection, forward responses via runtime.notifyChainResponse */ }
        return id
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        /* send JSON-RPC request on the host connection */
    }

    func chainClose(connectionId: UInt32) throws {
        /* close host connection */
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        // Switch on the review variant (.signPayload, .createTransaction, ...)
        // to render the confirmation prompt with its typed fields.
        await MainActor.run { /* render review; */ false }
    }

    func confirmPermission(review: UserConfirmationReview) async throws -> PermissionDecision {
        await MainActor.run { /* render permission review; */ PermissionDecision.deny }
    }

    func lookupPreimage(key: Data) async throws -> Data? { nil }

    func currentTheme() throws -> HostThemeSubscribeItem {
        HostThemeSubscribeItem(name: .default, variant: .dark)
    }

    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool { false }

}

let bridge = MyBridge()
let runtimeConfig = HostRuntimeConfig(
    hostName: "My Host",
    hostIcon: "https://host.example/icon.png",
    peopleChainGenesisHash: Data(repeating: 0, count: 32),
    bulletinChainGenesisHash: Data(repeating: 0, count: 32),
    // Stand-in for a real Asset Hub genesis hash. Non-zero on purpose:
    // all-zero is the "no Asset Hub" sentinel and refuses every cross-product
    // `trustedProducts` grant.
    assetHubChainGenesisHash: Data(repeating: 1, count: 32),
    networkSuffix: "dot"
)
let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: runtimeConfig)
try runtime.activateLocalSession(secret: entropyBytes, liteUsername: nil)
let execution = try runtime.openProductExecution(
    bridge: bridge,
    configuration: ProductExecutionConfig(
        productId: "my-product.dot",
        executionKind: .app
    )
)
// The endpoint stays valid across backgrounding: iOS reclaims a suspended
// app's listening socket, and the runtime rebinds it on the same port when
// the app returns to the foreground.
let endpoint = try execution.startWsBridge()

// Call these from host/platform observers so native subscriptions see updates
// after their immediate current item.
execution.notifyThemeChanged(
    theme: HostThemeSubscribeItem(name: .default, variant: .dark)
)
execution.notifyPreimageChanged(key: preimageKey, value: preimageBytesOrNil)
runtime.notifyChainResponse(connectionId: chainConnectionId, json: jsonRpcResponse)
runtime.notifyChainClosed(connectionId: chainConnectionId)

// Install before loading. The product URL comes from trusted host resolution.
let configuration = WKWebViewConfiguration()
let webView = WKWebView(frame: .zero, configuration: configuration)
let productURL = URL(string: "https://your-product.example/")!
try TrUAPIHost.installProductScripts(
    into: webView,
    endpoint: endpoint
)
webView.load(URLRequest(url: productURL))

// Settings changes apply to subsequent permission-checked operations.
try execution.setPermissionAuthorizationStatus(
    request: .remote(RemotePermissionRequest(permission: .remote(domains: ["api.example.com"]))),
    status: .denied
)

// On view teardown:
webView.stopLoading()
execution.close()

// On logout:
runtime.disconnect()
```

The updated `@parity/truapi` SDK keeps the same client across connection loss. The SDK replaces the socket; interrupted operations fail with `ConnectionResetError` and are never replayed. Recreate read/watch subscriptions in the provider that owns them. SDKs 0.16.0 and 0.18.0 can still start through the minimal `__HOST_API_PORT__` adapter, but require a page reload after a disconnect. Remove that adapter once deployed products adopt the injected client.

The shared container uses the same WebSocket as SDK calls and asks Rust to authorize each fetch or XHR before sending it, and each remote WebSocket before connecting. It parses the URL with captured browser primitives and sends its hostname to `authorize_remote_permission`; Rust normalizes and checks the domain. Swift supplies the endpoint and handles native permission prompts; it does not relay individual network permission messages. An upfront permission request and a network operation are separate, so an Allow once decision is consumed by the next permitted operation rather than persisted.

XHR keeps native request headers, response types and browser CORS behavior. `open()` configures the request synchronously; `send()` waits for permission before sending. Aborting or reopening during that wait cancels the pending send. Synchronous XHR is unsupported because it cannot wait for an asynchronous permission decision.

A remote `WebSocket` starts in `CONNECTING` while Rust checks the same domain permission. Allow once permits that connection and all its messages; a new connection checks again. Closing while permission is pending prevents the connection from opening. Text, binary messages and subprotocols use the native socket after approval. The private host connection uses the browser constructor captured before these gates are installed. Product-created sockets receive no endpoint exemption.

Forwarded WebSocket events and XHR failures before sending are synthetic, with `isTrusted` set to `false`.

WebRTC uses the same private transport. Each peer connection asks Rust for permission at its first network method, such as `createOffer`, and shares that decision across later methods on the connection. Allow once permits one connection. New connections check the current permission without requiring a page reload.

To disable WebRTC, call `execution.setPermissionAuthorizationStatus` with a remote `.webRtc` request and `.denied` before loading each product. This overrides saved grants and trusted-product auto-grants, which otherwise skip `remotePermission` callbacks.

The installer adds the bootstrap and container scripts before loading. It preserves the host's website data store and navigation delegate. Hosts that assemble their own script lists can keep using `LocalhostBridgeBootstrap.script` followed by `ContainerScriptBundle.load()`, with the container injected into every frame.

`Worker`, `WebTransport` and `getDisplayMedia` screen capture are unavailable. Workers would provide a separate realm with unguarded network APIs; WebTransport has no permission wrapper, and screen capture has no product permission.

Redirects and stylesheet/font loads retain native WebKit behavior. Redirect destinations are not separately authorized by the fetch/XHR wrappers; direct DOM resource loads remain outside those wrappers. There is no content-rule registration, global settings refresh or installation disposal requirement. Close the execution when its product stops, and maintain the host's existing web-view navigation and teardown behavior.

Build the generated JavaScript SDK before the container: from the repository root, run `npm ci --ignore-scripts`, `npm run build --prefix js/packages/truapi`, then `npm run build --prefix js/container`. A protocol change also requires regenerating the SDK through the repository's normal build pipeline.

`ProductNetworkAccessTests` exercises grant/deny/revocation, one-use fetch, WebRTC and media authorization, native redirects, stylesheet/font requests, and preserving a persistent store and existing navigation delegate. Media coverage uses a capture stub with the actual private Rust permission transport; it does not require simulator camera hardware. The tests require the built container, current Rust bindings and a real WKWebView in the UIKit test host. These Apple-only tests cannot run on Linux.


## Build outputs in detail

`./scripts/rebuild.sh` orchestrates everything; the underlying pieces, should you need one in isolation:

- **xcframework** — `make xcframework` (repo root) builds `truapi` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` and bundles `target/truapi_server.xcframework`; the script copies it into `Binaries/` and strips the per-slice `module.modulemap` (module resolution comes from the `systemLibrary` target; the slice copy collides with other xcframeworks in Xcode's flat include dir).
- **bindings** — `make uniffi` (run automatically by `make xcframework`) emits the Swift bindings into `target/uniffi-swift-out/` via the workspace `uniffi-bindgen-cli`; `scripts/sync-bindings.sh` copies them into `Sources/TrUAPIHost/truapi.swift` and `Sources/truapiFFI/include/`, renaming the emitted `truapiFFI.modulemap` to `module.modulemap` so the SwiftPM `systemLibrary` target picks it up. `rebuild.sh` calls it, and so does the `iOS package (Swift + WebKit)` job, which is what puts Swift sources into the package before `xcodebuild` runs.
- **container** — `npm run build` in `js/container/` (repo root) bundles `src/index.ts` into `Sources/TrUAPIHost/Resources/truapi-container.js`.
