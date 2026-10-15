import Foundation
@testable import Products

final class MockProductPermissionRequester: ProductPermissionRequesting, @unchecked Sendable {
    var decision: PermissionDecision = .allowAlways
    /// When set, every prompt ends with this error, as a prompt that no screen answers.
    var failure: ProductPermissionPromptError?

    private(set) var promptCalls: [(productId: String, permission: ProductPermission)] = []
    private(set) var promptBatchedCalls: [(productId: String, permissions: [ProductPermission])] = []

    func prompt(
        productId: String,
        permission: ProductPermission
    ) async throws -> PermissionDecision {
        promptCalls.append((productId, permission))
        if let failure { throw failure }
        return decision
    }

    func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        promptBatchedCalls.append((productId, permissions))
        if let failure { throw failure }
        return decision
    }
}
