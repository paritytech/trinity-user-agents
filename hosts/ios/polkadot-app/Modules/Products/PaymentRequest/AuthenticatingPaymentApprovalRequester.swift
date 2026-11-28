import struct TrUAPIHost.ProductId
import Foundation
import Products
import SubstrateSdk

/// Asks for device authentication after an approval when the payment is above the confirmation limit.
/// A failed or cancelled authentication rejects the payment.
struct AuthenticatingPaymentApprovalRequester: PaymentApprovalRequesting {
    let gate: PaymentConfirmationGating
    let authorizeDeviceOwner: @Sendable () async -> Bool
    let wrapped: PaymentApprovalRequesting

    func requestApproval(
        productId: ProductId,
        amount: Balance,
        destination: AccountId
    ) async -> PaymentApprovalDecision {
        let decision = await wrapped.requestApproval(
            productId: productId,
            amount: amount,
            destination: destination
        )

        guard decision == .approved, await gate.requiresAuthentication(amount: amount) else {
            return decision
        }

        return await authorizeDeviceOwner() ? .approved : .rejected
    }
}
