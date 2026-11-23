import Foundation
import Products
import TrUAPIHost

enum TrUAPIActionConfirmationRequest: Equatable, Sendable {
    case productSubtree(productId: ProductId)
}

@MainActor
final class TrUAPIActionConfirmationContext {
    nonisolated let request: TrUAPIActionConfirmationRequest

    private var continuation: CheckedContinuation<Bool, Never>?

    init(request: TrUAPIActionConfirmationRequest) {
        self.request = request
    }

    deinit {
        continuation?.resume(returning: false)
    }

    func setContinuation(_ continuation: CheckedContinuation<Bool, Never>) {
        self.continuation = continuation
    }

    func deliver(_ approved: Bool) {
        continuation?.resume(returning: approved)
        continuation = nil
    }
}

/// Unlike action confirmation, upload consent preserves the selected lifetime.
@MainActor
final class TrUAPIPreimageConfirmationContext {
    nonisolated let review: PreimageSubmitReview
    private var continuation: CheckedContinuation<TrUAPIPermissionDecision, Never>?

    init(review: PreimageSubmitReview) {
        self.review = review
    }

    deinit {
        continuation?.resume(returning: .deny)
    }

    func setContinuation(_ continuation: CheckedContinuation<TrUAPIPermissionDecision, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: TrUAPIPermissionDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
