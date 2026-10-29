import Foundation
import MessageExchangeKit
import StatementStore

enum ChatNotificationSlots {
    static let allocationTimeout: Duration = .seconds(5 * 60)
}

/// Notification-funded statements (requests, acceptances) toward the session's peer.
extension NonPrivateSigningFallbacking {
    func identitySigner(for session: MessageExchange.SessionRequest) async throws -> StatementStoreSigning {
        try await identitySigner(for: session.peer.accountId, ownSignKeyId: session.own.signKeyId)
    }
}
