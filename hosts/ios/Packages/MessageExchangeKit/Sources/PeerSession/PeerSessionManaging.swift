import Foundation

public protocol PeerSessionManaging {
    associatedtype Message: MessageExchange.CodableMessage

    func updateSessions(_ requests: Set<MessageExchange.SessionRequest>)
    func addMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer)
}

// MARK: - Type Erasure Implementation

public final class AnyPeerSessionManager<M: MessageExchange.CodableMessage>: PeerSessionManaging {
    public typealias Message = M

    private let updateSessionsClosure: (Set<MessageExchange.SessionRequest>) -> Void
    private let addMessagesToQueueClosure: ([M], MessageExchange.Peer) -> Void

    public init<P: PeerSessionManaging>(_ targetManager: P) where P.Message == M {
        updateSessionsClosure = { requests in
            targetManager.updateSessions(requests)
        }

        addMessagesToQueueClosure = { messages, peer in
            targetManager.addMessagesToQueue(messages, for: peer)
        }
    }

    public func updateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        updateSessionsClosure(requests)
    }

    public func addMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer) {
        addMessagesToQueueClosure(messages, peer)
    }
}
