import Foundation
import MessageExchangeKit
import StatementDelivery

/// A recorded outgoing chat request, delivered and renewed until the peer accepts it.
struct OutgoingChatRequestInput: ChatNotificationStatementInput, @unchecked Sendable {
    let message: Chat.RequestMessage
    let session: MessageExchange.SessionRequest

    var statementId: String { message.messageId }

    var identitySignKeyId: String { session.own.signKeyId }

    func oneShotKeyId(period: UInt32) throws -> String {
        try ChatKeyPaths.request(requestId: message.messageId, period: period)
    }
}

extension ChatSigningConsentError: OutgoingStatementTerminalError {}

extension ChatKeyPathsError: OutgoingStatementTerminalError {}
