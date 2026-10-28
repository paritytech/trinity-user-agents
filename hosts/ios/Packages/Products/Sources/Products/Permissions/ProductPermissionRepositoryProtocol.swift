import struct TrUAPIHost.ProductId
import Foundation

/// Persists product permission grants and tracks in-memory one-time grants.
public protocol ProductPermissionRepositoryProtocol: Sendable {
    func getPermissionState(
        productId: ProductId,
        permission: ProductPermission
    ) async throws -> ProductPermissionState

    func isAnyAlwaysGranted(
        productId: ProductId,
        typeName: String,
        keys: [String]
    ) async throws -> Bool

    func grant(productId: ProductId, permission: ProductPermission) async throws

    func grantOneTime(productId: ProductId, permission: ProductPermission)

    func consumeOneTimeGrant(productId: ProductId, permission: ProductPermission) -> Bool

    func revoke(productId: ProductId, permission: ProductPermission) async throws

    func revoke(productId: ProductId, permissions: [ProductPermission]) async throws

    func revokeAllByProduct(productId: ProductId) async throws

    func deny(productId: ProductId, permission: ProductPermission) async throws

    func getAllByProduct(productId: ProductId) async throws -> [ProductPermissionGrant]
}
