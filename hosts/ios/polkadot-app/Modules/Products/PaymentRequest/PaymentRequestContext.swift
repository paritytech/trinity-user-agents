import Foundation
import Products
import SubstrateSdk

/// Main-actor owner of a pending approval: created, armed and answered on the same actor as the sheet.
@MainActor
final class PaymentRequestContext {
    let productId: ProductId
    let amountInPlanks: Balance
    let destination: AccountId

    private var continuation: CheckedContinuation<PaymentApprovalDecision, Never>?

    init(productId: ProductId, amountInPlanks: Balance, destination: AccountId) {
        self.productId = productId
        self.amountInPlanks = amountInPlanks
        self.destination = destination
    }

    func setContinuation(_ continuation: CheckedContinuation<PaymentApprovalDecision, Never>) {
        self.continuation = continuation
    }

    func deliverApproved() {
        continuation?.resume(returning: .approved)
        continuation = nil
    }

    func deliverRejected() {
        continuation?.resume(returning: .rejected)
        continuation = nil
    }
}
