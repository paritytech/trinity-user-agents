import Foundation
import Products

enum SignVrfDecision {
    case approved
    case rejected
}

struct SignVrfConfirmationRequest {
    let callingProductId: ProductId?
    let payload: SignVrfPayload
}

/// Carries the state of an in-flight sign-vrf confirmation prompt.
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`;
/// the decision is delivered exactly once.
@MainActor
final class SignVrfConfirmationContext {
    nonisolated let request: SignVrfConfirmationRequest

    private var continuation: CheckedContinuation<SignVrfDecision, Never>?

    init(request: SignVrfConfirmationRequest) {
        self.request = request
    }

    deinit {
        continuation?.resume(returning: .rejected)
    }

    func setContinuation(_ continuation: CheckedContinuation<SignVrfDecision, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: SignVrfDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
