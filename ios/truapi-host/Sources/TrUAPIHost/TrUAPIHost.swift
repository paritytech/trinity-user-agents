// TrUAPIHost - iOS host adapter.
//
// The Rust core (compiled to `libtruapi`, surfaced through UniFFI in the
// sibling `truapi.swift` file) owns wire decoding, request
// routing, subscription lifecycle, and platform trait dispatch.
//
// This file exposes the process-owned `TrUAPIHostRuntime`, its independently
// scoped `TrUAPIProductExecution` connections, and the
// `LocalhostBridgeBootstrap` helper used to publish an execution's WS endpoint.
//
// Products running inside a `WKWebView` connect to the Rust core via the
// localhost WebSocket bridge. The bootstrap publishes its endpoint in
// `window.__truapi_localhost` for the shared container to consume.

import Foundation
import UIKit

/// Package metadata.
public enum TrUAPIHost {
    public static let version = "0.1.0"
}

/// Bootstrap helper for the native localhost WebSocket bridge that a product
/// execution starts.
public enum LocalhostBridgeBootstrap {
    /// Publishes the WebSocket endpoint for the product's SDK.
    /// Inject at document start, before the container and product scripts.
    public static func script(port: UInt16, token: String) -> String {
        localhostBridgeBootstrapScript(port: port, token: token)
    }
}

/// Product-scoped key-value storage provided by the embedding host.
public protocol HostStorageBackend: AnyObject, Sendable {
    func read(key: String) throws -> Data?
    func write(key: String, value: Data) throws
    func clear(key: String) throws
}

/// Core-owned host-private storage backend. Keys are SCALE-encoded
/// `truapi::platform::CoreStorageKey` values, so embedders can persist them
/// opaquely or decode them to choose a secure backing store per slot.
public protocol HostCoreStorageBackend: AnyObject, Sendable {
    func read(key: Data) throws -> Data?
    func write(key: Data, value: Data) throws
    func clear(key: Data) throws
}

/// Host-side callback bundle that the Rust core invokes for capabilities the
/// native shell owns. The permission split mirrors the Rust `Permissions`
/// trait:
///
///   * ``devicePermission(product:request:)`` handles OS-scoped grants
///     (camera, mic, location).
///   * ``remotePermission(product:request:)`` handles per-product capability
///     bundles.
///
/// The Rust core invokes callbacks on its shared background bridge executor.
/// Async callbacks must suspend while waiting for a decision; blocking their
/// thread stalls other TrUAPI traffic. Synchronous callbacks must return promptly.
/// Run UI work on the main actor, for example with `await MainActor.run { ... }`.
public protocol HostBridge: AnyObject, Sendable {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    func onCoreLog(marker: String, detail: String)

    /// Open a URL in the system browser, suspending for any approval on the main actor.
    func navigateTo(url: String) async throws

    /// Deliver a push notification (`HostPushNotificationRequest`)
    /// and return the host-assigned notification id. Run any UI work on the main actor.
    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32

    /// Cancel a previously scheduled notification id.
    func cancelNotification(id: UInt32) throws

