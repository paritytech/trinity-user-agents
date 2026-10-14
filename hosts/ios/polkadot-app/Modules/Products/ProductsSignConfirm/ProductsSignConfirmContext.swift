import Foundation

/// Carries the state of an in-flight rust-core signing confirmation.
///
/// Exposes the review (via ``ProductsSignConfirmContextProtocol``) so the model
/// factory can render the sheet, and resolves the user's Bool decision exactly
/// once. The rust core performs the signing itself, so there is no reply
/// channel and no wallet resolution.
@MainActor
final class ProductsSignConfirmContext: ProductsSignConfirmContextProtocol {
    nonisolated let requester: PolkadotSigningRequester
    nonisolated let input: ProductsSignConfirmInput

    private var continuation: CheckedContinuation<Bool, Never>?

    nonisolated init(
        requester: PolkadotSigningRequester,
        input: ProductsSignConfirmInput
    ) {
        self.requester = requester
        self.input = input
    }

    deinit {
        continuation?.resume(returning: false)
    }

    func setContinuation(_ continuation: CheckedContinuation<Bool, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: Bool) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
