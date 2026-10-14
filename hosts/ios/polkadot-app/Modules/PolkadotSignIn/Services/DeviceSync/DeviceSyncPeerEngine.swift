import Foundation

protocol DeviceSyncPeerEngining: AnyObject, Sendable {
    func start() async
    func dispose() async
}

enum DeviceSyncPeerEngineError: Error {
    case dependenciesUnavailable
}

final class DeviceSyncPeerEngine: DeviceSyncPeerEngining {
    private let context: DeviceSyncPeerEngineContext

    init(
        peerStatementAccountId: Data,
        contextFactory: any DeviceSyncPeerEngineContextMaking
    ) {
        context = contextFactory.makeContext(peerStatementAccountId: peerStatementAccountId)
    }

    func start() async { await context.start() }
    func dispose() async { await context.dispose() }
}
