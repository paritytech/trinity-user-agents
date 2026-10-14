import Foundation

/// Handles simple remote permissions (`webRtcAccess`, `chainSubmitAccess`,
/// `preimageSubmitAccess`, `statementSubmitAccess`) with a check-or-prompt
/// pattern. Network-access permissions are routed to
/// ``NetworkAccessPermissionHandler`` instead.
public final class RemotePermissionHandler: Sendable {
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
        permission: ProductPermission
    ) async throws -> Bool {
        let state = try await repository.getPermissionState(
            productId: productId,
            permission: permission
        )
        return state.isAllowed
    }

    public func request(
        productId: String,
        permission: ProductPermission
    ) async throws -> Bool {
        let state = try await repository.getPermissionState(
            productId: productId,
            permission: permission
        )

        switch state {
        case .allowedAlways,
             .allowedOnce:
            return true
        case .denied:
            return false
        case .notDetermined:
            return try await promptPermission(
                productId: productId,
                permission: permission
            )
        }
    }
}

private extension RemotePermissionHandler {
    func promptPermission(
        productId: String,
        permission: ProductPermission
    ) async throws -> Bool {
        switch await requester.prompt(productId: productId, permission: permission) {
        case .allowAlways:
            try await repository.grant(productId: productId, permission: permission)
            return true
        case .allowOnce:
            repository.grantOneTime(productId: productId, permission: permission)
            return true
        case .deny:
            try await repository.deny(productId: productId, permission: permission)
            return false
        }
    }
}
