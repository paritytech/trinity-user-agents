import Foundation
import MessageExchangeKit
import StatementStore

@testable import polkadot_app

/// Hands out `StubMessageExchangeService`s that report session updates to
/// `onUpdateSessions`.
final class StubMessageExchangeServiceFactory: MessageExchageServiceMaking {
    var onUpdateSessions: (Set<MessageExchange.SessionRequest>) -> Void = { _ in }

    func makeService<M: MessageExchange.CodableMessage>(
        statementStoreConnection _: StatementStoreConnecting,
        delegate _: AnyPeerSessionDelegate<M>,
        compactorFactory _: AnyMessageCompactorFactory<M>?
    ) throws -> AnyMessageExchangeService<M> {
        AnyMessageExchangeService(StubMessageExchangeService<M>(onUpdateSessions: onUpdateSessions))
    }
}
