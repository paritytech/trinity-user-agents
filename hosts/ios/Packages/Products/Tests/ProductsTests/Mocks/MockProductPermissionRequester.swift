import struct TrUAPIHost.ProductId
import Foundation
@testable import Products

final class MockProductPermissionRequester: ProductPermissionRequesting, @unchecked Sendable {
    var decision: PermissionDecision = .allowAlways

    private(set) var promptCalls: [(productId: ProductId, permission: ProductPermission)] = []
    private(set) var promptBatchedCalls: [(productId: ProductId, permissions: [ProductPermission])] = []

    func prompt(
        productId: ProductId,
        permission: ProductPermission
    ) async -> PermissionDecision {
        promptCalls.append((productId, permission))
        return decision
    }

    func promptBatched(
        productId: ProductId,
        permissions: [ProductPermission]
    ) async -> PermissionDecision {
        promptBatchedCalls.append((productId, permissions))
        return decision
    }
}
