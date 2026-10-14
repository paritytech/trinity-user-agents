import Foundation

public protocol MessageExchangeServicing {
    associatedtype Message: MessageExchange.CodableMessage

    func updateSessions(_ requests: Set<MessageExchange.SessionRequest>)
    func addMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer)
}

// MARK: - Type Erasure Implementation

public final class AnyMessageExchangeService<M: MessageExchange.CodableMessage>: MessageExchangeServicing {
    public typealias Message = M

    private let updateSessionsClosure: (Set<MessageExchange.SessionRequest>) -> Void
    private let addMessagesToQueueClosure: ([M], MessageExchange.Peer) -> Void

    public init<Service: MessageExchangeServicing>(_ targetService: Service) where Service.Message == M {
        updateSessionsClosure = { peers in
            targetService.updateSessions(peers)
        }

        addMessagesToQueueClosure = { messages, peer in
            targetService.addMessagesToQueue(messages, for: peer)
        }
    }

    public func updateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        updateSessionsClosure(requests)
    }

    public func addMessagesToQueue(_ messages: [Message], for peer: MessageExchange.Peer) {
        addMessagesToQueueClosure(messages, peer)
    }
}