    /// Prompt for a device-level permission `product` requested on the main
    /// actor, suspending until the user decides. Preserve the approval lifetime.
    func devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest
    ) async throws -> PermissionDecision

    /// Report the OS status of a device capability without prompting. Answer
    /// from the platform's authorization APIs, for example
    /// `AVCaptureDevice.authorizationStatus(for:)` or
    /// `UNUserNotificationCenter.getNotificationSettings`.
    ///
    /// The core calls this before every device-permission request and status
    /// read, so it must not show UI. A capability your app has no OS gate for
    /// answers `.notApplicable`, which leaves the stored product decision
    /// governing. Defaults to `.notApplicable`, so an app that does not
    /// implement it keeps today's behaviour.
    func devicePermissionStatus(request: HostDevicePermissionRequest) async throws
        -> DevicePermissionStatus

    /// Prompt for a remote permission bundle `product` requested on the main
    /// actor, suspending until the user decides.
    func remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission
    ) async throws -> PermissionDecision

    /// Observe an auth state change, in transition order: render `.pairing` as
    /// the pairing QR UI, `.connected`/`.disconnected` as the account badge,
    /// and `.loginFailed` as a retryable error, unless its `kind` is
    /// `.noFreeAllowanceSlots`, which is unlikely to succeed before the period
    /// rolls over, so retry should not be the primary action. A pairing host's
    /// session activation reports its outcome even
    /// when it is the default `.disconnected`, so a host that awaits activation
    /// before routing never has to read silence as "signed out"; every other
    /// emission, and every emission on a host role that has no session
    /// activation, happens only when the state actually changes.
    /// Invoked on the dispatcher thread; hand the state to the main thread and
    /// return promptly.
    func authStateChanged(state: AuthState)

    /// Open a JSON-RPC chain connection and return a host-assigned id, or nil if unsupported.
    func chainConnect(genesisHash: Data) throws -> UInt32?

    /// Send one JSON-RPC request on a native chain connection.
    func chainSend(connectionId: UInt32, request: String) throws

    /// Close a native chain connection.
    func chainClose(connectionId: UInt32) throws

    /// Confirm one user-reviewed core action before it continues.
    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool

    /// Preserve the selected lifetime for identity and account access consent.
    func confirmPermission(review: UserConfirmationReview) async throws -> PermissionDecision

    /// Return the current preimage value for `key`, or nil for a miss.
    func lookupPreimage(key: Data) async throws -> Data?

    /// Return the current host theme. Hosts with no named themes report
    /// `ThemeName.default`.
    func currentTheme() throws -> HostThemeSubscribeItem

    /// Return the language this host presents its interface in, as a BCP 47
    /// tag. Hosts with no in-app language picker report the system language.
    func currentLocale() throws -> HostLocaleSubscribeItem

    /// Answer a feature-support query. Invoked on the dispatcher thread; must
    /// return promptly.
    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool

    /// Enumerate the chains this host serves: its environment plus one entry
    /// per chain role. Invoked on the dispatcher thread; must return promptly.
    func supportedChains() throws -> HostChainSet

    /// Observe demand on a product's worker crossing zero. `.start` means run
    /// the worker now, `.stop` that nothing wants it any more. Every
    /// transition arrives here in ledger order, the ones the app asks for by
    /// taking a reference of its own included.
    ///
    /// Demand is runtime-wide, so the core invokes this only on the bridge
    /// ``TrUAPIHostRuntime/init(bridge:runtimeConfig:)`` was given, never on
    /// the per-execution bridge passed to
    /// ``TrUAPIHostRuntime/openProductExecution(bridge:configuration:chat:pocket:game:)``.
    /// Can arrive on any thread, including synchronously on the calling
    /// thread during `acquireWorker`/`releaseWorker`, often the main thread
    /// and re-entrantly: hand the transition off rather than blocking on
    /// another thread from inside it. Defaults to a no-op for a host that
    /// runs no workers.
    func workerDemandChanged(productId: String, transition: WorkerTransition)

    /// Begin a pending operation, whose id keeps the product's worker alive
    /// until it ends. `label` is a log/UI hint, empty when the product gave none.
    func beginOperation(productId: String, label: String) async throws -> UInt32

    /// End a pending operation. Idempotent: an unknown or already-ended id
    /// succeeds, so a retry after an ambiguous failure is safe.
    func endOperation(productId: String, id: UInt32) async throws

    /// A device finished pairing with this signing host.
    ///
    /// The core has no chat of its own, so announcing the new device to the
    /// user's existing contacts is the host's to do. At least once per
    /// pairing, and the host keeps its own record of which devices it has
    /// already seen: a resumed pairing reports nothing and the core has no
    /// list to replay. Arrives on the thread answering the handshake, while
    /// the pairing call is still running: hand the device off rather than
    /// announcing it inline. Defaults to a no-op for a host that answers no
    /// pairing.
    func devicePaired(device: PairedSsoPeer)

    /// Scoped key-value storage for the Rust core.
    var storage: HostStorageBackend { get }

    /// Core-owned host-private storage for auth session, pairing identity,
    /// and persisted permission decisions.
    var coreStorage: HostCoreStorageBackend { get }

}

/// Native Chat storage and UI surface. Implement and pass to
/// ``TrUAPIHostRuntime/openProductExecution(bridge:configuration:chat:pocket:game:)``
/// when the host supports the Chat modality; hosts without it pass nothing.
/// Native Chat storage and UI surface, called from the process-wide dispatch
/// pool shared by every product execution: implementations must be safe to
/// enter concurrently.
///
/// Throw ``HostRejection`` (or an error conforming to `LocalizedError`) to
/// decline a call. A plain `Error` reaches the product as its type name alone,
/// because a value's stored properties would otherwise cross to it.
public protocol ChatHostBridge: AnyObject, Sendable {
    /// Create or resolve a native product Chat room. The core has bounded and
    /// normalized these arguments and screened the icon scheme; escaping them
    /// for the surface that renders them is still the host's job.
    func createRoom(roomId: String, name: String, icon: String) async throws
        -> ChatRoomRegistrationStatus

    /// Register or resolve a native product Chat bot. The core has bounded and
    /// normalized these arguments and screened the icon scheme; escaping them
    /// for the surface that renders them is still the host's job.
    func registerBot(botId: String, name: String, icon: String) async throws
        -> ChatBotRegistrationStatus

    /// Persist a product-authored message in native Chat storage. Throw for a
    /// content variant this host cannot render.
    ///
    /// The core has bounded and screened every field, but a body passes
    /// through byte-for-byte and `ChatFile.sizeBytes` is an unverified product
    /// assertion, so escaping and sizing remain the host's job.
    ///
    /// The returned id is what `ActionTrigger.messageId` carries back, so it
    /// must name this message for as long as the host stores it. An id
    /// arriving in a `reaction` or `reactionRemoved` is product-chosen and
    /// untrusted: it may name a message in another room, or none at all.
    func postMessage(roomId: String, content: ChatMessageContent) async throws -> String

    /// Return the current product-scoped native Chat rooms.
    func listRooms() async throws -> [ChatRoom]
}

/// Native Pocket collection surface. Implement and pass to
/// ``TrUAPIHostRuntime/openProductExecution(bridge:configuration:chat:pocket:game:)``
/// when the host has a Pocket surface; hosts without one pass nothing. Called
/// from the process-wide dispatch pool shared by every product execution:
/// implementations must be safe to enter concurrently, and one that blocks
/// stalls the others.
///
/// Throw ``HostRejection`` (or an error conforming to `LocalizedError`) to
/// decline a call.
public protocol PocketHostBridge: AnyObject, Sendable {
    /// Return the product's cards as this host holds them, each carrying
    /// whether the host pinned it.
    func listCards() throws -> [PocketCard]

    /// Remove one of the product's cards and report what happened. Decide and
    /// remove together, under whatever lock this host holds, so a card cannot
    /// be pinned between the two.
    func removeCard(cardId: String) throws -> NativePocketRemoval
}

