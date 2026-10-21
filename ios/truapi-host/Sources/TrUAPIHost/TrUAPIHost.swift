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

    /// Show or hide the face above this execution's expanded card. Answers
    /// `.notPresented` when the product is not under its card and `.userMoving`
    /// while the user drags it, and returns without waiting for the animation.
    /// Defaults to `.unsupported`, so an app without cards says so instead of
    /// pretending it moved one.
    func setExpandedCardFaceShown(shown: Bool) async throws -> ExpandedCardFaceOutcome

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

    /// Whether the core has durable transactions still awaiting a verdict.
    /// While `pending` is true, keep a background task running that awaits
    /// ``TrUAPIHostRuntime/runDurableRecovery()``; false means nothing needs
    /// it. Reported on change, starting with the state the runtime finds at
    /// launch. Runtime-wide, like `workerDemandChanged`, and arrives on a
    /// core thread: hand it off rather than blocking. Defaults to a no-op
    /// for a host that runs no durable work.
    func durableWorkChanged(pending: Bool)

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
    ///
    /// `alt` is the product's one-line description of the message, already
    /// trimmed and screened, for places that list it without drawing it.
    func postMessage(roomId: String, content: ChatMessageContent, alt: String?) async throws -> String

    /// Set what a product's native Chat room shows below its messages, and
    /// keep it until the product sets another. A room without a footer shows
    /// the input with photo and video on.
    func setRoomFooter(roomId: String, footer: ChatRoomFooter) async throws

    /// Return the current product-scoped native Chat rooms.
    func listRooms() async throws -> [ChatRoom]

    /// Return the configuration the product last declared for a room, or
    /// ``ChatRoomConfig/hostDefault`` when it declared none. The core reads it
    /// before each feed update, posted message and query of the room.
    func roomConfig(roomId: String) async throws -> ChatRoomConfig

    /// Replace the configuration of a room whole, and keep it across
    /// restarts. A change of `productSource` discards the stored messages or
    /// the cached pages of the room.
    func setRoomConfig(roomId: String, config: ChatRoomConfig) async throws

    /// Replace the name, avatar and description of a room.
    func setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) async throws

    /// Replace how a room shows in the chat list.
    func setRoomPreview(roomId: String, preview: ChatRoomPreview) async throws

    /// Delete a room with its messages, cache and declarations, and close its
    /// screen. A room that does not exist is not an error.
    func deleteRoom(roomId: String) async throws

    /// Apply one change to the messages of a room. With `persist` the host
    /// stores the change. Without it the host applies the change only to an
    /// open room screen and to the chat list preview.
    ///
    /// Throw ``NativeChatFeedRejection`` to refuse the change. The product
    /// receives each case as its own error, and any other error as
    /// ``NativeChatFeedRejection/Rejected(reason:)``.
    func updateFeed(roomId: String, update: ChatFeedUpdate, persist: Bool) async throws

    /// Return the names of the Chat features that this host serves from end
    /// to end, from the feature table of the Chat Extension v2 additions RFC.
    /// The core removes each name that it does not know or cannot back. List
    /// a name only in the build that serves its feature.
    func capabilities() async throws -> [String]

    /// Return the optional features the product last declared for a room, or
    /// ``ChatRoomFeatures/hostDefault`` when it declared none.
    func roomFeatures(roomId: String) async throws -> ChatRoomFeatures

    /// Replace the optional features of a room whole, and keep them across
    /// restarts. Keep a feature that this host does not serve, so that a later
    /// build applies it.
    func setRoomFeatures(roomId: String, features: ChatRoomFeatures) async throws

    /// Show or change the call of a room, or remove it with `nil`. The core
    /// has already checked that this host lists the feature of the call.
    func setRoomCall(roomId: String, call: ChatRoomCall?) async throws

    /// Return the state of each room of the product that the user muted or
    /// archived.
    func roomUserStates() async throws -> [ChatRoomUserState]

    /// Return the bot ids of the personas that the product registered.
    func bots() async throws -> [String]
}

/// Defaults for a host that does not serve the Chat Extension v2 requests yet.
/// The product receives a rejection for each request, except for the
/// declarations that a new room always carries: the host keeps the name and
/// icon of `createRoom`, and the configuration that the host serves anyway.
public extension ChatHostBridge {
    func roomConfig(roomId _: String) async throws -> ChatRoomConfig {
        .hostDefault
    }

    func setRoomConfig(roomId _: String, config: ChatRoomConfig) async throws {
        guard config == .hostDefault else { throw unservedChatRequest("setRoomConfig") }
    }

    func setRoomMetadata(roomId _: String, metadata _: ChatRoomMetadata) async throws {}

    func setRoomPreview(roomId _: String, preview _: ChatRoomPreview) async throws {
        throw unservedChatRequest("setRoomPreview")
    }

    func deleteRoom(roomId _: String) async throws {
        throw unservedChatRequest("deleteRoom")
    }

    func updateFeed(roomId _: String, update _: ChatFeedUpdate, persist _: Bool) async throws {
        throw NativeChatFeedRejection.Rejected(reason: unservedChatRequestReason("updateFeed"))
    }

    func capabilities() async throws -> [String] {
        []
    }

    func roomFeatures(roomId _: String) async throws -> ChatRoomFeatures {
        .hostDefault
    }

    func setRoomFeatures(roomId _: String, features _: ChatRoomFeatures) async throws {
        throw unservedChatRequest("setRoomFeatures")
    }

    func setRoomCall(roomId _: String, call _: ChatRoomCall?) async throws {
        throw unservedChatRequest("setRoomCall")
    }

    func roomUserStates() async throws -> [ChatRoomUserState] {
        []
    }

    func bots() async throws -> [String] {
        []
    }
}

