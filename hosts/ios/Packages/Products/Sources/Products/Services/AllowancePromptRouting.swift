import Foundation
import UIKitExt

// MARK: - Decision

public enum AllowancePromptDecision {
    case approved
    case rejected
}

// MARK: - Context

/// Carries the state of an in-flight resource allocation approval prompt.
///
/// Bridges the prompt UI to the async caller via a `CheckedContinuation`.
/// The continuation is delivered exactly once — subsequent `deliver(_:)` calls
/// are ignored.
@MainActor
public final class AllowancePromptContext {
    public nonisolated let productId: ProductId
    public nonisolated let resources: [AllocatableResource]

    private var continuation: CheckedContinuation<AllowancePromptDecision, Never>?

    public init(productId: ProductId, resources: [AllocatableResource]) {
        self.productId = productId
        self.resources = resources
    }

    public func setContinuation(_ continuation: CheckedContinuation<AllowancePromptDecision, Never>) {
        self.continuation = continuation
    }

    public func deliver(_ decision: AllowancePromptDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}

// MARK: - Routing

@MainActor
public protocol AllowancePromptRouting: ProductsRouting {
    func showAllowancePrompt(context: AllowancePromptContext)
}
