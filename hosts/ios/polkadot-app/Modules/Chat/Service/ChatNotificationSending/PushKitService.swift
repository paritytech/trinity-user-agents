import PushKit
import AsyncExtensions

protocol PushKitServicing {
    func register(for pushTypes: Set<PKPushType>)
    func observeToken() -> AnyAsyncSequence<PushKitToken>
    func token(with type: PKPushType) async -> Data?
}

final class PushKitService: NSObject {
    static let shared = PushKitService()

    private let voipCallManager: VoIPCallKitManaging
    private let pushRegistry: PKPushRegistry
    private let logger: LoggerProtocol
    private let tokenSubject = AsyncPassthroughSubject<PushKitToken>()
    private let state = State()

    init(
        voipCallManager: VoIPCallKitManaging = VoIPCallKitManager.shared,
        queue: DispatchQueue = PushKitQueueProvider.queue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.voipCallManager = voipCallManager
        self.logger = logger
        pushRegistry = PKPushRegistry(queue: queue)
        super.init()
        pushRegistry.delegate = self
    }
}

extension PushKitService: PushKitServicing {
    func register(for pushTypes: Set<PKPushType>) {
        pushRegistry.desiredPushTypes = pushTypes
    }

    func observeToken() -> AnyAsyncSequence<PushKitToken> {
        tokenSubject.eraseToAnyAsyncSequence()
    }

    func token(with type: PKPushType) async -> Data? {
        await state.token(with: type)
    }
}

extension PushKitService: PKPushRegistryDelegate {
    func pushRegistry(
        _: PKPushRegistry,
        didUpdate pushCredentials: PKPushCredentials,
        for type: PKPushType
    ) {
        logger.debug("Did update token for type \(type.rawValue): \(pushCredentials.token.toHex())")

        Task {
            await state.setToken(pushCredentials.token, with: type)
            tokenSubject.send(.init(type: type, payload: pushCredentials.token))
        }
    }

    func pushRegistry(_: PKPushRegistry, didInvalidatePushTokenFor type: PKPushType) {
        logger.debug("Did invalidate push token for type \(type.rawValue)")
    }

    func pushRegistry(
        _: PKPushRegistry,
        didReceiveIncomingPushWith payload: PKPushPayload,
        for type: PKPushType,
        completion: @escaping () -> Void
    ) {
        logger.debug("Did receive incoming push for type \(type.rawValue): \(payload.dictionaryPayload)")

        guard type == .voIP else {
            completion()
            return
        }

        voipCallManager.reportIncomingCall(
            fromPushPayload: payload.dictionaryPayload,
            completion: completion
        )
    }
}

struct PushKitToken {
    let type: PKPushType
    let payload: Data
}

private extension PushKitService {
    actor State {
        private var tokensByType = [PKPushType: Data]()

        func token(with type: PKPushType) -> Data? {
            tokensByType[type]
        }

        func setToken(_ token: Data, with type: PKPushType) {
            tokensByType[type] = token
        }
    }
}

enum PushKitQueueProvider {
    static let queue = DispatchQueue(label: "PushKitService.callbackQueue")
}
