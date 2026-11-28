import struct TrUAPIHost.ProductId
import Products
import SubstrateSdk

struct PaymentApprovalRequesterStub: PaymentApprovalRequesting {
    let decision: PaymentApprovalDecision

    func requestApproval(
        productId _: ProductId,
        amount _: Balance,
        destination _: AccountId
    ) async -> PaymentApprovalDecision {
        decision
    }
}
