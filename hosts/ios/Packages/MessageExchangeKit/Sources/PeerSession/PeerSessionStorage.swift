import Foundation
import Foundation_iOS
import SDKLogger

final class PeerSessionStorage<M: MessageExchange.CodableMessage> {
    private var sessions = InMemoryCache<MessageExchange.SessionId, AnyPeerSession<M>>()
    private var idsByPeer = InMemoryCache<MessageExchange.Peer, MessageExchange.SessionId>()

    private let logger: SDKLoggerProtocol?

    init(logger: SDKLoggerProtocol?) {
        self.logger = logger
    }
}

extension PeerSessionStorage {
    func session(for sessionId: MessageExchange.SessionId) -> AnyPeerSession<M>? {
        sessions.fetchValue(for: sessionId)
    }

    func session(for peer: MessageExchange.Peer) -> AnyPeerSession<M>? {
        guard let sessionId = idsByPeer.fetchValue(for: peer) else {
            return nil
        }
        return sessions.fetchValue(for: sessionId)
    }

    func saveSession(_ session: AnyPeerSession<M>) {
        let sessionId = session.sessionIdClosure()
        let peer = session.peerClosure()
        sessions.store(value: session, for: sessionId)
        idsByPeer.store(value: sessionId, for: peer)
    }

    func clearSession(for peer: MessageExchange.Peer) -> MessageExchange.SessionId? {
        let sessionId = idsByPeer.fetchValue(for: peer)
        idsByPeer.clear(for: peer)

        if let sessionId {
            sessions.clear(for: sessionId)
        }

        return sessionId
    }

    func enumeratePeers(_ closure: (MessageExchange.Peer) -> Void) {
        idsByPeer.allKeys().forEach(closure)
    }
}

private extension PeerSessionStorage {
    enum StorageError: Error {
        case noRawSessionIdInStatement
    }
}
