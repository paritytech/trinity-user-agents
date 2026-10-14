import Foundation

/// Carries the state of an in-flight permission prompt (single or batched).
///
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`.
/// The continuation is delivered exactly once — subsequent `deliver(_:)`
/// calls are ignored.
@MainActor
public final class ProductPermissionContext {
    public nonisolated let productId: String
    public nonisolated let permissions: [ProductPermission]

    private var continuation: CheckedContinuation<PermissionDecision, Never>?

    public init(productId: String, permissions: [ProductPermission]) {
        self.productId = productId
        self.permissions = permissions
    }

    /// Convenience for a single permission prompt.
    public convenience init(productId: String, permission: ProductPermission) {
        self.init(productId: productId, permissions: [permission])
    }

    public func setContinuation(_ continuation: CheckedContinuation<PermissionDecision, Never>) {
        self.continuation = continuation
    }

    public func deliver(_ decision: PermissionDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
