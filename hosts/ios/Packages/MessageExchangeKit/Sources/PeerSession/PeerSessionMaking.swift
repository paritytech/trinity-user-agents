import Foundation

public protocol PeerSessionMaking {
    associatedtype Message: MessageExchange.CodableMessage

    func makeSession(for request: MessageExchange.SessionRequest) -> AnyPeerSession<Message>?
}

// MARK: - Type Erasure Implementation

public final class AnyPeerSessionFactory<M: MessageExchange.CodableMessage>: PeerSessionMaking {
    public typealias Message = M

    private let makeSessionClosure: (MessageExchange.SessionRequest) -> AnyPeerSession<M>?

    public init<Maker: PeerSessionMaking>(_ targetMaker: Maker) where Maker.Message == M {
        makeSessionClosure = { request in
            targetMaker.makeSession(for: request)
        }
    }

    public func makeSession(for request: MessageExchange.SessionRequest) -> AnyPeerSession<Message>? {
        makeSessionClosure(request)
    }
}
