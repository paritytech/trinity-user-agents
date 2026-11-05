import struct TrUAPIHost.ProductId
import Foundation
@testable import Products

final class MockProductPermissionRequester: ProductPermissionRequesting, @unchecked Sendable {
    var decision: PermissionDecision = .allowAlways
    var error: Error?

    private(set) var promptCalls: [(productId: ProductId, permission: ProductPermission)] = []
    private(set) var promptBatchedCalls: [(productId: ProductId, permissions: [ProductPermission])] = []

    func prompt(
        productId: ProductId,
        permission: ProductPermission
    ) async throws -> PermissionDecision {
        promptCalls.append((productId, permission))
        if let error { throw error }
        return decision
    }

    func promptBatched(
        productId: ProductId,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        promptBatchedCalls.append((productId, permissions))
        if let error { throw error }
        return decision
    }
}
