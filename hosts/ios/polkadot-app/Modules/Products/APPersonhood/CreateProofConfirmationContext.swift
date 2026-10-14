import Foundation
import Products

enum CreateProofDecision {
    case approved
    case rejected
}

struct CreateProofConfirmationRequest {
    let callingProductId: ProductId?
    let onBehalfOfProductId: ProductId
    let suffix: Data
    let message: Data
}

/// Carries the state of an in-flight create-proof confirmation prompt.
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`;
/// the decision is delivered exactly once.
@MainActor
final class CreateProofConfirmationContext {
    nonisolated let request: CreateProofConfirmationRequest

    private var continuation: CheckedContinuation<CreateProofDecision, Never>?

    init(request: CreateProofConfirmationRequest) {
        self.request = request
    }

    func setContinuation(_ continuation: CheckedContinuation<CreateProofDecision, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: CreateProofDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
