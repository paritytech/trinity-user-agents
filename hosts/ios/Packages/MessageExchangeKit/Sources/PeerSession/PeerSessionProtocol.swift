import Foundation

public protocol PeerSessionProtocol {
    associatedtype Message: MessageExchange.CodableMessage

    var peer: MessageExchange.Peer { get }
    var sessionId: MessageExchange.SessionId { get }

    func addMessagesToQueue(_ messages: [Message])
}

// MARK: - Type Erasure Implementation

public final class AnyPeerSession<M: MessageExchange.CodableMessage>: PeerSessionProtocol {
    public typealias Message = M

    let peerClosure: () -> MessageExchange.Peer
    let sessionIdClosure: () -> MessageExchange.SessionId

    private let addMessagesToQueueClosure: ([M]) -> Void

    public init<Session: PeerSessionProtocol>(_ targetSession: Session) where Session.Message == M {
        peerClosure = { targetSession.peer }

        sessionIdClosure = { targetSession.sessionId }

        addMessagesToQueueClosure = { messages in
            targetSession.addMessagesToQueue(messages)
        }
    }

    public func addMessagesToQueue(_ messages: [Message]) {
        addMessagesToQueueClosure(messages)
    }

    public var peer: MessageExchange.Peer { peerClosure() }

    public var sessionId: MessageExchange.SessionId { sessionIdClosure() }
}
