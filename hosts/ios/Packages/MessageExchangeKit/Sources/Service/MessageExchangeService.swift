import Foundation
import Foundation_iOS

public final class MessageExchangeService<M: MessageExchange.CodableMessage> {
    public typealias Message = M

    private let sessionManager: AnyPeerSessionManager<M>

    public init(
        sessionManager: AnyPeerSessionManager<M>,
    ) {
        self.sessionManager = sessionManager
    }
}

extension MessageExchangeService: MessageExchangeServicing {
    public func updateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        sessionManager.updateSessions(requests)
    }

    public func addMessagesToQueue(_ messages: [M], for peer: MessageExchange.Peer) {
        sessionManager.addMessagesToQueue(messages, for: peer)
    }
}
