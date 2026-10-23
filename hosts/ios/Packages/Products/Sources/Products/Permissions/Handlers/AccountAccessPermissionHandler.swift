import Foundation

/// Seam over account-access permission requests so consumers stay testable.
public protocol AccountAccessPermissionHandling: Sendable {
    func request(productId: String, targetProductId: String) async throws -> Bool
}

/// Handles `.accountAccess` permissions: same-product auto-grant, otherwise
/// prompt.
public final class AccountAccessPermissionHandler: AccountAccessPermissionHandling, Sendable {
    private let repository: ProductPermissionRepositoryProtocol
    private let requester: ProductPermissionRequesting

    public init(
        repository: ProductPermissionRepositoryProtocol,
        requester: ProductPermissionRequesting
    ) {
        self.repository = repository
        self.requester = requester
    }

    public func isGranted(
        productId: String,
        targetProductId: String
    ) async throws -> Bool {
        let state = try await getPermissionState(
            productId: productId,
            targetProductId: targetProductId
        )

        return state.isAllowed
    }

    public func request(productId: String, targetProductId: String) async throws -> Bool {
        let state = try await getPermissionState(productId: productId, targetProductId: targetProductId)

        switch state {
        case .allowedOnce,
             .allowedAlways:
            return true
        case .denied:
            return false
        case .notDetermined:
            return try await promptPermission(productId: productId, targetProductId: targetProductId)
        }
    }
}

private extension AccountAccessPermissionHandler {
    func getPermissionState(
        productId: String,
        targetProductId: String
    ) async throws -> ProductPermissionState {
        if targetProductId == productId { return .allowedAlways }

        return try await repository.getPermissionState(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId)
        )
    }

    func promptPermission(productId: String, targetProductId: String) async throws -> Bool {
        let permission = ProductPermission.accountAccess(targetProductId: targetProductId)

        return try await repository.promptPermission(
            productId: productId, permission: permission, requester: requester
        )
    }
}
