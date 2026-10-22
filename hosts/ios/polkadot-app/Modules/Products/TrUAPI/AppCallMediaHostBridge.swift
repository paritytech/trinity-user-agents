import AsyncAlgorithms
import AsyncExtensions
import Foundation
import os
import SubstrateSdk
import TrUAPIHost
import UIKit
import UIKitExt
import WebRTC

/// The media of the calls of product rooms, with the call engine and the call screen of the native chat: WebRTC with
/// the TURN relays of the backend. The product carries the signals of the call between the two hosts through its own
/// encrypted channel, so the media is end to end between the devices. The core allows a start only shortly after the
/// user pressed a call button or accepted a call in the room.
///
/// A call that rings while the app is in the foreground opens the incoming call screen of the native chat at once, as
/// the native chat does. Accept and Decline on that screen answer the product (`CallAnswered`), and the call goes on in
/// the same screen when the product starts the media. A call that the user starts opens the outgoing call screen at
/// once, under a call id that the host chose, and shows "Ringing" when a device of the contact read the offer.
final class AppCallMediaHostBridge: CallMediaHostBridge, ProductOutgoingCalling, @unchecked Sendable {
    private let publisher: ChatRoomEventPublishing
    private let feedListeners: ProductFeedListeners
    private let makeEngine: @Sendable (ProductCallSignaler, CallRole, ChatCallType) -> CallEngineProtocol
    private let present: @Sendable (CallPeer, CallEngineProtocol, CallRole, ChatCallType, Bool) async -> Void
    private let peer: @Sendable (String, String) async -> CallPeer
    private let calls = ProductCalls()
    private var ringingTask: Task<Void, Never>?

    init(
        publisher: ChatRoomEventPublishing,
        feedListeners: ProductFeedListeners = .shared,
        makeEngine: @escaping @Sendable (ProductCallSignaler, CallRole, ChatCallType) -> CallEngineProtocol,
        present: @escaping @Sendable (CallPeer, CallEngineProtocol, CallRole, ChatCallType, Bool) async -> Void,
        peer: @escaping @Sendable (String, String) async -> CallPeer
    ) {
        self.publisher = publisher
        self.feedListeners = feedListeners
        self.makeEngine = makeEngine
        self.present = present
        self.peer = peer
    }

    deinit {
        ringingTask?.cancel()
    }

    /// Opens the outgoing call screen of the native chat at once and asks the product for the call under a call id
    /// that the host chose, which is also the id of the offer message of the product.
    func startOutgoingCall(productId: String, roomId: String, video: Bool) async {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: UUID().uuidString)
        let signaler = makeSignaler(key)
        let callType: ChatCallType = video ? .video : .audio
        let engine = makeEngine(signaler, .initiator, callType)
        _ = await calls.start(key, signaler: signaler)

        // The native chat shows "Ringing" once a device of the contact read the offer.
        let watcher = ProductCallOfferWatcher(offerId: key.callId) { signaler.offerRead() }
        let registration = feedListeners.register(
            watcher,
            for: .chatExtension(productId, roomId: roomId),
            showsRoom: false
        )
        await calls.keep(watcher, registration, for: key)

        do {
            try publisher.publish(
                .callRequested(video: video, callId: key.callId),
                productId: productId,
                roomId: roomId
            )
        } catch {
            Logger.shared.error("Room \(roomId) did not ask for a call: \(error)")
            _ = await calls.stop(key)
            return
        }

