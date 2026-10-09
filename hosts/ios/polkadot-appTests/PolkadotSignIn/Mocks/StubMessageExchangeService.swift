import Foundation
import MessageExchangeKit

/// Exchange service that sends nothing and reports each session update.
final class StubMessageExchangeService<M: MessageExchange.CodableMessage>: MessageExchangeServicing {
    typealias Message = M

    private let onUpdateSessions: (Set<MessageExchange.SessionRequest>) -> Void

    init(onUpdateSessions: @escaping (Set<MessageExchange.SessionRequest>) -> Void) {
        self.onUpdateSessions = onUpdateSessions
    }

    func updateSessions(_ requests: Set<MessageExchange.SessionRequest>) {
        onUpdateSessions(requests)
    }

    func addMessagesToQueue(_: [M], for _: MessageExchange.Peer) {}
}
