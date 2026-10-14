import Foundation
@testable import Products

final class MockProductPermissionRequester: ProductPermissionRequesting, @unchecked Sendable {
    var decision: PermissionDecision = .allowAlways

    private(set) var promptCalls: [(productId: String, permission: ProductPermission)] = []
    private(set) var promptBatchedCalls: [(productId: String, permissions: [ProductPermission])] = []

    func prompt(
        productId: String,
        permission: ProductPermission
    ) async -> PermissionDecision {
        promptCalls.append((productId, permission))
        return decision
    }

    func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async -> PermissionDecision {
        promptBatchedCalls.append((productId, permissions))
        return decision
    }
}