/// The resource type of Chat Extension v2, under a name that a host app with a
/// `ChatResource` type of its own can still write.
public typealias TrUAPIChatResource = ChatResource

public extension ChatRoomFeatures {
    /// The features of a room that declares none: the room shows what a
    /// room of Chat Extension v2 shows.
    static var hostDefault: ChatRoomFeatures {
        ChatRoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            readAloud: false,
            sealedAttachments: false,
            hopAttachments: false
        )
    }
}

public extension ChatRoomConfig {
    /// The configuration of a room that declares nothing: the host stores the
    /// messages and serves every part of the room itself.
    static var hostDefault: ChatRoomConfig {
        ChatRoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
    }
}

private func unservedChatRequestReason(_ request: String) -> String {
    "this host does not serve chat \(request)"
}

private func unservedChatRequest(_ request: String) -> HostRejection {
    HostRejection.Rejected(reason: unservedChatRequestReason(request))
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

/// Host-implemented top-up engine: claims a source's funds into the user's
/// balance.
///
/// Installed once on the runtime with ``TrUAPIHostRuntime/setTopUp(_:)``.
/// The core has already checked the session and the source keys; a
/// `productAccount` source arrives as its derivation index for the host to
/// derive. The host owns claiming, retries, partial claims, persistence and
/// scoping ids to the product. A runtime without one answers `topUp` and
/// `topUpStatusSubscribe` with `Unsupported`.
public protocol TopUpHostBridge: AnyObject, Sendable {
    /// Start `request` for `productId`, returning once the host has accepted
    /// it. Throw a `HostPaymentTopUpError` to answer with its code; anything
    /// else reaches the product as `Unknown`.
    func topUp(productId: String, request: HostPaymentTopUpRequest) async throws

    /// The current status of `productId`'s top-up `id`, or `nil` when the
    /// host holds none. Report every later status with
    /// ``TrUAPIHostRuntime/notifyTopUpStatus(productId:id:status:)``.
    func topUpStatus(productId: String, id: Data) throws -> HostPaymentTopUpStatusSubscribeItem?
}

/// Host-implemented payment engine: pays from the user's balance to an
/// account once the user approves.
///
/// Installed once on the runtime with ``TrUAPIHostRuntime/setPayments(_:)``.
/// Answer `InsufficientBalance` whenever the balance is short: the core turns
/// it into `Rejected` for a product without balance access. A runtime without
/// one answers `request` and `statusSubscribe` with `Unsupported`.
public protocol PaymentHostBridge: AnyObject, Sendable {
    /// Ask the user to approve `request` for `productId`, returning once the
    /// user authorized it and the host took it on. Throw a `HostPaymentError`
    /// to answer with its code; anything else reaches the product as
    /// `Unknown`.
    func requestPayment(productId: String, request: HostPaymentRequest) async throws

    /// The current status of `productId`'s payment `id`, or `nil` when the
    /// host holds none. Report every later status with
    /// ``TrUAPIHostRuntime/notifyPaymentStatus(productId:id:status:)``.
    func paymentStatus(productId: String, id: Data) throws -> HostPaymentStatusSubscribeItem?
}

/// Host-implemented balance view: what a payment request can spend right now.
///
/// Installed once on the runtime with ``TrUAPIHostRuntime/setBalance(_:)``.
/// The core checks the product's balance access itself, so never prompt for
/// it here. A runtime without one answers `balanceSubscribe` with
/// `Unsupported`.
public protocol BalanceHostBridge: AnyObject, Sendable {
    /// The current balance of `purse` (`nil` for the main purse), a decimal
    /// string of CASH units. The balance is the user's, the same for every
    /// product; `productId` only names who is asking. Report every change with
    /// ``TrUAPIHostRuntime/notifyBalance(purse:available:)``.
    func balance(productId: String, purse: UInt32?) async throws -> U128
}

/// Draws the viewfinder for `scanner.scan`, following the rules on the core's
/// `ScannerPlatform`. Closes it when the task is cancelled.
public protocol ScannerHostBridge: AnyObject, Sendable {
    func scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest
    ) async throws -> HostScan
}

/// The username directory of the host, following the rules on the core's
/// `UsernamesPlatform`. Closes an open question when the task is cancelled.
public protocol UsernamesHostBridge: AnyObject, Sendable {
    func searchUsernames(productId: String, prefix: String) async throws -> [String]
}

/// The person chat services of the host, following the rules on the core's
/// `PersonChatPlatform`: the `wss://` URLs of the HOP nodes of the network's
/// Bulletin chain, the devices paired with the wallet, and the chat's contacts
/// for the rest of the app.
public protocol PersonChatHostBridge: AnyObject, Sendable {
    func hopNodes() async throws -> [String]

    /// The devices paired with this wallet device over SSO, as the chat announces them to contacts.
    func pairedDevices() async throws -> [HostPairedDevice]

    /// Replace the chat contacts of `productId`, which the app shows outside the chat.
    func setContacts(productId: String, contacts: [HostChatContact]) async throws

    /// At most `maxLen` bytes from `offset` of the native chat history staged
    /// for `productId`, and the size of the whole export; a total of 0 when
    /// none is staged.
    func historyExport(productId: String, offset: UInt64, maxLen: UInt32) async throws -> HostHistoryExportChunk

    /// `productId` imported the staged history: drop it.
    func historyImported(productId: String) async throws
}

/// The push service of the host for `push.address` and `push.notify`. The
/// host keeps the push id and the key of each route where its notification
/// service extension reads them, and sends each delivery to its push relay
/// with its own credentials. Only the core calls it.
public protocol PushHostBridge: AnyObject, Sendable {
    /// Give `delivery` to the push relay. Answers whether the push service
    /// took it.
    func sendPush(productId: String, delivery: HostPushDelivery) async throws -> Bool