/// Native game-reminder surface. Implement and pass to
/// ``TrUAPIHostRuntime/openProductExecution(bridge:configuration:chat:pocket:game:)``
/// when the host can hold reminders; hosts without one pass nothing. Both
/// calls are async, so an implementation may hop to the main actor to answer;
/// implementations must be safe to enter concurrently.
///
/// The host holds one reminder per product: a schedule replaces the reminder
/// the same product already holds. The core asks for no per-product consent;
/// the host asks the OS for what it needs, rings an alarm where the OS allows
/// one and delivers a notification otherwise, may add the game to the
/// calendar, keeps the reminder across app kill and reboot, and drops it once
/// the game starts.
///
/// Both calls throw ``HostRejection`` (or an error conforming to
/// `LocalizedError`) to decline. A failed schedule reaches the product as a
/// host failure carrying its reason, a failed cancel as its generic error.
public protocol GameHostBridge: AnyObject, Sendable {
    /// Hold `startsAt` (Unix milliseconds, UTC) as this product's reminder,
    /// replacing any it holds. Throw when the OS allows neither alarms nor
    /// notifications.
    func scheduleReminder(startsAt: UInt64) async throws

    /// Drop this product's reminder. Dropping none succeeds.
    func cancelReminder() async throws
}

/// Host-implemented contacts surface: a lookup from handles to contacts, and
/// the picker drawn over them.
///
/// Installed once on the runtime with
/// ``TrUAPIHostRuntime/setContacts(_:)``, because the list belongs to the host
/// and not to any one product. A runtime without one answers `contacts.pick`
/// with `Unsupported`.
///
/// Nothing here reaches a product, and the list never reaches the core: it
/// asks only about the handles a transaction names, and the picker returns the
/// one person the user chose. Omit the contacts the user has blocked, from both.
public protocol ContactsHostBridge: AnyObject, Sendable {
    /// Resolve `lookup.handles` to contacts: one entry per handle, in order,
    /// `nil` where none matches. A contact's handle is BLAKE2b-256 keyed with
    /// `lookup.handleKey` over its 32-byte account. Called inline, so answer
    /// from what is already in hand.
    func contacts(lookup: HostContactLookup) throws -> HostContactMatches

    /// Present the picker on behalf of `productId` and report what the user
    /// did. With no contacts, answer `.noContacts` instead of drawing an empty
    /// overlay.
    func pickContact(productId: String) async throws -> HostContactPick
}

public extension HostBridge {
    /// Default no-op logger. Override to plumb into your logging framework.
    func onCoreLog(marker: String, detail: String) {}
    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32 { 0 }
    func cancelNotification(id: UInt32) throws {}
    func authStateChanged(state: AuthState) {}
    func chainConnect(genesisHash: Data) throws -> UInt32? { nil }
    func chainSend(connectionId: UInt32, request: String) throws {}
    func chainClose(connectionId: UInt32) throws {}
    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool { false }
    func confirmPermission(review: UserConfirmationReview) async throws -> PermissionDecision {
        try await confirmUserAction(review: review) ? .allowAlways : .deny
    }
    func lookupPreimage(key: Data) async throws -> Data? { nil }
    func currentTheme() throws -> HostThemeSubscribeItem {
        HostThemeSubscribeItem(name: .default, variant: .dark)
    }
    func currentLocale() throws -> HostLocaleSubscribeItem {
        HostLocaleSubscribeItem(
            languageTag: Locale.current.language.languageCode?.identifier ?? "en"
        )
    }
    func supportedChains() throws -> HostChainSet { HostChainSet(network: "", chains: []) }
    func workerDemandChanged(productId: String, transition: WorkerTransition) {}
    func devicePaired(device: PairedSsoPeer) {}
    func devicePermissionStatus(request: HostDevicePermissionRequest) async throws
        -> DevicePermissionStatus { .notApplicable }
    /// Defaults opt out of worker keep-alive; override to run background work
    /// past the product's surface. The id is still distinct per call, because
    /// an `OperationId` names one operation: a host overriding only
    /// `endOperation`, and the core's own demand accounting, both end the
    /// wrong ones when every operation shares an id.
    func beginOperation(productId: String, label: String) async throws -> UInt32 {
        defaultOperationIds.take()
    }

    func endOperation(productId: String, id: UInt32) async throws {}
}

/// Ids handed out by the default `beginOperation`, distinct for the life of
/// the process.
private final class DefaultOperationIds: @unchecked Sendable {
    private let lock = NSLock()
    private var nextId: UInt32 = 1

    func take() -> UInt32 {
        lock.lock()
        defer { lock.unlock() }
        let id = nextId
        // Never zero, and never traps: an id is only ever compared, so wrapping
        // back to one costs nothing.
        nextId = nextId == UInt32.max ? 1 : nextId + 1
        return id
    }
}

private let defaultOperationIds = DefaultOperationIds()

/// Adapter that bridges the public `ChatHostBridge` to the generated UniFFI
/// `NativeChatCallbacks` protocol.
private final class ChatCallbackAdapter: NativeChatCallbacks, @unchecked Sendable {
    private let bridge: ChatHostBridge

    init(bridge: ChatHostBridge) {
        self.bridge = bridge
    }

    func createRoom(
        roomId: String,
        name: String,
        icon: String
    ) async throws -> ChatRoomRegistrationStatus {
        try await withHostRejection {
            try await bridge.createRoom(roomId: roomId, name: name, icon: icon)
        }
    }

