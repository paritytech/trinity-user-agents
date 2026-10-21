import AsyncExtensions
import Foundation
import SubstrateSdk
import TrUAPIHost
import UIKit
import UIKitExt

/// The media of the calls of product rooms, with the call engine and the call screen of the native chat: WebRTC with
/// the TURN relays of the backend. The product carries the signals of the call between the two hosts through its own
/// encrypted channel, so the media is end to end between the devices. The core allows a start only shortly after the
/// user pressed a call button or accepted a call in the room.
final class AppCallMediaHostBridge: CallMediaHostBridge, @unchecked Sendable {
    private let publisher: ChatRoomEventPublishing
    private let makeEngine: @Sendable (ProductCallSignaler, CallRole, ChatCallType) -> CallEngineProtocol
    private let present: @Sendable (CallPeer, CallEngineProtocol, CallRole, ChatCallType) async -> Void
    private let roomName: @Sendable (String, String) async -> String?
    private let calls = ProductCalls()

    init(
        publisher: ChatRoomEventPublishing,
        makeEngine: @escaping @Sendable (ProductCallSignaler, CallRole, ChatCallType) -> CallEngineProtocol,
        present: @escaping @Sendable (CallPeer, CallEngineProtocol, CallRole, ChatCallType) async -> Void,
        roomName: @escaping @Sendable (String, String) async -> String?
    ) {
        self.publisher = publisher
        self.makeEngine = makeEngine
        self.present = present
        self.roomName = roomName
    }

    func startCallMedia(productId: String, roomId: String, callId: String, video: Bool, offerer: Bool) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        guard await !calls.isRunning(key) else { return }

        let signaler = ProductCallSignaler(callId: callId) { [publisher] event in
            try publisher.publish(event, productId: productId, roomId: roomId)
        }
        let role: CallRole = offerer ? .initiator : .acceptor
        let callType: ChatCallType = video ? .video : .audio
        let engine = makeEngine(signaler, role, callType)
        for signal in await calls.start(key, signaler: signaler) {
            signaler.receive(signal)
        }

        let name = await roomName(productId, roomId) ?? roomId
        await present(CallPeer(name: name, accountId: Data(count: 32)), engine, role, callType)
    }

    func callMediaSignal(productId: String, roomId: String, callId: String, signal: Data) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        await calls.signaler(of: key, keeping: signal)?.receive(signal)
    }

    func stopCallMedia(productId: String, roomId: String, callId: String) async throws {
        let key = ProductCalls.Key(productId: productId, roomId: roomId, callId: callId)
        await calls.stop(key)?.closeFromRemote()
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
    private var pending: [Key: [Data]] = [:]

    func isRunning(_ key: Key) -> Bool {
        running[key] != nil
    }

    /// Keeps the call, and gives the signals that came before it.
    func start(_ key: Key, signaler: ProductCallSignaler) -> [Data] {
        running[key] = signaler
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
        return running.removeValue(forKey: key)
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
        if signals.contains(where: { if case .offer = $0 { true } else { false } }) {
            events.send(.offerDelivered)
        }
        if signals.contains(.closed) {
            events.send(.closedSent)
        }
        return .fullySent
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

extension AppCallMediaHostBridge {
    /// The call media of the app: the call engine of the native chat with the TURN relays of the backend, and its
    /// call screen over every screen. The caller of a product room accepts on the call screen at once: the user
    /// accepted the call in the room already.
    static func app(
        publisher: ChatRoomEventPublishing,
        logger: LoggerProtocol = Logger.shared
    ) -> AppCallMediaHostBridge {
        let turnService = TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(),
            tokenProvider: JWTTokenManager.shared
        )
        return AppCallMediaHostBridge(
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
            present: { peer, engine, role, callType in
                await MainActor.run {
                    guard let view = ChatCallViewFactory.createView(
                        peer: peer,
                        engine: engine,
                        role: role,
                        callType: callType,
                        answersAtOnce: role == .acceptor
                    ) else {
                        logger.error("The call screen of a product room did not open")
                        return
                    }
                    view.controller.modalPresentationStyle = .fullScreen
                    view.controller.modalTransitionStyle = .crossDissolve
                    UIWindow.keyWindow?.rootViewController?.topmostPresented.present(view.controller, animated: true)
                }
            },
            roomName: { productId, roomId in
                await ProductRoomCallAlerts.storedRoomName(.chatExtension(productId, roomId: roomId))
            }
        )
    }
}
