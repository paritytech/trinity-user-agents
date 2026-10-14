import Foundation
import SubstrateSdk
@testable import Products

final class MockPaymentApprovalRequester: PaymentApprovalRequesting, @unchecked Sendable {
    var decision: PaymentApprovalDecision = .approved

    private(set) var calls: [(productId: String, amount: Balance, destination: AccountId)] = []

    func requestApproval(
        productId: String,
        amount: Balance,
        destination: AccountId
    ) async -> PaymentApprovalDecision {
        calls.append((productId, amount, destination))
        return decision
    }
}
