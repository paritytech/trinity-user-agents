import struct TrUAPIHost.ProductId
import func TrUAPIHost.productIdName
import Foundation
import SubstrateSdk

/// Approves payments without a prompt for an allowlisted set of products.
/// Wraps the real requester for everything else. Same shape and rationale as
/// ``AutoAllowProductPermissionRequester``.
public struct AutoAllowPaymentApprovalRequester: PaymentApprovalRequesting {
    private let allowedLabels: Set<String>
    private let wrapped: PaymentApprovalRequesting

    public init(allowedLabels: Set<String>, wrapped: PaymentApprovalRequesting) {
        self.allowedLabels = allowedLabels
        self.wrapped = wrapped
    }

    public func requestApproval(
        productId: ProductId,
        amount: Balance,
        destination: AccountId
    ) async -> PaymentApprovalDecision {
        guard !isAutoAllowed(productId: productId) else { return .approved }

        return await wrapped.requestApproval(
            productId: productId,
            amount: amount,
            destination: destination
        )
    }
}

private extension AutoAllowPaymentApprovalRequester {
    /// Matches the label only, dropping the root: the root is the chain TLD and
    /// differs per network, and resolving it here would put a network call in
    /// front of an auto-allow.
    func isAutoAllowed(productId: ProductId) -> Bool {
        guard let name = productIdName(productId: productId) else { return false }
        return allowedLabels.contains(name)
    }
}
