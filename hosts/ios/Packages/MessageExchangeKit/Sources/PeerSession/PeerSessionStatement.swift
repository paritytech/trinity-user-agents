import Foundation
import StatementStore

struct PeerSessionStatement {
    let statement: Statement
    let route: PeerSessionRoute
}

struct PeerSessionRequest<Message: MessageExchange.CodableMessage> {
    let request: MessageExchange.Request<Message>
    let route: PeerSessionRoute
}
