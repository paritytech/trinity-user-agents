import Foundation
import Products
import SubstrateSdk

/// Presents the payment request sheet and suspends until the user decides.
final class PaymentApprovalRequester: PaymentApprovalRequesting, @unchecked Sendable {
    private let router: ProductsRouting

    init(router: ProductsRouting) {
        self.router = router
    }

    func requestApproval(
        productId: String,
        amount: Balance,
        destination: AccountId
    ) async -> PaymentApprovalDecision {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                let context = PaymentRequestContext(
                    productId: productId,
                    amountInPlanks: amount,
                    destination: destination
                )
                context.setContinuation(continuation)
                router.showPaymentRequest(context: context)
            }
        }
    }
}