    func registerBot(
        botId: String,
        name: String,
        icon: String
    ) async throws -> ChatBotRegistrationStatus {
        try await withHostRejection {
            try await bridge.registerBot(botId: botId, name: name, icon: icon)
        }
    }

    func postMessage(roomId: String, content: ChatMessageContent) async throws -> String {
        try await withHostRejection {
            try await bridge.postMessage(roomId: roomId, content: content)
        }
    }

    func listRooms() async throws -> [ChatRoom] {
        try await withHostRejection { try await bridge.listRooms() }
    }

    private func withHostRejection<T>(_ operation: () async throws -> T) async throws -> T {
        do {
            return try await operation()
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `PocketHostBridge` to the generated UniFFI
/// `NativePocketCallbacks` protocol.
private final class PocketCallbackAdapter: NativePocketCallbacks, @unchecked Sendable {
    private let bridge: PocketHostBridge

    init(bridge: PocketHostBridge) {
        self.bridge = bridge
    }

    func listCards() throws -> [PocketCard] {
        try withHostRejection { try bridge.listCards() }
    }

    func removeCard(cardId: String) throws -> NativePocketRemoval {
        try withHostRejection { try bridge.removeCard(cardId: cardId) }
    }

    private func withHostRejection<T>(_ operation: () throws -> T) throws -> T {
        do {
            return try operation()
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `GameHostBridge` to the generated UniFFI
/// `NativeGameCallbacks` protocol.
private final class GameCallbackAdapter: NativeGameCallbacks, @unchecked Sendable {
    private let bridge: GameHostBridge

    init(bridge: GameHostBridge) {
        self.bridge = bridge
    }

    func scheduleReminder(startsAt: UInt64) async throws {
        try await withHostRejection { try await bridge.scheduleReminder(startsAt: startsAt) }
    }

    func cancelReminder() async throws {
        try await withHostRejection { try await bridge.cancelReminder() }
    }

    private func withHostRejection<T>(_ operation: () async throws -> T) async throws -> T {
        do {
            return try await operation()
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `ContactsHostBridge` to the generated
/// UniFFI `NativeContactsCallbacks` protocol.
private final class ContactsCallbackAdapter: NativeContactsCallbacks, @unchecked Sendable {
    private let bridge: ContactsHostBridge

    init(bridge: ContactsHostBridge) {
        self.bridge = bridge
    }

    func contacts(lookup: HostContactLookup) throws -> HostContactMatches {
        do {
            return try bridge.contacts(lookup: lookup)
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }

    func pickContact(productId: String) async throws -> HostContactPick {
        do {
            return try await bridge.pickContact(productId: productId)
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `HostBridge` to the generated UniFFI
/// `HostCallbacks` protocol. Kept private so the generated names never
/// leak into consumers.
private final class HostCallbackAdapter: HostCallbacks, @unchecked Sendable {
    private let bridge: HostBridge

    init(bridge: HostBridge) {
        self.bridge = bridge
    }

    func onCoreLog(marker: String, detail: String) {
        bridge.onCoreLog(marker: marker, detail: detail)
    }

    func workerDemandChanged(productId: String, transition: WorkerTransition) {
        bridge.workerDemandChanged(productId: productId, transition: transition)
    }

    func devicePaired(device: PairedSsoPeer) {
        bridge.devicePaired(device: device)
    }

    func navigateTo(url: String) async throws {
        try await withNavigationRejection {
            try await bridge.navigateTo(url: url)
        }
    }

    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32 {
        try await withHostRejection {
            try await bridge.pushNotification(request: request)
        }
    }

    func cancelNotification(id: UInt32) throws {
        try withHostRejection {
            try bridge.cancelNotification(id: id)
        }
    }

    func devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest
    ) async throws -> PermissionDecision {
        try await withHostRejection {
            try await bridge.devicePermission(
                product: product,
                request: request
            )
        }
    }

    func devicePermissionStatus(request: HostDevicePermissionRequest) async throws
        -> DevicePermissionStatus
    {
        try await withHostRejection {
            try await bridge.devicePermissionStatus(request: request)
        }
    }

    func remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission
    ) async throws -> PermissionDecision {
        try await withHostRejection {
            try await bridge.remotePermission(
                product: product,
                request: request
            )
        }
    }

    func authStateChanged(state: AuthState) {
        bridge.authStateChanged(state: state)
    }

    func coreStorageRead(key: Data) throws -> Data? {
        try withHostRejection {
            try bridge.coreStorage.read(key: key)
        }
    }

    func coreStorageWrite(key: Data, value: Data) throws {
        try withHostRejection {
            try bridge.coreStorage.write(key: key, value: value)
        }
    }

    func coreStorageClear(key: Data) throws {
        try withHostRejection {
            try bridge.coreStorage.clear(key: key)
        }
    }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        try withHostRejection {
            try bridge.chainConnect(genesisHash: genesisHash)
        }
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        try withHostRejection {
            try bridge.chainSend(connectionId: connectionId, request: request)
        }
    }

    func chainClose(connectionId: UInt32) throws {
        try withHostRejection {
            try bridge.chainClose(connectionId: connectionId)
        }
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        try await withHostRejection {
            try await bridge.confirmUserAction(review: review)
        }
    }

    func confirmPermission(review: UserConfirmationReview) async throws -> PermissionDecision {
        try await withHostRejection {
            try await bridge.confirmPermission(review: review)
        }
    }

    func lookupPreimage(key: Data) async throws -> Data? {
        try await withHostRejection {
            try await bridge.lookupPreimage(key: key)
        }
    }

    func currentTheme() throws -> HostThemeSubscribeItem {
        try withHostRejection {
            try bridge.currentTheme()
        }
    }

    func currentLocale() throws -> HostLocaleSubscribeItem {
        try withHostRejection {
            try bridge.currentLocale()
        }
    }

    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool {
        try await withHostRejection {
            try await bridge.featureSupported(request: request)
        }
    }

    func supportedChains() throws -> HostChainSet {
        try withHostRejection {
            try bridge.supportedChains()
        }
    }

    func localStorageRead(key: String) throws -> Data? {
        try withStorageError {
            try bridge.storage.read(key: key)
        }
    }

    func localStorageWrite(key: String, value: Data) throws {
        try withStorageError {
            try bridge.storage.write(key: key, value: value)
        }
    }

    func localStorageClear(key: String) throws {
        try withStorageError {
            try bridge.storage.clear(key: key)
        }
    }

    func beginOperation(productId: String, label: String) async throws -> UInt32 {
        try await withHostRejection {
            try await bridge.beginOperation(productId: productId, label: label)
        }
    }

    func endOperation(productId: String, id: UInt32) async throws {
        try await withHostRejection {
            try await bridge.endOperation(productId: productId, id: id)
        }
    }

    private func withHostRejection<T>(_ operation: () throws -> T) throws -> T {
        do {
            return try operation()
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }

    private func withHostRejection<T>(_ operation: () async throws -> T) async throws -> T {
        do {
            return try await operation()
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }

    private func withNavigationRejection<T>(_ operation: () throws -> T) throws -> T {
        do {
            return try operation()
        } catch let error as HostNavigateToError {
            throw error
        } catch {
            throw HostNavigateToError.Unknown(reason: hostRejectionReason(error))
        }
    }

    private func withNavigationRejection<T>(_ operation: () async throws -> T) async throws -> T {
        do {
            return try await operation()
        } catch let error as HostNavigateToError {
            throw error
        } catch {
            throw HostNavigateToError.Unknown(reason: hostRejectionReason(error))
        }
    }

    private func withStorageError<T>(_ operation: () throws -> T) throws -> T {
        do {
            return try operation()
        } catch let error as HostLocalStorageReadError {
            throw error
        } catch {
            throw HostLocalStorageReadError.Unknown(reason: hostRejectionReason(error))
        }
    }
}

/// Process-owned Rust host runtime. Product executables open independent
/// connections from this object and share its authentication and core services.
public final class TrUAPIHostRuntime: @unchecked Sendable {
    private let inner: NativeTrUApiHostRuntime
    private let callbackRetainer: HostCallbacks
    private let notificationCenter: NotificationCenter
    private let foregroundObserver: NSObjectProtocol
    private var contactsRetainer: NativeContactsCallbacks?

    public convenience init(bridge: HostBridge, runtimeConfig: HostRuntimeConfig) throws {
        try self.init(bridge: bridge, runtimeConfig: runtimeConfig, notificationCenter: .default)
    }

    init(
        bridge: HostBridge,
        runtimeConfig: HostRuntimeConfig,
        notificationCenter: NotificationCenter
    ) throws {
        let adapter = HostCallbackAdapter(bridge: bridge)
        callbackRetainer = adapter
        let inner = try NativeTrUApiHostRuntime.withRuntimeConfig(
            callbacks: adapter,
            runtimeConfig: runtimeConfig
        )
        self.inner = inner
        self.notificationCenter = notificationCenter
        // iOS reclaims a suspended app's listening sockets. Rebinding waits on
        // the Rust runtime's threads, so it runs off the main thread and at their QoS.
        foregroundObserver = notificationCenter.addObserver(
            forName: UIApplication.willEnterForegroundNotification,
            object: nil,
            queue: nil
        ) { _ in
            DispatchQueue.global().async(flags: .noQoS) {
                inner.relistenWsBridge()
            }
        }
    }

    deinit {
        notificationCenter.removeObserver(foregroundObserver)
    }

    /// Install the host's contacts adapter, which owns the contact list and
    /// draws the picker.
    ///
    /// Set-once, so the picker cannot change hands under a running product.
    /// Answers whether this call installed it. Call it before opening any
    /// product execution.
    @discardableResult
    public func setContacts(_ contacts: ContactsHostBridge) -> Bool {
        let adapter = ContactsCallbackAdapter(bridge: contacts)
        contactsRetainer = adapter
        return inner.setContactsCallbacks(callbacks: adapter)
    }

    /// Tell the core the host's contacts changed. Call it whenever a contact
    /// is removed or blocked, so a contact handle the core cached stops
    /// resolving.
    public func notifyContactsChanged() {
        inner.notifyContactsChanged()
    }

    /// Open one executable connection with a host-assigned immutable context.
    /// Pass `chat` to install the host's Chat adapter; hosts without the Chat
    /// modality omit it. Pass `pocket` to install the card collection, and
    /// omit that where the host has no Pocket surface. Pass `game` to hold
    /// game reminders, and omit it where the host cannot.
    public func openProductExecution(
        bridge: HostBridge,
        configuration: ProductExecutionConfig,
        chat: ChatHostBridge? = nil,
        pocket: PocketHostBridge? = nil,
        game: GameHostBridge? = nil
    ) throws -> TrUAPIProductExecution {
        let adapter = HostCallbackAdapter(bridge: bridge)
        let chatAdapter = chat.map { ChatCallbackAdapter(bridge: $0) }
        let pocketAdapter = pocket.map { PocketCallbackAdapter(bridge: $0) }
        let gameAdapter = game.map { GameCallbackAdapter(bridge: $0) }
        let execution = try inner.openProductExecution(
            callbacks: adapter,
            chatCallbacks: chatAdapter,
            pocketCallbacks: pocketAdapter,
            gameCallbacks: gameAdapter,
            executionConfig: configuration
        )
        return TrUAPIProductExecution(
            inner: execution,
            callbackRetainer: adapter,
            chatRetainer: chatAdapter,
            pocketRetainer: pocketAdapter,
            gameRetainer: gameAdapter
        )
    }

    public func disconnect() {
        inner.disconnect()
    }

    /// Take one reference on the product's worker for a modality holder that
    /// is on screen or in flight. The first one reports `.start` to
    /// ``HostBridge/workerDemandChanged(productId:transition:)``, which is
    /// where the host starts the worker. Pair every call with one
    /// ``releaseWorker(productId:)``.
    public func acquireWorker(productId: String) {
        inner.acquireWorker(productId: productId)
    }

    /// Release one reference. The last one reports `.stop`, after which the
    /// host may stop the worker. Releasing with none held is a no-op.
    public func releaseWorker(productId: String) {
        inner.releaseWorker(productId: productId)
    }

    /// Tell the pairing host behind `deeplink` that allowance allocation is
    /// under way, so it leaves its QR screen while the allocation runs.
    ///
    /// Answering needs this host's own statement-store allowance, so register
    /// the `WalletSso` renewal target first. The peer's own device statement
    /// account is the other target, read with ``parsePairingDeeplink(deeplink:)``
    /// and tracked before ``establishPairing(deeplink:)`` runs; the allocation
    /// this notice covers is what that call waits on. The returned handle is
    /// owed a ``notifyPairingFailed(announced:reason:)`` if pairing then
    /// fails: the peer has dropped its QR and waits without a deadline of its
    /// own, and the handle holds the responder secret until it is released.
    public func notifyPairingAllowanceAllocation(
        deeplink: String
    ) async throws -> NativeAnnouncedPairing {
        try await inner.notifyPairingAllowanceAllocation(deeplink: deeplink)
    }

    /// Tell a pairing host that already dropped its QR why pairing stopped.
    ///
    /// Takes the handle from
    /// ``notifyPairingAllowanceAllocation(deeplink:)``, so the notice is
    /// signed by the account that already reached that peer even if this
    /// host's signer has rotated since.
    public func notifyPairingFailed(
        announced: NativeAnnouncedPairing,
        reason: String
    ) async throws {
        try await inner.notifyPairingFailed(announced: announced, reason: reason)
    }

    /// Answer a pairing host's handshake deeplink, without serving the session
    /// it opens.
    ///
    /// The answer is signed by this host's own SSO statement identity, so the
    /// `.walletSso` renewal target has to be allocated for it to reach the
    /// Statement Store at all. The peer's device statement account is the
    /// other tracked target, since this host allocates the allowance the peer
    /// authors its own session statements under; read it from the deeplink
    /// with ``parsePairingDeeplink(deeplink:)``. A pairing that fails after
    /// that leaves the peer's target to untrack again, unless the device was
    /// already paired and the target still carries a live pairing.
    ///
    /// A device that pairs here reaches ``HostBridge/devicePaired(device:)``.
    /// Serving the session is ``resumePairing(peer:)``, called with the peer
    /// this host persisted.
    public func establishPairing(deeplink: String) async throws {
        try await inner.establishPairing(deeplink: deeplink)
    }

    /// Serve a paired host's SSO session until it ends.
    ///
    /// Runs for the life of the session, so give it its own task. Only
    /// `.peerDisconnected` authorises dropping the stored pairing; after
    /// `.subscriptionEnded` or a thrown error the peer is still paired and
    /// this can be called again.
    public func resumePairing(peer: PairedSsoPeer) async throws -> ResponderExit {
        try await inner.resumePairing(peer: peer)
    }

    /// Tell a paired host this signing host is ending their SSO session.
    ///
    /// Submits the disconnect notice and nothing else. The local side is the
    /// caller's: cancel that peer's ``resumePairing(peer:)`` task, which
    /// otherwise keeps answering a host this one no longer considers paired,
    /// and untrack its device statement account, which otherwise keeps being
    /// renewed every period. Dropping the stored pairing alone leaves both
    /// running.
    public func disconnectPairedHost(peer: PairedSsoPeer) async throws {
        try await inner.disconnectPairedHost(peer: peer)
    }

    /// Reports the core database's SQLite version, schema version and file
    /// path.
    public func coreDatabaseStatus() async throws -> DbStatus {
        try await inner.coreDatabaseStatus()
    }

    public func activateLocalSession(secret: Data, liteUsername: String? = nil) throws {
        try inner.activateLocalSession(secret: secret, liteUsername: liteUsername)
    }

    /// Answer one decrypted SSO remote message from the wallet-managed
    /// statement-store session. `message` is one SCALE-encoded
    /// `RemoteMessage` exactly as decrypted. `.response` carries the
    /// SCALE-encoded reply to post back over the same session;
    /// `.disconnected` means the peer ended the session (perform native
    /// teardown); `.ignored` means the message was not a request.
    /// Confirmation-gated requests await `confirmUserAction`, so this can
    /// take arbitrarily long — call from a `Task`, never the main thread.
    public func handleSsoRequest(message: Data) async throws -> SsoRequestOutcome {
        try await inner.handleSsoRequest(message: message)
    }

    /// Build the SCALE-encoded `Disconnected` message to post over a
    /// session the wallet is ending; record cleanup stays with the wallet.
    public func prepareDisconnectRequest() -> Data {
        inner.prepareDisconnectRequest()
    }

    public func notifyChainResponse(connectionId: UInt32, json: String) {
        inner.notifyChainResponse(connectionId: connectionId, json: json)
    }

    public func notifyChainClosed(connectionId: UInt32) {
        inner.notifyChainClosed(connectionId: connectionId)
    }

    /// Record the accounts renewal should keep allowed on the Statement Store.
    ///
    /// Needs an active session, so call it after
    /// ``activateLocalSession(secret:liteUsername:)`` or after pairing, not at
    /// construction.
    ///
    /// Recipe-shaped targets survive a change of root entropy; a raw
    /// ``StatementRenewalTarget/account(accountId:label:)`` does not, and is
    /// dropped by the next pass after ``activateLocalSession(secret:liteUsername:)``
    /// installs a different identity. Re-track those whenever the identity changes.
    public func trackStatementRenewalTargets(_ targets: [StatementRenewalTarget]) throws {
        try inner.trackStatementRenewalTargets(targets: targets)
    }

    /// The accounts the ledger tracks, in the order they were tracked.
    ///
    /// Needs no active session, so a `BGTaskScheduler` wake can read it on a
    /// cold start before deciding whether a pass is worth running.
    public func statementRenewalTargets() throws -> [TrackedStatementRenewalTarget] {
        try inner.statementRenewalTargets()
    }

    /// The root public key the active identity records its fixed entries under.
    ///
    /// Needs an active session. An entry from ``statementRenewalTargets()``
    /// whose owner is this key, or which has no owner, is one a pass will
    /// renew; any other is one it will prune.
    public func statementRenewalOwnerKey() throws -> Data {
        try inner.statementRenewalOwnerKey()
    }

    /// Stop renewing one fixed statement account, reporting whether the ledger
    /// held it.
    ///
    /// Scoped to the active identity, so it never removes an entry another
    /// identity promised.
    @discardableResult
    public func untrackStatementRenewalAccount(accountId: Data) throws -> Bool {
        try inner.untrackStatementRenewalAccount(accountId: accountId)
    }

    /// Run one renewal pass now, reporting what each tracked target got.
    ///
    /// Submits extrinsics and blocks until they are included, so call it off the
    /// main thread. There is no cancellation: a pass with several targets can
    /// outlast a short background budget, though a target registered before the
    /// process is killed is not lost and reads back as already allocated.
    public func renewStatementAllowances() throws -> StatementRenewalReport {
        try inner.renewStatementAllowances()
    }

    /// Start the in-process renewal loop, for a host that stays resident. A
    /// suspended app stops ticking, so prefer scheduling
    /// ``renewStatementAllowances()``.
    public func startStatementAllowanceRenewal() {
        inner.startStatementAllowanceRenewal()
    }

    /// The in-process loop's own cadence, capped at an hour. Allowances only
    /// stop being renewed at a period boundary and survive it by the chain's
    /// grace window, so a host scheduling one wake-up per period
    /// should read a value under an hour as the boundary approaching rather
    /// than waking hourly.
    public func nextStatementRenewalDelay() -> TimeInterval {
        inner.nextStatementRenewalDelay()
    }

    /// The most recent pass the in-process renewal loop ran.
    ///
    /// `nil` until a pass has run, which is "not yet" rather than healthy.
    /// ``startStatementAllowanceRenewal()`` returns nothing, so a host driving the
    /// loop reads its result here. `slotsExhausted` on the last pass means a
    /// period filled up and an allowance went unrenewed, which retrying cannot
    /// fix and a person may need telling about.
    public func lastStatementRenewalReport() -> StatementRenewalReport? {
        inner.lastStatementRenewalReport()
    }
}

/// Testable surface for one connection-scoped product execution.
public protocol TrUAPIProductExecutionProtocol: AnyObject, Sendable {
    func startWsBridge(bindPort: UInt16) throws -> WsBridgeEndpoint
    func stopWsBridge()
    func close()
    func publishChatAction(_ action: HostChatActionSubscribeItem) throws
    func render(_ request: ProductRendererRenderRequest) throws -> AsyncThrowingStream<RendererNode, Error>
    func publishRendererAction(_ item: HostRendererActionSubscribeItem) throws
    func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) async throws -> PermissionAuthorizationStatus
    func setPermissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws
    func notifyThemeChanged(theme: HostThemeSubscribeItem)
    func notifyLocaleChanged(locale: HostLocaleSubscribeItem)
    func notifyStorageChanged(key: String, value: Data?)
    func notifyPreimageChanged(key: Data, value: Data?)
    func notifyChainResponse(connectionId: UInt32, json: String)
    func notifyChainClosed(connectionId: UInt32)
    func notifyChatRoomsChanged(rooms: [ChatRoom])
    func sessionChatIdentityKey() throws -> Data?
    func notifyPocketCardsChanged(cards: [PocketCard])
}

/// One App, Widget, or Worker executable connected to a shared host runtime.
public final class TrUAPIProductExecution: TrUAPIProductExecutionProtocol, @unchecked Sendable {
    private let inner: NativeProductExecution
    private let callbackRetainer: HostCallbacks
    private let chatRetainer: NativeChatCallbacks?
    private let pocketRetainer: NativePocketCallbacks?
    private let gameRetainer: NativeGameCallbacks?

    fileprivate init(
        inner: NativeProductExecution,
        callbackRetainer: HostCallbacks,
        chatRetainer: NativeChatCallbacks?,
        pocketRetainer: NativePocketCallbacks?,
        gameRetainer: NativeGameCallbacks?
    ) {
        self.inner = inner
        self.callbackRetainer = callbackRetainer
        self.chatRetainer = chatRetainer
        self.pocketRetainer = pocketRetainer
        self.gameRetainer = gameRetainer
    }

    deinit {
        inner.shutdown()
    }

    public func startWsBridge(bindPort: UInt16 = 0) throws -> WsBridgeEndpoint {
        try inner.startWsBridge(bindPort: bindPort)
    }

    public func stopWsBridge() {
        inner.stopWsBridge()
    }

    public func close() {
        inner.shutdown()
    }

    public func publishChatAction(_ action: HostChatActionSubscribeItem) throws {
        try inner.publishChatAction(action: action)
    }

    public func render(
        _ request: ProductRendererRenderRequest
    ) throws -> AsyncThrowingStream<RendererNode, Error> {
        try rendererStream { observer in
            try inner.render(request: request, observer: observer)
        }
    }

    public func publishRendererAction(_ item: HostRendererActionSubscribeItem) throws {
        try inner.publishRendererAction(item: item)
    }

    public func notifyPocketCardsChanged(cards: [PocketCard]) {
        inner.notifyPocketCardsChanged(cards: cards)
    }

    public func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) async throws -> PermissionAuthorizationStatus {
        try await inner.permissionAuthorizationStatus(request: request)
    }

    /// Updates the product decision used by subsequent permission checks.
    public func setPermissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws {
        try inner.setPermissionAuthorizationStatus(request: request, status: status)
    }

    public func notifyThemeChanged(theme: HostThemeSubscribeItem) {
        inner.notifyThemeChanged(theme: theme)
    }

    public func notifyLocaleChanged(locale: HostLocaleSubscribeItem) {
        inner.notifyLocaleChanged(locale: locale)
    }

    /// Push a host storage change to active TrUAPI storage subscriptions,
    /// across every execution of the product; `nil` means cleared.
    ///
    /// Only for changes the host makes itself. A write a product made through
    /// TrUAPI already reaches its subscribers, so reporting one here delivers
    /// it twice.
    public func notifyStorageChanged(key: String, value: Data?) {
        inner.notifyStorageChanged(key: key, value: value)
    }

    public func notifyPreimageChanged(key: Data, value: Data?) {
        inner.notifyPreimageChanged(key: key, value: value)
    }

    public func notifyChainResponse(connectionId: UInt32, json: String) {
        inner.notifyChainResponse(connectionId: connectionId, json: json)
    }

    public func notifyChainClosed(connectionId: UInt32) {
        inner.notifyChainClosed(connectionId: connectionId)
    }

    public func sessionChatIdentityKey() throws -> Data? {
        try inner.sessionChatIdentityKey()
    }

    public func notifyChatRoomsChanged(rooms: [ChatRoom]) {
        inner.notifyChatRoomsChanged(rooms: rooms)
    }
}

/// Reason text for an error a host threw from a callback.
///
/// A value that is not a `LocalizedError` has no author-written description,
/// and `localizedDescription` renders it as "The operation couldn't be
/// completed. (Module.Type error 1.)" — which names the host's module and says
/// nothing about the failure. That string reaches the product, so prefer what
/// the host actually wrote.
private func hostRejectionReason(_ error: Error) -> String {
    let reason: String
    if let described = (error as? LocalizedError)?.errorDescription {
        reason = described
    } else if type(of: error) is NSError.Type {
        // Foundation writes these, and the text describes the failure rather
        // than the host's internals.
        reason = error.localizedDescription
    } else {
        // A plain value's `String(describing:)` prints its stored properties,
        // so only the type name crosses to the product.
        reason = String(describing: type(of: error))
    }
    return String(reason.prefix(hostRejectionReasonMaxCharacters))
}

/// Bounded: the reason reaches the product, and a host message can carry a
/// whole failed statement.
private let hostRejectionReasonMaxCharacters = 256

private func rendererStream(
    _ subscribe: (RendererStreamObserver) throws -> NativeRendererSubscription
) throws -> AsyncThrowingStream<RendererNode, Error> {
    let (stream, continuation) = AsyncThrowingStream.makeStream(
        of: RendererNode.self
    )
    let observer = RendererStreamObserver(continuation: continuation)
    let subscription = try subscribe(observer)
    continuation.onTermination = { @Sendable _ in
        subscription.cancel()
    }
    return stream
}

private final class RendererStreamObserver: NativeRendererObserver, @unchecked Sendable {
    private let continuation: AsyncThrowingStream<RendererNode, Error>.Continuation

    init(continuation: AsyncThrowingStream<RendererNode, Error>.Continuation) {
        self.continuation = continuation
    }

    func onUpdate(node: RendererNode) {
        continuation.yield(node)
    }

    func onComplete() {
        continuation.finish()
    }

    /// The product could not serve the render, so the last tree yielded is
    /// partial. Finishing with an error keeps that distinct from a clean end.
    func onError(reason: String) {
        continuation.finish(throwing: RendererStreamError(reason: reason))
    }
}

/// A render the product declined or could not encode.
public struct RendererStreamError: Error, CustomStringConvertible {
    /// Why the product ended the render.
    public let reason: String

    public var description: String { reason }
}
