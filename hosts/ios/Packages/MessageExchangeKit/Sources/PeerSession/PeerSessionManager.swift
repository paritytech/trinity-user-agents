import Foundation
import Foundation_iOS
import Operation_iOS
import SDKLogger

public final class PeerSessionManager<M: MessageExchange.CodableMessage> {
    public typealias Message = M

    private let sessionFactory: AnyPeerSessionFactory<M>
    private let workQueue: DispatchQueue
    private let logger: SDKLoggerProtocol?
    private let sessionStorage: PeerSessionStorage<M>

    public init(
        sessionFactory: AnyPeerSessionFactory<M>,
        workQueue: DispatchQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.sessionFactory = sessionFactory
        self.workQueue = workQueue
        self.logger = logger
        sessionStorage = PeerSessionStorage(logger: logger)
    }
}

extension PeerSessionManager: PeerSessionManaging {
    public func updateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        workQueue.async { [weak self] in
            self?.performUpdateSessions(requests)
        }
    }

    public func addMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer) {
        workQueue.async { [weak self] in
            self?.performAddMessagesToQueue(messages, for: peer)
        }
    }
}

private extension PeerSessionManager {
    func performUpdateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        // Disconnect from unneeded peers
        let newPeers = Set(requests.map(\.peer))
        sessionStorage.enumeratePeers { peer in
            if !newPeers.contains(peer),
               let sessionId = sessionStorage.clearSession(for: peer) {
                logger?.debug("Cleared session: \(sessionId)")
            }
        }

        // Create sessions for expected peers
        for request in requests {
            createSession(for: request)
        }
    }

    func performAddMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer) {
        guard let session = sessionStorage.session(for: peer) else {
            logger?.warning("Messages were not sent, peer session does not exist")
            return
        }
        session.addMessagesToQueue(messages)
    }

    func createSession(for request: MessageExchange.SessionRequest) {
        logger?.debug("Adding session with peer: \(request.peer.accountId.toHex())")

        guard sessionStorage.session(for: request.peer) == nil else {
            logger?.debug("Session already created")
            return
        }

        guard let session = sessionFactory.makeSession(for: request) else {
            logger?.error("Failed to create session with \(request.peer.accountId.toHex())")
            return
        }

        sessionStorage.saveSession(session)
    }
}
