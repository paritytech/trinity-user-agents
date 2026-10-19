import Foundation
@testable import Products

final class MockProductPermissionRequester: ProductPermissionRequesting, @unchecked Sendable {
    var decision: PermissionDecision = .allowAlways
    var error: Error?

    private(set) var promptCalls: [(productId: String, permission: ProductPermission)] = []
    private(set) var promptBatchedCalls: [(productId: String, permissions: [ProductPermission])] = []

    func prompt(
        productId: String,
        permission: ProductPermission
    ) async throws -> PermissionDecision {
        promptCalls.append((productId, permission))
        if let error { throw error }
        return decision
    }

    func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        promptBatchedCalls.append((productId, permissions))
        if let error { throw error }
        return decision
    }
}