    /// The push tokens of this device now, which the chat product shares with
    /// its contacts.
    func pushTokens() async throws -> HostPushTokens

    /// Replace the chat push routes of `productId` with `routes`, where the
    /// notification service extension reads them.
    func setChatPushRoutes(productId: String, routes: [HostChatPushRoute]) async throws
}

/// The call media of the host for `callMedia.start`, `callMedia.signal` and
/// `callMedia.stop`: the host runs the audio and the video of the call of a
/// room, gives each of its media signals to the product as the room event
/// `CallSignal`, and sends `CallEnded` when the user ends the call on its call
/// screen. Only the core calls it.
public protocol CallMediaHostBridge: AnyObject, Sendable {
    func startCallMedia(productId: String, roomId: String, callId: String, video: Bool, offerer: Bool) async throws

    func callMediaSignal(productId: String, roomId: String, callId: String, signal: Data) async throws

    func stopCallMedia(productId: String, roomId: String, callId: String) async throws
}

/// The push relay client of the host. The core gives it the wake topics of
/// every product and the key of the device; the host adds what identifies the
/// device to its relay (the push token and the app id) and sends the
/// registration to its relay endpoint. Only the core calls it. An empty topic
/// list removes the registration.
public protocol WakeRelayHostBridge: AnyObject, Sendable {
    func registerWakeTopics(registration: WakeRelayRegistration) async throws
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
    func durableWorkChanged(pending: Bool) {}
    func devicePermissionStatus(request: HostDevicePermissionRequest) async throws
        -> DevicePermissionStatus { .notApplicable }
    func setExpandedCardFaceShown(shown: Bool) async throws -> ExpandedCardFaceOutcome {
        .unsupported
    }
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

    func postMessage(roomId: String, content: ChatMessageContent, alt: String?) async throws -> String {
        try await withHostRejection {
            try await bridge.postMessage(roomId: roomId, content: content, alt: alt)
        }
    }

    func setRoomFooter(roomId: String, footer: ChatRoomFooter) async throws {
        try await withHostRejection {
            try await bridge.setRoomFooter(roomId: roomId, footer: footer)
        }
    }

    func listRooms() async throws -> [ChatRoom] {
        try await withHostRejection { try await bridge.listRooms() }
    }

    func roomConfig(roomId: String) async throws -> ChatRoomConfig {
        try await withHostRejection { try await bridge.roomConfig(roomId: roomId) }
    }

    func setRoomConfig(roomId: String, config: ChatRoomConfig) async throws {
        try await withHostRejection {
            try await bridge.setRoomConfig(roomId: roomId, config: config)
        }
    }

    func setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) async throws {
        try await withHostRejection {
            try await bridge.setRoomMetadata(roomId: roomId, metadata: metadata)
        }
    }

    func setRoomPreview(roomId: String, preview: ChatRoomPreview) async throws {
        try await withHostRejection {
            try await bridge.setRoomPreview(roomId: roomId, preview: preview)
        }
    }

    func deleteRoom(roomId: String) async throws {
        try await withHostRejection { try await bridge.deleteRoom(roomId: roomId) }
    }

    func updateFeed(roomId: String, update: ChatFeedUpdate, persist: Bool) async throws {
        try await withFeedRejection {
            try await bridge.updateFeed(roomId: roomId, update: update, persist: persist)
        }
    }

    func capabilities() async throws -> [String] {
        try await withHostRejection { try await bridge.capabilities() }
    }

    func roomFeatures(roomId: String) async throws -> ChatRoomFeatures {
        try await withHostRejection { try await bridge.roomFeatures(roomId: roomId) }
    }

    func setRoomFeatures(roomId: String, features: ChatRoomFeatures) async throws {
        try await withHostRejection {
            try await bridge.setRoomFeatures(roomId: roomId, features: features)
        }
    }

    func setRoomCall(roomId: String, call: ChatRoomCall?) async throws {
        try await withHostRejection { try await bridge.setRoomCall(roomId: roomId, call: call) }
    }

    func roomUserStates() async throws -> [ChatRoomUserState] {
        try await withHostRejection { try await bridge.roomUserStates() }
    }

    func bots() async throws -> [String] {
        try await withHostRejection { try await bridge.bots() }
    }

    /// Pass a ``NativeChatFeedRejection`` to the product as it is, and any
    /// other error as ``NativeChatFeedRejection/Rejected(reason:)``.
    private func withFeedRejection(_ operation: () async throws -> Void) async throws {
        do {
            try await operation()
        } catch let rejection as NativeChatFeedRejection {
            throw rejection
        } catch {
            throw NativeChatFeedRejection.Rejected(reason: hostRejectionReason(error))
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
}

/// Adapter that bridges the public `WakeRelayHostBridge` to the generated
/// UniFFI `NativeWakeRelayCallbacks` protocol.
private final class WakeRelayCallbackAdapter: NativeWakeRelayCallbacks, @unchecked Sendable {
    private let bridge: WakeRelayHostBridge

    init(bridge: WakeRelayHostBridge) {
        self.bridge = bridge
    }

    func registerWakeTopics(registration: WakeRelayRegistration) async throws {
        try await withHostRejection { try await bridge.registerWakeTopics(registration: registration) }
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
        try withHostRejection { try bridge.contacts(lookup: lookup) }
    }

    func pickContact(productId: String) async throws -> HostContactPick {
        try await withHostRejection { try await bridge.pickContact(productId: productId) }
    }
}

/// Adapter that bridges the public `ScannerHostBridge` to the generated UniFFI
/// `NativeScannerCallbacks` protocol.
private final class ScannerCallbackAdapter: NativeScannerCallbacks, @unchecked Sendable {
    private let bridge: ScannerHostBridge

    init(bridge: ScannerHostBridge) {
        self.bridge = bridge
    }

    func scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest
    ) async throws -> HostScan {
        try await withHostRejection {
            try await bridge.scanCode(productId: productId, executionKind: executionKind, request: request)
        }
    }
}