        await present(peer(productId, roomId), engine, .initiator, callType, false)
    }

    func startCallMedia(productId: String, roomId: String, callId: String, video: Bool, offerer: Bool) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        // A call that rang in the foreground runs on its incoming call screen already, and a call that the user
        // started runs on its outgoing call screen.
        guard await !calls.isRunning(key) else { return }

        let signaler = makeSignaler(key)
        let role: CallRole = offerer ? .initiator : .acceptor
        let callType: ChatCallType = video ? .video : .audio
        let engine = makeEngine(signaler, role, callType)
        for signal in await calls.start(key, signaler: signaler) {
            signaler.receive(signal)
        }

        // An acceptor here accepted on the CallKit screen already.
        await present(peer(productId, roomId), engine, role, callType, role == .acceptor)
    }

    func callMediaSignal(productId: String, roomId: String, callId: String, signal: Data) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        await calls.signaler(of: key, keeping: signal)?.receive(signal)
    }

    func stopCallMedia(productId: String, roomId: String, callId: String) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        await calls.stop(key)?.closeFromRemote()
    }

    /// Opens the incoming call screen of each call that rings while the app is in the foreground, and closes it when
    /// the caller stops before the user answered.
    func followRinging(of store: ChatRoomCallStoring, isInForeground: @escaping @Sendable () async -> Bool) {
        ringingTask?.cancel()
        ringingTask = Task { [weak self] in
            var ringing: [Chat.Id: ProductCalls.Key] = [:]

            for await change in store.changes() {
                guard let self, case let .chatExtension(productId, roomId?) = change.chatId else { continue }

                if case let .ringing(callId, video, _) = change.call {
                    guard ringing[change.chatId] == nil else { continue }
                    let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
                    // The native chat turns away an offer while a call runs: the caller hears that the call ended.
                    if await calls.hasCall(otherThan: key) {
                        await decline(key)
                        continue
                    }
                    guard await isInForeground() else { continue }
                    ringing[change.chatId] = key
                    await ring(key, video: video)
                } else if let key = ringing.removeValue(forKey: change.chatId), change.call == nil {
                    await calls.stopRinging(key)
                }
            }
        }
    }
}

private extension AppCallMediaHostBridge {
    func makeSignaler(_ key: ProductCalls.Key) -> ProductCallSignaler {
        ProductCallSignaler(callId: key.callId) { [publisher, calls] event in
            try publisher.publish(event, productId: key.productId, roomId: key.roomId)
            // The user ended the call on the call screen: the host has no call any more, also when the product
            // does not stop its media.
            if case .callEnded = event {
                Task { _ = await calls.stop(key) }
            }
        }
    }

    func decline(_ key: ProductCalls.Key) async {
        do {
            try publisher.publish(
                .callAnswered(callId: key.callId, accept: false),
                productId: key.productId,
                roomId: key.roomId
            )
        } catch {
            Logger.shared.error("Room \(key.roomId) did not turn away a call: \(error)")
        }
    }

    /// The incoming call screen of a call that rings. The engine waits for the answer of the user.
    func ring(_ key: ProductCalls.Key, video: Bool) async {
        guard await !calls.isRunning(key) else { return }

        let signaler = makeSignaler(key)
        let callType: ChatCallType = video ? .video : .audio
        let engine = ProductRingingCallEngine(inner: makeEngine(signaler, .acceptor, callType)) { [publisher] accept in
            try? publisher.publish(
                .callAnswered(callId: key.callId, accept: accept),
                productId: key.productId,
                roomId: key.roomId
            )
        }
        _ = await calls.start(key, signaler: signaler, ringing: engine)

        await present(peer(key.productId, key.roomId), engine, .acceptor, callType, false)
    }
}

/// The running calls, and the signals that came before their call started.
actor ProductCalls {
    struct Key: Hashable {
        let productId: String
        let roomId: String
        let callId: String
    }

    /// The most signals kept for a call that did not start.
    static let pendingMax = 32

    private var running: [Key: ProductCallSignaler] = [:]
    private var ringing: [Key: ProductRingingCallEngine] = [:]
    private var pending: [Key: [Data]] = [:]
    private var offerWatches: [Key: (ProductCallOfferWatcher, ProductFeedListeners.Registration)] = [:]

    func isRunning(_ key: Key) -> Bool {
        running[key] != nil
    }

    /// True when another call runs.
    func hasCall(otherThan key: Key) -> Bool {
        running.keys.contains { $0 != key }
    }

    /// Keeps the watch of the offer of an outgoing call until the call stops.
    func keep(_ watcher: ProductCallOfferWatcher, _ registration: ProductFeedListeners.Registration, for key: Key) {
        offerWatches[key] = (watcher, registration)
    }

    /// Keeps the call, and gives the signals that came before it.
    func start(_ key: Key, signaler: ProductCallSignaler, ringing engine: ProductRingingCallEngine? = nil) -> [Data] {
        running[key] = signaler
        ringing[key] = engine
        return pending.removeValue(forKey: key) ?? []
    }

    /// The signaler of a running call, or nil after keeping `signal` for a call that did not start.
    func signaler(of key: Key, keeping signal: Data) -> ProductCallSignaler? {
        if let signaler = running[key] { return signaler }
        var kept = pending[key] ?? []
        if kept.count < Self.pendingMax {
            kept.append(signal)
        }
        pending[key] = kept
        return nil
    }

    func stop(_ key: Key) -> ProductCallSignaler? {
        pending[key] = nil
        ringing[key] = nil
        offerWatches.removeValue(forKey: key)?.1.cancel()
        return running.removeValue(forKey: key)
    }

    /// The product ended a call that rang: the caller stopped before the user answered, and the incoming call
    /// screen ends, or the user declined it. A call that the user accepted goes on until the product stops its media.
    func stopRinging(_ key: Key) {
        guard let engine = ringing.removeValue(forKey: key), !engine.isAccepted else { return }
        running[key] = nil
        pending[key] = nil
        if !engine.isAnswered {
            engine.endRinging()
        }
    }
}

