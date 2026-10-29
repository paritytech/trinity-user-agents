import Foundation
import StatementStore
import SubstrateSdk

struct PeerSessionStatement {
    let statement: Statement
    let route: PeerSessionRoute
    /// The account the topic was derived for. Any account may sign, so decoding keys off this, not the signer.
    let topicAccountId: AccountId
}

struct PeerSessionRequest<Message: MessageExchange.CodableMessage> {
    let request: MessageExchange.Request<Message>
    let route: PeerSessionRoute
}