/// Adapter that bridges the public `TopUpHostBridge` to the generated UniFFI
/// `NativeTopUpCallbacks` protocol.
private final class TopUpCallbackAdapter: NativeTopUpCallbacks, @unchecked Sendable {
    private let bridge: TopUpHostBridge

    init(bridge: TopUpHostBridge) {
        self.bridge = bridge
    }

    func topUp(productId: String, request: HostPaymentTopUpRequest) async throws {
        do {
            try await bridge.topUp(productId: productId, request: request)
        } catch let error as HostPaymentTopUpError {
            throw error
        } catch {
            throw HostPaymentTopUpError.Unknown(reason: hostRejectionReason(error))
        }
    }

    func topUpStatus(productId: String, id: Bytes32) throws -> HostPaymentTopUpStatusSubscribeItem? {
        do {
            return try bridge.topUpStatus(productId: productId, id: id)
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `PaymentHostBridge` to the generated UniFFI
/// `NativePaymentCallbacks` protocol.
private final class PaymentCallbackAdapter: NativePaymentCallbacks, @unchecked Sendable {
    private let bridge: PaymentHostBridge

    init(bridge: PaymentHostBridge) {
        self.bridge = bridge
    }

    func requestPayment(productId: String, request: HostPaymentRequest) async throws {
        do {
            try await bridge.requestPayment(productId: productId, request: request)
        } catch let error as HostPaymentError {
            throw error
        } catch {
            throw HostPaymentError.Unknown(reason: hostRejectionReason(error))
        }
    }

    func paymentStatus(productId: String, id: Bytes32) throws -> HostPaymentStatusSubscribeItem? {
        do {
            return try bridge.paymentStatus(productId: productId, id: id)
        } catch let error as HostRejection {
            throw error
        } catch {
            throw HostRejection.Rejected(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `BalanceHostBridge` to the generated UniFFI
/// `NativeBalanceCallbacks` protocol.
private final class BalanceCallbackAdapter: NativeBalanceCallbacks, @unchecked Sendable {
    private let bridge: BalanceHostBridge

    init(bridge: BalanceHostBridge) {
        self.bridge = bridge
    }

    func balance(productId: String, purse: UInt32?) async throws -> U128 {
        do {
            return try await bridge.balance(productId: productId, purse: purse)
        } catch let error as HostPaymentBalanceSubscribeError {
            throw error
        } catch {
            throw HostPaymentBalanceSubscribeError.Unknown(reason: hostRejectionReason(error))
        }
    }
}

/// Adapter that bridges the public `UsernamesHostBridge` to the generated UniFFI
/// `NativeUsernamesCallbacks` protocol.
private final class UsernamesCallbackAdapter: NativeUsernamesCallbacks, @unchecked Sendable {
    private let bridge: UsernamesHostBridge

    init(bridge: UsernamesHostBridge) {
        self.bridge = bridge
    }

    func searchUsernames(productId: String, prefix: String) async throws -> [String] {
        try await withHostRejection {
            try await bridge.searchUsernames(productId: productId, prefix: prefix)
        }
    }

}

/// Adapter that bridges the public `PersonChatHostBridge` to the generated
/// UniFFI `NativePersonChatCallbacks` protocol.
private final class PersonChatCallbackAdapter: NativePersonChatCallbacks, @unchecked Sendable {
    private let bridge: PersonChatHostBridge

    init(bridge: PersonChatHostBridge) {
        self.bridge = bridge
    }

    func hopNodes() async throws -> [String] {
        try await withHostRejection {
            try await bridge.hopNodes()
        }
    }

    func pairedDevices() async throws -> [HostPairedDevice] {
        try await withHostRejection {
            try await bridge.pairedDevices()
        }
    }

    func setContacts(productId: String, contacts: [HostChatContact]) async throws {
        try await withHostRejection {
            try await bridge.setContacts(productId: productId, contacts: contacts)
        }
    }

    func historyExport(productId: String, offset: UInt64, maxLen: UInt32) async throws -> HostHistoryExportChunk {
        try await withHostRejection {
            try await bridge.historyExport(productId: productId, offset: offset, maxLen: maxLen)
        }
    }

    func historyImported(productId: String) async throws {
        try await withHostRejection {
            try await bridge.historyImported(productId: productId)
        }
    }
}

/// Adapter that bridges the public `CallMediaHostBridge` to the generated
/// UniFFI `NativeCallMediaCallbacks` protocol.
private final class CallMediaCallbackAdapter: NativeCallMediaCallbacks, @unchecked Sendable {
    private let bridge: CallMediaHostBridge

    init(bridge: CallMediaHostBridge) {
        self.bridge = bridge
    }

    func startCallMedia(productId: String, roomId: String, callId: String, video: Bool, offerer: Bool) async throws {
        try await withHostRejection {
            try await bridge.startCallMedia(
                productId: productId,
                roomId: roomId,
                callId: callId,
                video: video,
                offerer: offerer
            )
        }
    }

    func callMediaSignal(productId: String, roomId: String, callId: String, signal: Data) async throws {
        try await withHostRejection {
            try await bridge.callMediaSignal(productId: productId, roomId: roomId, callId: callId, signal: signal)
        }
    }

    func stopCallMedia(productId: String, roomId: String, callId: String) async throws {
        try await withHostRejection {
            try await bridge.stopCallMedia(productId: productId, roomId: roomId, callId: callId)
        }
    }
}

/// Adapter that bridges the public `PushHostBridge` to the generated UniFFI
/// `NativePushCallbacks` protocol.
private final class PushCallbackAdapter: NativePushCallbacks, @unchecked Sendable {
    private let bridge: PushHostBridge

    init(bridge: PushHostBridge) {
        self.bridge = bridge
    }

    func sendPush(productId: String, delivery: HostPushDelivery) async throws -> Bool {
        try await withHostRejection {
            try await bridge.sendPush(productId: productId, delivery: delivery)
        }
    }

    func pushTokens() async throws -> HostPushTokens {
        try await withHostRejection {
            try await bridge.pushTokens()
        }
    }

    func setChatPushRoutes(productId: String, routes: [HostChatPushRoute]) async throws {
        try await withHostRejection {
            try await bridge.setChatPushRoutes(productId: productId, routes: routes)
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

    func durableWorkChanged(pending: Bool) {
        bridge.durableWorkChanged(pending: pending)
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

    func setExpandedCardFaceShown(shown: Bool) async throws -> ExpandedCardFaceOutcome {
        try await withHostRejection {
            try await bridge.setExpandedCardFaceShown(shown: shown)
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
    private var topUpRetainer: NativeTopUpCallbacks?
    private var paymentsRetainer: NativePaymentCallbacks?
    private var balanceRetainer: NativeBalanceCallbacks?
    private var scannerRetainer: NativeScannerCallbacks?
    private var usernamesRetainer: NativeUsernamesCallbacks?
    private var personChatRetainer: NativePersonChatCallbacks?
    private var pushRetainer: NativePushCallbacks?
    private var callMediaRetainer: NativeCallMediaCallbacks?
    private let wakeRelayLock = NSLock()
    private var wakeRelayAdapter: NativeWakeRelayCallbacks?

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

    /// Install the host's scanner before opening any product execution.
    /// Set-once: answers whether this call installed it.
    @discardableResult
    public func setScanner(_ scanner: ScannerHostBridge) -> Bool {
        let adapter = ScannerCallbackAdapter(bridge: scanner)
        scannerRetainer = adapter
        return inner.setScannerCallbacks(callbacks: adapter)
    }

    /// Install the host's username directory before opening any product
    /// execution. Set-once: answers whether this call installed it.
    @discardableResult
    public func setUsernames(_ usernames: UsernamesHostBridge) -> Bool {
        let adapter = UsernamesCallbackAdapter(bridge: usernames)
        usernamesRetainer = adapter
        return inner.setUsernamesCallbacks(callbacks: adapter)
    }

    /// Install the host's person chat services (the HOP nodes) before opening
    /// any product execution. Set-once: answers whether this call installed it.
    @discardableResult
    public func setPersonChat(_ personChat: PersonChatHostBridge) -> Bool {
        let adapter = PersonChatCallbackAdapter(bridge: personChat)
        personChatRetainer = adapter
        return inner.setPersonChatCallbacks(callbacks: adapter)
    }

    /// Install the host's push service before opening any product execution.
    /// Set-once: answers whether this call installed it.
    @discardableResult
    public func setPush(_ push: PushHostBridge) -> Bool {
        let adapter = PushCallbackAdapter(bridge: push)
        pushRetainer = adapter
        return inner.setPushCallbacks(callbacks: adapter)
    }

    /// Install the host's call media before opening any product execution.
    /// Set-once: answers whether this call installed it.
    @discardableResult
    public func setCallMedia(_ callMedia: CallMediaHostBridge) -> Bool {
        let adapter = CallMediaCallbackAdapter(bridge: callMedia)
        callMediaRetainer = adapter
        return inner.setCallMediaCallbacks(callbacks: adapter)
    }

    /// Install the host's push relay client, or remove it with `nil`. The core
    /// gives it the stored wake topics of every product at once, and again at
    /// each change of the watches. Throws when the relay refused the stored
    /// topics; the relay stays installed.
    public func setWakeRelay(_ relay: WakeRelayHostBridge?) async throws {
        let adapter = relay.map { WakeRelayCallbackAdapter(bridge: $0) }
        wakeRelayLock.withLock { wakeRelayAdapter = adapter }
        try await inner.setWakeRelay(relay: adapter)
    }

    /// Check a wake push from the push relay and wake each product that
    /// watches its topic for 30 seconds, as a notification wake. `payload` is
    /// the JSON text of the `truapi-wake` value of the push. Throws
    /// ``PushWakeRejection`` for a push the core does not trust; the core
    /// wakes nothing then. Keep the app running until the end of each hold.
    public func handleWakePush(payload: String) async throws -> [PushWakeDelivery] {
        try await inner.handleWakePush(payload: payload)
    }

    /// Keep `secret`, the X25519 key the native chat of this install used as
    /// its device key, so the chat product goes on using it and contacts keep
    /// reaching this device. Call it once, when the app moves its native chat
    /// to the chat product, before the product first runs. Answers false when
    /// the core already keeps a different key, which it never replaces.
    public func importChatDeviceKey(_ secret: Data) async throws -> Bool {
        try await inner.importChatDeviceKey(secret: secret)
    }

    /// Tell the core the host's contacts changed. Call it whenever a contact
    /// is removed or blocked, so a contact handle the core cached stops
    /// resolving.
    public func notifyContactsChanged() {
        inner.notifyContactsChanged()
    }

    /// Install the host's top-up engine. Set-once; answers whether this call
    /// installed it. Call it before opening any product execution.
    @discardableResult
    public func setTopUp(_ topUp: TopUpHostBridge) -> Bool {
        let adapter = TopUpCallbackAdapter(bridge: topUp)
        guard inner.setTopUpCallbacks(callbacks: adapter) else { return false }
        topUpRetainer = adapter
        return true
    }

    /// Report a later status of `productId`'s top-up `id` to the products
    /// following it. A terminal status ends their streams.
    public func notifyTopUpStatus(productId: String, id: Data, status: HostPaymentTopUpStatusSubscribeItem) throws {
        try inner.notifyTopUpStatus(productId: productId, id: id, status: status)
    }

    /// Install the host's payment engine. Set-once; answers whether this call
    /// installed it. Call it before opening any product execution.
    @discardableResult
    public func setPayments(_ payments: PaymentHostBridge) -> Bool {
        let adapter = PaymentCallbackAdapter(bridge: payments)
        guard inner.setPaymentCallbacks(callbacks: adapter) else { return false }
        paymentsRetainer = adapter
        return true
    }

    /// Report a later status of `productId`'s payment `id` to the products
    /// following it. A terminal status ends their streams.
    public func notifyPaymentStatus(productId: String, id: Data, status: HostPaymentStatusSubscribeItem) throws {
        try inner.notifyPaymentStatus(productId: productId, id: id, status: status)
    }

    /// Install the host's balance view. Set-once; answers whether this call
    /// installed it. Call it before opening any product execution.
    @discardableResult
    public func setBalance(_ balance: BalanceHostBridge) -> Bool {
        let adapter = BalanceCallbackAdapter(bridge: balance)
        guard inner.setBalanceCallbacks(callbacks: adapter) else { return false }
        balanceRetainer = adapter
        return true
    }

    /// Report the new balance of `purse` (`nil` for the main purse), a decimal
    /// string of CASH units, to the products following it.
    public func notifyBalance(purse: UInt32?, available: U128) {
        inner.notifyBalance(purse: purse, available: available)
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

    /// Tell the core whether the app is in the foreground. While it is, the
    /// core holds the worker of each product that declares
    /// `WorkerKeepAlive.foreground`. The core starts in the background, so
    /// report the state once the runtime exists and at each change.
    public func setAppForeground(foreground: Bool) {
        inner.setAppForeground(foreground: foreground)
    }

    /// Tell the core when the worker of a product runs, from the `keepAlive`
    /// field of its Worker executable manifest. Set `.onDemand` again when
    /// the product goes.
    public func setWorkerKeepAlive(productId: String, keepAlive: WorkerKeepAlive) {
        inner.setWorkerKeepAlive(productId: productId, keepAlive: keepAlive)
    }

    /// Send one event about a room to the product: the user muted or
    /// archived the room, pressed a call button, answered a call, or selected
    /// another alternative of a message. The core keeps the newest events of
    /// a product until its worker subscribes, so the worker does not have to
    /// run.
    public func publishChatRoomEvent(productId: String, roomId: String, event: ChatRoomEvent) throws {
        try inner.publishChatRoomEvent(productId: productId, roomId: roomId, event: event)
    }

    /// Hold the worker of the product for a wake of 30 seconds. A new wake
    /// moves the end of the hold. A notification wake also sends
    /// ``ChatRoomEvent/woken(watchId:)`` to the room, at most 12 times in one
    /// hour; after that the core answers ``ChatWakeOutcome/budgetSpent``.
    @discardableResult
    public func wakeProduct(productId: String, reason: ChatWakeReason) throws -> ChatWakeOutcome {
        try inner.wakeProduct(productId: productId, reason: reason)
    }

    /// Stage a user attachment of a room as a preimage that the Bulletin
    /// allowance of the product pays for, and return its key. The core
    /// throws ``NativeChatStageRejection``: `notDeclared` for a room whose
    /// features do not declare `preimage_attachments`, `tooLarge` above
    /// 8 MiB or above the Bulletin chain's per-transaction bound,
    /// `permissionDenied` when the user refused `PreimageSubmit` for the
    /// product (the core prompts through the product's open Worker
    /// execution when the user did not decide yet), `allowanceUnavailable`
    /// and `submitFailed`. Staging needs no worker reference.
    public func stageChatAttachment(
        productId: String,
        roomId: String,
        mimeType: String,
        bytes: Data
    ) async throws -> Data {
        try await inner.chatStageAttachment(
            productId: productId,
            roomId: roomId,
            mimeType: mimeType,
            bytes: bytes
        )
    }

    /// Seal a user attachment of a room: the core encrypts it with a new key,
    /// stores the ciphertext as Bulletin preimages in parts that the
    /// allowance of the product pays for, and returns the
    /// ``ChatResource/sealed(parts:key:nonce:size:digest:)`` that names them.
    /// The core throws ``NativeChatStageRejection``: `notDeclared` for a room
    /// whose features do not declare `sealed_attachments`, `tooLarge` above
    /// 32 MiB, `permissionDenied`, `allowanceUnavailable` and `submitFailed`,
    /// as ``stageChatAttachment(productId:roomId:mimeType:bytes:)`` does.
    public func sealChatAttachment(productId: String, roomId: String, bytes: Data) async throws -> ChatResource {
        try await inner.chatSealAttachment(productId: productId, roomId: roomId, bytes: bytes)
    }

    /// Upload a user file of a room of the chat product that declares
    /// `hop_attachments` to a HOP node, up to 128 MiB, and return the
    /// ``ChatResource/hop(identifier:ticket:node:size:)`` to send to the
    /// product. Throws ``NativeChatStageRejection`` as
    /// ``sealChatAttachment(productId:roomId:bytes:)`` does.
    public func hopChatAttachment(productId: String, roomId: String, bytes: Data) async throws -> ChatResource {
        try await inner.chatHopAttachment(productId: productId, roomId: roomId, bytes: bytes)
    }

    /// The bytes of a ``ChatResource/hop(identifier:ticket:node:size:)`` of a
    /// room of the chat product, which the core claims from the node it names
    /// when ``PersonChatHostBridge`` lists that node, or from Bulletin once the
    /// node promoted it. Keep the bytes, then call
    /// ``ackHopChatResource(productId:resource:entries:)`` with the entries.
    public func openHopChatResource(productId: String, resource: ChatResource) async throws -> NativeHopFile {
        try await inner.openHopChatResource(productId: productId, resource: resource)
    }

    /// Tell the node that the entries ``openHopChatResource(productId:resource:)``
    /// gave were taken, once the bytes are kept.
    public func ackHopChatResource(productId: String, resource: ChatResource, entries: [Data]) async throws {
        try await inner.ackHopChatResource(productId: productId, resource: resource, entries: entries)
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

    /// Decides durable transactions from the chain until none is live, then
    /// returns. Await it from the background task
    /// ``HostBridge/durableWorkChanged(pending:)`` asks for; a throw means
    /// recovery stopped early and the task should be retried. Cancelling the
    /// Swift task does not stop the run in the core, so a background task
    /// completes itself from its expiration handler rather than waiting for
    /// this call to return.
    public func runDurableRecovery() async throws {
        try await inner.runDurableRecovery()
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
    func publishChatEvent(roomId: String, peer: String, event: ChatEvent) throws
    func chatMessagesQuery(
        _ request: ProductChatMessagesQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatMessagesQueryItem
    func chatUserAction(
        _ request: ProductChatUserActionRequest,
        timeout: Duration
    ) async throws -> ProductChatUserActionItem
    func chatReactionsQuery(
        _ request: ProductChatReactionsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatReactionsQueryItem
    func chatParticipantsQuery(
        _ request: ProductChatParticipantsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatParticipantsQueryItem
    func chatMessageActionsQuery(
        _ request: ProductChatMessageActionsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatMessageActionsQueryItem
    func chatSearchQuery(
        _ request: ProductChatSearchQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatSearchQueryItem
    func chatOpenLink(
        _ request: ProductChatOpenLinkRequest,
        timeout: Duration
    ) async throws -> ProductChatOpenLinkItem
    func render(_ request: ProductRendererRenderRequest) throws -> AsyncThrowingStream<RendererNode, Error>
    func publishRendererAction(_ item: HostRendererActionSubscribeItem) throws
    func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) async throws -> PermissionAuthorizationStatus
    func authorizeDevicePermission(_ request: HostDevicePermissionRequest) async throws -> Bool
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

/// The chat queries with the timeouts of ``ChatQueryTimeout``.
public extension TrUAPIProductExecutionProtocol {
    func chatMessagesQuery(
        _ request: ProductChatMessagesQueryRequest
    ) async throws -> ProductChatMessagesQueryItem {
        try await chatMessagesQuery(request, timeout: ChatQueryTimeout.messages)
    }

    func chatUserAction(
        _ request: ProductChatUserActionRequest
    ) async throws -> ProductChatUserActionItem {
        try await chatUserAction(request, timeout: ChatQueryTimeout.timeout(for: request.action))
    }

    func chatReactionsQuery(
        _ request: ProductChatReactionsQueryRequest
    ) async throws -> ProductChatReactionsQueryItem {
        try await chatReactionsQuery(request, timeout: ChatQueryTimeout.lookup)
    }

    func chatParticipantsQuery(
        _ request: ProductChatParticipantsQueryRequest
    ) async throws -> ProductChatParticipantsQueryItem {
        try await chatParticipantsQuery(request, timeout: ChatQueryTimeout.lookup)
    }

    func chatMessageActionsQuery(
        _ request: ProductChatMessageActionsQueryRequest
    ) async throws -> ProductChatMessageActionsQueryItem {
        try await chatMessageActionsQuery(request, timeout: ChatQueryTimeout.lookup)
    }

    func chatSearchQuery(
        _ request: ProductChatSearchQueryRequest
    ) async throws -> ProductChatSearchQueryItem {
        try await chatSearchQuery(request, timeout: ChatQueryTimeout.search)
    }

    func chatOpenLink(
        _ request: ProductChatOpenLinkRequest
    ) async throws -> ProductChatOpenLinkItem {
        try await chatOpenLink(request, timeout: ChatQueryTimeout.openLink)
    }

    /// An execution that does not give links answers that the product does
    /// not take them.
    func chatOpenLink(
        _: ProductChatOpenLinkRequest,
        timeout _: Duration
    ) async throws -> ProductChatOpenLinkItem {
        throw ChatQueryError.refused(.notAvailable)
    }

    /// An execution that does not ask searches answers that search is not
    /// available.
    func chatSearchQuery(
        _: ProductChatSearchQueryRequest,
        timeout _: Duration
    ) async throws -> ProductChatSearchQueryItem {
        throw ChatQueryError.refused(.notAvailable)
    }
}

/// How long the host waits for the answer to a chat query. After the timeout
/// the host shows its failure state for that query.
public enum ChatQueryTimeout {
    /// A page of messages.
    public static let messages: Duration = .seconds(10)
    /// A message that the user sent.
    public static let sentMessage: Duration = .seconds(30)
    /// A user action that is not a sent message.
    public static let userAction: Duration = .seconds(10)
    /// Reactions, participants, a message menu or a participant menu.
    public static let lookup: Duration = .seconds(5)
    /// The messages that match a search.
    public static let search: Duration = .seconds(10)
    /// A room link that the user opened. The product can be starting, so the
    /// host waits longer, and it does not block the screen.
    public static let openLink: Duration = .seconds(90)

    /// The timeout of one user action.
    public static func timeout(for action: ChatUserAction) -> Duration {
        if case .newMessage = action { sentMessage } else { userAction }
    }
}

/// Why a chat query has no answer.
public enum ChatQueryError: Error, Equatable {
    /// The product, or the core for a room that does not declare the handler,
    /// refused the query. ``ProductChatQueryError/notAvailable`` asks the host
    /// to apply its default.
    case refused(ProductChatQueryError)
    /// The product, or the core for a room that the host stores, refused the
    /// user action. The host reverts the action, and shows the reason of
    /// ``ProductChatUserActionError/rejected(reason:)`` to the user.
    case actionRefused(ProductChatUserActionError)
    /// The query failed on its way, for example because the worker stopped.
    case failed(reason: String)
    /// The product ended the query without an answer.
    case noAnswer
    /// No answer came before the timeout.
    case timedOut

    fileprivate init(_ refusal: NativeChatQueryRefusal) {
        switch refusal {
        case let .query(error): self = .refused(error)
        case let .userAction(error): self = .actionRefused(error)
        case let .failed(reason): self = .failed(reason: reason)
        }
    }
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

    /// Publish one chat event, and keep it until the product connection
    /// subscribes. A subscriber at the first protocol version receives only
    /// the events that its version carries. The product receives at most one
    /// `seen` event for each room each second.
    public func publishChatEvent(roomId: String, peer: String, event: ChatEvent) throws {
        try inner.publishChatEvent(roomId: roomId, peer: peer, event: event)
    }

    /// Ask the product for one page of messages of a room that it keeps.
    public func chatMessagesQuery(
        _ request: ProductChatMessagesQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatMessagesQueryItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatMessagesQuery(request: request, observer: observer)
        } answer: { answer in
            if case let .messages(page) = answer { page } else { nil }
        }
    }

    /// Tell the product what the user did in a room that it keeps.
    public func chatUserAction(
        _ request: ProductChatUserActionRequest,
        timeout: Duration
    ) async throws -> ProductChatUserActionItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatUserAction(request: request, observer: observer)
        } answer: { answer in
            if case let .userAction(item) = answer { item } else { nil }
        }
    }

    /// Ask the product who reacted to one message.
    public func chatReactionsQuery(
        _ request: ProductChatReactionsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatReactionsQueryItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatReactionsQuery(request: request, observer: observer)
        } answer: { answer in
            if case let .reactions(reactions) = answer { reactions } else { nil }
        }
    }

    /// Ask the product about the participants of a room.
    public func chatParticipantsQuery(
        _ request: ProductChatParticipantsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatParticipantsQueryItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatParticipantsQuery(request: request, observer: observer)
        } answer: { answer in
            if case let .participants(item) = answer { item } else { nil }
        }
    }

    /// Ask the product for the menu of one message.
    public func chatMessageActionsQuery(
        _ request: ProductChatMessageActionsQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatMessageActionsQueryItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatMessageActionsQuery(request: request, observer: observer)
        } answer: { answer in
            if case let .messageMenu(menu) = answer { menu } else { nil }
        }
    }

    /// Ask the product for the messages of a room that it keeps that match
    /// what the user typed, newest first.
    public func chatSearchQuery(
        _ request: ProductChatSearchQueryRequest,
        timeout: Duration
    ) async throws -> ProductChatSearchQueryItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatSearchQuery(request: request, observer: observer)
        } answer: { answer in
            if case let .search(page) = answer { page } else { nil }
        }
    }

    /// Give the product the payload of a room link that the user opened.
    public func chatOpenLink(
        _ request: ProductChatOpenLinkRequest,
        timeout: Duration
    ) async throws -> ProductChatOpenLinkItem {
        try await chatQuery(timeout: timeout) { observer in
            try inner.chatOpenLink(request: request, observer: observer)
        } answer: { answer in
            if case let .openLink(answer) = answer { answer } else { nil }
        }
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

    /// Resolves a capability the web view gates itself through the product's
    /// saved, one-use, or prompted decision.
    public func authorizeDevicePermission(_ request: HostDevicePermissionRequest) async throws -> Bool {
        try await inner.authorizeDevicePermission(request: request)
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

/// Start one chat query, and wait for its answer. A timeout or a cancelled
/// task stops the query in the product.
func chatQuery<Answer>(
    timeout: Duration,
    start: (NativeChatQueryObserver) throws -> NativeChatQuery,
    answer extract: (NativeChatQueryAnswer) -> Answer?
) async throws -> Answer {
    let outcome = ChatQueryOutcome()
    let query = try start(outcome)
    defer { query.cancel() }
    let answer = try await withTaskCancellationHandler {
        let timer = Task {
            guard (try? await Task.sleep(for: timeout)) != nil else { return }
            outcome.resolve(.failure(ChatQueryError.timedOut))
        }
        defer { timer.cancel() }
        return try await outcome.value()
    } onCancel: {
        outcome.resolve(.failure(CancellationError()))
    }
    guard let typed = extract(answer) else {
        throw ChatQueryError.failed(reason: "the product answered another query")
    }
    return typed
}

/// The first result of one chat query: the answer of the product, its
/// refusal, a timeout or a cancellation. Later results have no effect.
final class ChatQueryOutcome: NativeChatQueryObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var result: Result<NativeChatQueryAnswer, Error>?
    private var waiter: CheckedContinuation<NativeChatQueryAnswer, Error>?

    func onAnswer(answer: NativeChatQueryAnswer) {
        resolve(.success(answer))
    }

    func onRefusal(refusal: NativeChatQueryRefusal) {
        resolve(.failure(ChatQueryError(refusal)))
    }

    func onComplete() {
        resolve(.failure(ChatQueryError.noAnswer))
    }

    func resolve(_ outcome: Result<NativeChatQueryAnswer, Error>) {
        let waiter: CheckedContinuation<NativeChatQueryAnswer, Error>? = lock.withLock {
            guard result == nil else { return nil }
            result = outcome
            defer { self.waiter = nil }
            return self.waiter
        }
        waiter?.resume(with: outcome)
    }

    func value() async throws -> NativeChatQueryAnswer {
        try await withCheckedThrowingContinuation { continuation in
            let ready: Result<NativeChatQueryAnswer, Error>? = lock.withLock {
                if result == nil { waiter = continuation }
                return result
            }
            if let ready { continuation.resume(with: ready) }
        }
    }
}

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
