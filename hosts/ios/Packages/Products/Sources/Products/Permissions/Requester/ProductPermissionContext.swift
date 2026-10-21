import Foundation

/// Carries the state of an in-flight permission prompt (single or batched).
///
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`.
/// The continuation is resolved exactly once: the first `deliver(_:)` or
/// `abandon(_:)` wins, and the calls after it do nothing.
@MainActor
public final class ProductPermissionContext {
    public nonisolated let productId: String
    public nonisolated let permissions: [ProductPermission]

    private var continuation: CheckedContinuation<PermissionDecision, Error>?
    private var isResolved = false

    public init(productId: String, permissions: [ProductPermission]) {
        self.productId = productId
        self.permissions = permissions
    }

    /// Convenience for a single permission prompt.
    public convenience init(productId: String, permission: ProductPermission) {
        self.init(productId: productId, permissions: [permission])
    }

    /// True after a decision or an abandon resolved the prompt.
    public var isAnswered: Bool {
        isResolved
    }

    public func setContinuation(_ continuation: CheckedContinuation<PermissionDecision, Error>) {
        guard !isResolved else {
            // The prompt ended before the caller started to wait. The caller gets the same end.
            continuation.resume(throwing: ProductPermissionPromptError.closedWithoutAnswer)
            return
        }
        self.continuation = continuation
    }

    public func deliver(_ decision: PermissionDecision) {
        guard !isResolved else { return }
        isResolved = true
        continuation?.resume(returning: decision)
        continuation = nil
    }

    /// Ends the wait with no decision. The caller gets `error` and keeps no refusal.
    public func abandon(_ error: ProductPermissionPromptError) {
        guard !isResolved else { return }
        isResolved = true
        continuation?.resume(throwing: error)
        continuation = nil
    }
}
