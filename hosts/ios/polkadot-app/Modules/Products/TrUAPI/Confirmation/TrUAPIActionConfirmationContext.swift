import Foundation
import Products

enum TrUAPIActionConfirmationRequest: Equatable, Sendable {
    case preimageSubmit(productId: ProductId, size: UInt64)
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
