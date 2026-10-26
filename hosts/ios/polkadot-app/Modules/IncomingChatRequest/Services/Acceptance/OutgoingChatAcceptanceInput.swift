import Foundation
import MessageExchangeKit
import StatementDelivery

/// A recorded acceptance, published on the identity route and renewed until the requester speaks on the device route.
struct OutgoingChatAcceptanceInput: ChatNotificationStatementInput, @unchecked Sendable {
    let message: Chat.LocalMessage
    let requestId: String
    let contact: Chat.Contact

    var session: MessageExchange.SessionRequest { contact.toMessageExchangeSessionRequest() }

    var statementId: String { message.messageId }

    var identitySignKeyId: String { session.own.signKeyId }

    func oneShotKeyId(period: UInt32) throws -> String {
        try ChatKeyPaths.accept(requestId: requestId, period: period)
    }
}

extension OutgoingChatAcceptanceInput {
    init?(message: Chat.LocalMessage, contact: Chat.Contact) {
        guard case let .multiChatAccepted(acceptance) = message.content else { return nil }

        self.init(message: message, requestId: acceptance.requestId, contact: contact)
    }
}
