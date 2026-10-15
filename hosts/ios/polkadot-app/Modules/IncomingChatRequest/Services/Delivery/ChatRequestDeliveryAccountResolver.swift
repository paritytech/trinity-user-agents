import Foundation
import Individuality

protocol ChatRequestDeliveryAccountResolving {
    func maxStatementSize() async throws -> Int
    func resolveFirstDelivery(requestId: String) async throws -> ChatRequestDeliverySigner
}

/// Picks the notification-funded account of the current period for a request's first delivery. Never falls back
/// to the username account, which would link the request to us: without a slot the request stays pending.
final class ChatRequestDeliveryAccountResolver {
    static let allocationTimeout: Duration = .seconds(5 * 60)

    private let allocator: NotificationStatementAccountAllocating
    private let signers: ChatRequestDeliverySigning

    init(allocator: NotificationStatementAccountAllocating, signers: ChatRequestDeliverySigning) {
        self.allocator = allocator
        self.signers = signers
    }
}

extension ChatRequestDeliveryAccountResolver: ChatRequestDeliveryAccountResolving {
    func maxStatementSize() async throws -> Int {
        try await allocator.maxStatementSize()
    }

    func resolveFirstDelivery(requestId: String) async throws -> ChatRequestDeliverySigner {
        let period = try await allocator.currentPeriod()
        let anonymous = try signers.anonymous(requestId: requestId, period: period)

        try await allocator.allocate(anonymous.signer.accountId, timeout: Self.allocationTimeout)

        return anonymous
    }
}
