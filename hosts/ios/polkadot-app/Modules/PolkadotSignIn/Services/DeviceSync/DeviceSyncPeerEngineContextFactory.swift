import Foundation

protocol DeviceSyncPeerEngineContextMaking {
    func makeContext(peerStatementAccountId: Data) -> DeviceSyncPeerEngineContext
}

final class DeviceSyncPeerEngineContextFactory: DeviceSyncPeerEngineContextMaking {
    private let componentFactory: any DeviceSyncPeerComponentMaking
    private let sessionDelegate: any DeviceSyncPeerSessionDelegating
    private let persistedOfferId: String?
    private let offerIdPersistenceHandler: @Sendable (String) async throws -> Void
    private let restartDelayPolicy: DeviceSyncRestartDelayPolicy
    private let peerLogger: LoggerProtocol
    private let sleep: DeviceSyncSleep

    init(
        componentFactory: any DeviceSyncPeerComponentMaking,
        sessionDelegate: any DeviceSyncPeerSessionDelegating,
        persistedOfferId: String?,
        offerIdPersistenceHandler: @escaping @Sendable (String) async throws -> Void,
        restartDelayPolicy: DeviceSyncRestartDelayPolicy,
        peerLogger: LoggerProtocol,
        sleep: @escaping DeviceSyncSleep = liveDeviceSyncSleep
    ) {
        self.componentFactory = componentFactory
        self.sessionDelegate = sessionDelegate
        self.persistedOfferId = persistedOfferId
        self.offerIdPersistenceHandler = offerIdPersistenceHandler
        self.restartDelayPolicy = restartDelayPolicy
        self.peerLogger = peerLogger
        self.sleep = sleep
    }

    func makeContext(peerStatementAccountId: Data) -> DeviceSyncPeerEngineContext {
        DeviceSyncPeerEngineContext(
            peerStatementAccountId: peerStatementAccountId,
            componentFactory: componentFactory,
            sessionDelegate: sessionDelegate,
            persistedOfferId: persistedOfferId,
            offerIdPersistenceHandler: offerIdPersistenceHandler,
            restartDelayPolicy: restartDelayPolicy,
            peerLogger: peerLogger,
            sleep: sleep
        )
    }
}
