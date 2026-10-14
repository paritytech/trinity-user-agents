import Foundation
import SubstrateSdk

public enum PaymentApprovalDecision: Sendable, Equatable {
    case approved
    case rejected
}

/// Asks the user to approve an outgoing product payment.
public protocol PaymentApprovalRequesting: Sendable {
    func requestApproval(
        productId: String,
        amount: Balance,
        destination: AccountId
    ) async -> PaymentApprovalDecision
}