/// The call engine of a call that rings on the incoming call screen. Accept connects the engine and tells the product
/// that the user answered. Decline, before an answer, tells the product that the user declined.
final class ProductRingingCallEngine: CallEngineProtocol, @unchecked Sendable {
    private let inner: CallEngineProtocol
    private let answer: @Sendable (Bool) -> Void
    private let state = AsyncCurrentValueSubject<CallEngineState?>(nil)
    private let lock = NSLock()
    private var answered = false
    private var accepted = false

    init(inner: CallEngineProtocol, answer: @escaping @Sendable (Bool) -> Void) {
        self.inner = inner
        self.answer = answer
    }

    var isAnswered: Bool {
        lock.withLock { answered }
    }

    /// The user accepted the call.
    var isAccepted: Bool {
        lock.withLock { accepted }
    }

    /// The caller stopped the call: the screen ends as when the other side hangs up.
    func endRinging() {
        state.send(.disconnected)
    }

    func observeState() -> AnyAsyncSequence<CallEngineState> {
        merge(inner.observeState(), state.compactMap { $0 }).eraseToAnyAsyncSequence()
    }

    func observeRemoteMediaState() -> AnyAsyncSequence<CallRemoteMediaState> {
        inner.observeRemoteMediaState()
    }

    func connect() {
        lock.withLock { accepted = true }
        if markAnswered() {
            answer(true)
        }
        inner.connect()
    }

    func endCall(notifiesRemote: Bool) async {
        if markAnswered(), notifiesRemote {
            answer(false)
            await inner.endCall(notifiesRemote: false)
            return
        }

        await inner.endCall(notifiesRemote: notifiesRemote)
    }

    func attach(localRenderer: RTCVideoRenderer) {
        inner.attach(localRenderer: localRenderer)
    }

    func attach(remoteRenderer: RTCVideoRenderer) {
        inner.attach(remoteRenderer: remoteRenderer)
    }

    func observeVideoState() -> AnyAsyncSequence<Bool> {
        inner.observeVideoState()
    }

    func observeVideoCaptureFailure() -> AnyAsyncSequence<Void> {
        inner.observeVideoCaptureFailure()
    }

    var isMuted: Bool {
        get async { await inner.isMuted }
    }

    func setMuted(_ isMuted: Bool) async -> Bool {
        await inner.setMuted(isMuted)
    }

    var isVideoEnabled: Bool {
        get async { await inner.isVideoEnabled }
    }

    func setVideoEnabled(_ isEnabled: Bool) async {
        await inner.setVideoEnabled(isEnabled)
    }

    /// True for the first answer only.
    private func markAnswered() -> Bool {
        lock.withLock {
            defer { answered = true }
            return !answered
        }
    }
}

/// The signals of one call of a product room. The call engine sends its signals here, and the product gets them as
/// the room event `CallSignal`; the signals of the other side come back through ``receive(_:)``. A signal is the SCALE
/// of the list of `PeerConnectionSignal`s of one send, as the native chat encodes them.
final class ProductCallSignaler: PeerConnectionSignaling, CallMessageEventProviding, @unchecked Sendable {
    private let subject = AsyncReplaySubject<PeerConnectionSignal>(bufferSize: 100)
    private let events = AsyncReplaySubject<CallMessageEvent>(bufferSize: 10)
    private let callId: String
    private let publish: @Sendable (Chat.RoomEvent) throws -> Void

