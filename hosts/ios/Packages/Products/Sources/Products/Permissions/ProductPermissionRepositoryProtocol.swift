import Foundation

/// Persists product permission grants and tracks in-memory one-time grants.
public protocol ProductPermissionRepositoryProtocol: Sendable {
    func getPermissionState(
        productId: String,
        permission: ProductPermission
    ) async throws -> ProductPermissionState

    func isAnyAlwaysGranted(
        productId: String,
        typeName: String,
        keys: [String]
    ) async throws -> Bool

    func grant(productId: String, permission: ProductPermission) async throws

    func grantOneTime(productId: String, permission: ProductPermission)

    func consumeOneTimeGrant(productId: String, permission: ProductPermission) -> Bool

    func revoke(productId: String, permission: ProductPermission) async throws

    func revoke(productId: String, permissions: [ProductPermission]) async throws

    func revokeAllByProduct(productId: String) async throws

    func deny(productId: String, permission: ProductPermission) async throws

    func getAllByProduct(productId: String) async throws -> [ProductPermissionGrant]

    /// Captures revocation state before presenting and refuses stale decisions.
    func promptPermissions(
        productId: String,
        permissions: [ProductPermission],
        requester: ProductPermissionRequesting
    ) async throws -> Bool
}

public extension ProductPermissionRepositoryProtocol {
    func promptPermission(
        productId: String,
        permission: ProductPermission,
        requester: ProductPermissionRequesting
    ) async throws -> Bool {
        try await promptPermissions(productId: productId, permissions: [permission], requester: requester)
    }
}
