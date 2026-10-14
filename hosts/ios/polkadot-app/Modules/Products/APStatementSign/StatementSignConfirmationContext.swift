import Foundation
import Products

enum StatementSignDecision {
    case approved
    case rejected
}

struct StatementSignConfirmationRequest: Equatable {
    let productId: ProductId
    let payload: Data
}

/// Carries the state of an in-flight statement-sign confirmation prompt.
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`;
/// the decision is delivered exactly once.
@MainActor
final class StatementSignConfirmationContext {
    nonisolated let request: StatementSignConfirmationRequest

    private var continuation: CheckedContinuation<StatementSignDecision, Never>?

    init(request: StatementSignConfirmationRequest) {
        self.request = request
    }

    deinit {
        continuation?.resume(returning: .rejected)
    }

    func setContinuation(_ continuation: CheckedContinuation<StatementSignDecision, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: StatementSignDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