    init(callId: String, publish: @escaping @Sendable (Chat.RoomEvent) throws -> Void) {
        self.callId = callId
        self.publish = publish
    }

    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        subject.eraseToAnyAsyncSequence()
    }

    var messageEvents: AnyAsyncSequence<CallMessageEvent> {
        events.eraseToAnyAsyncSequence()
    }

    @discardableResult
    func send(_ signals: [PeerConnectionSignal]) async throws -> PeerConnectionSignalSendResult {
        do {
            let encoder = ScaleEncoder()
            try signals.encode(scaleEncoder: encoder)
            try publish(.callSignal(callId: callId, signal: encoder.encode()))
            if signals.contains(.closed) {
                // The user ended the call on the call screen: the product ends it for both sides.
                try publish(.callEnded(callId: callId))
            }
        } catch {
            return .notFullySent
        }
        if signals.contains(.closed) {
            events.send(.closedSent)
        }
        return .fullySent
    }

    /// A device of the contact read the offer: the outgoing call screen shows "Ringing".
    func offerRead() {
        events.send(.offerDelivered)
    }

    /// A signal of the other side.
    func receive(_ signal: Data) {
        guard let signals = try? [PeerConnectionSignal](scaleDecoder: ScaleDecoder(data: signal)) else { return }
        for signal in signals {
            subject.send(signal)
        }
    }

    /// The product ended the call: the engine closes as when the other side hangs up.
    func closeFromRemote() {
        subject.send(.closed)
    }
}

/// Starts the calls that the user asks for in product rooms.
protocol ProductOutgoingCalling: Sendable {
    func startOutgoingCall(productId: String, roomId: String, video: Bool) async
}

/// Follows the offer message of an outgoing call in the feed of its room. The offer message has the id of the call.
final class ProductCallOfferWatcher: ProductFeedListening, @unchecked Sendable {
    private let offerId: String
    private let onRead: @Sendable () -> Void
    private let isRead = OSAllocatedUnfairLock(initialState: false)

    init(offerId: String, onRead: @escaping @Sendable () -> Void) {
        self.offerId = offerId
        self.onRead = onRead
    }

    func apply(roomId _: String, update: ChatFeedUpdate) async {
        let read: Bool =
            switch update {
            case let .new(message),
                 let .replaced(message):
                message.messageId == offerId && message.status == .read
            case let .statusChanged(messageId, status):
                messageId == offerId && status == .read
            default:
                false
            }

        guard read, isRead.withLock({ isRead in
            defer { isRead = true }
            return !isRead
        }) else {
            return
        }

        onRead()
    }
}

extension AppCallMediaHostBridge {
    /// The call media of the app: the call engine of the native chat with the TURN relays of the backend, and its
    /// call screen over every screen. The peer is the contact of the room, as the native chat names it.
    static func app(
        publisher: ChatRoomEventPublishing,
        rooms: ProductRoomReading = ProductRoomReader(storageFacade: UserDataStorageFacade.shared),
        logger: LoggerProtocol = Logger.shared
    ) -> AppCallMediaHostBridge {
        let turnService = TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(),
            tokenProvider: JWTTokenManager.shared
        )
        let bridge = AppCallMediaHostBridge(
            publisher: publisher,
            makeEngine: { signaler, role, callType in
                CallEngine(
                    signaling: signaler,
                    role: role,
                    initialCallType: callType,
                    purpose: WebRTCConnectionPurpose.call.rawValue,
                    configFactory: WebRTCConfigFactory(turnService: turnService),
                    peerConnectionFactory: WebRTCPeerConnectionFactoryProvider.make(),
                    logger: logger
                )
            },
            present: { peer, engine, role, callType, answersAtOnce in
                await MainActor.run {
                    guard let view = ChatCallViewFactory.createView(
                        peer: peer,
                        engine: engine,
                        role: role,
                        callType: callType,
                        answersAtOnce: answersAtOnce
                    ) else {
                        logger.error("The call screen of a product room did not open")
                        return
                    }
                    view.controller.modalPresentationStyle = .fullScreen
                    view.controller.modalTransitionStyle = .crossDissolve
                    UIWindow.keyWindow?.rootViewController?.topmostPresented.present(view.controller, animated: true)
                }
            },
            peer: { productId, roomId in
                let chatId = Chat.Id.chatExtension(productId, roomId: roomId)
                let declarations = await rooms.declarations(of: chatId)
                let name = await rooms.name(of: chatId) ?? roomId
                guard let contact = declarations?.contact else {
                    return CallPeer(name: name, accountId: Data(count: 32))
                }
                return CallPeer(name: contact.name(fallback: name), accountId: contact.account)
            }
        )
        bridge.followRinging(of: ChatRoomCallStore.shared) {
            await MainActor.run { UIApplication.shared.applicationState == .active }
        }
        return bridge
    }
}
