import Foundation

public protocol ProductPermissionGuarding: Sendable {
    /// Requests the permission from the user. Returns `true` immediately if
    /// already granted. Otherwise prompts for `AllowAlways`, `AllowOnce`, or
    /// `Deny`. One-time grants issued here can later be consumed by
    /// ``consumePermission(productId:permission:)``.
    func requestPermission(productId: String, permission: ProductPermission) async throws -> Bool

    /// Requests multiple permissions in a single batched prompt. Filters out
    /// already-granted permissions, prompts for the rest, and returns `true`
    /// only if all are granted.
    func requestPermissionsBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> Bool

    func requestDevicePermissionDecision(
        productId: String,
        capability: DeviceCapabilityType
    ) async throws -> PermissionDecision

    func requestPermissionsDecision(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision

    /// Consumes a previously-issued permission. Falls back to
    /// ``requestPermission(productId:permission:)`` if permission wasn't granted.
    func consumePermission(productId: String, permission: ProductPermission) async throws -> Bool

    /// Read-only permission check. Does not prompt the user.
    func check(productId: String, permission: ProductPermission) async throws -> Bool
}

/// Dispatches permission requests to the matching handler.
public final class ProductPermissionGuard: ProductPermissionGuarding, @unchecked Sendable {
    private let networkHandler: NetworkAccessPermissionHandler
    private let remoteHandler: RemotePermissionHandler
    private let deviceHandler: DeviceCapabilityPermissionHandler
    private let accountHandler: AccountAccessPermissionHandler
    private let repository: ProductPermissionRepositoryProtocol
    private let requester: ProductPermissionRequesting

    public init(
        networkHandler: NetworkAccessPermissionHandler,
        remoteHandler: RemotePermissionHandler,
        deviceHandler: DeviceCapabilityPermissionHandler,
        accountHandler: AccountAccessPermissionHandler,
        repository: ProductPermissionRepositoryProtocol,
        requester: ProductPermissionRequesting
    ) {
        self.networkHandler = networkHandler
        self.remoteHandler = remoteHandler
        self.deviceHandler = deviceHandler
        self.accountHandler = accountHandler
        self.repository = repository
        self.requester = requester
    }

    public func requestPermission(
        productId: String,
        permission: ProductPermission
    ) async throws -> Bool {
        switch permission {
        case let .networkAccess(domain):
            try await networkHandler.request(productId: productId, domain: domain)
        case let .accountAccess(targetProductId):
            try await accountHandler.request(
                productId: productId,
                targetProductId: targetProductId
            )
        case let .deviceCapability(capability):
            try await deviceHandler.request(productId: productId, capability: capability)
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess:
            try await remoteHandler.request(productId: productId, permission: permission)
        }
    }

    public func requestPermissionsBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> Bool {
        guard !permissions.isEmpty else { return true }

        var notYetGranted: [ProductPermission] = []
        for permission in permissions {
            let granted = try await check(productId: productId, permission: permission)
            if !granted {
                notYetGranted.append(permission)
            }
        }

        let unique = notYetGranted.removingDuplicates()
        guard !unique.isEmpty else { return true }

        let decision = await requester.promptBatched(
            productId: productId,
            permissions: unique
        )

        switch decision {
        case .allowAlways:
            for permission in unique {
                try await repository.grant(productId: productId, permission: permission)
            }
            return true
        case .allowOnce:
            for permission in unique {
                repository.grantOneTime(productId: productId, permission: permission)
            }
            return true
        case .deny:
            return false
        }
    }

    public func requestDevicePermissionDecision(
        productId: String,
        capability: DeviceCapabilityType
    ) async throws -> PermissionDecision {
        try await deviceHandler.requestDecision(productId: productId, capability: capability)
    }

    public func requestPermissionsDecision(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        var undecided: [ProductPermission] = []
        for permission in permissions.removingDuplicates() {
            let state = try await repository.getPermissionState(productId: productId, permission: permission)
            if state != .allowedOnce, try await check(productId: productId, permission: permission) {
                continue
            }
            undecided.append(permission)
        }
        guard !undecided.isEmpty else { return .allowAlways }
        return await requester.promptBatched(productId: productId, permissions: undecided)
    }

    public func consumePermission(
        productId: String,
        permission: ProductPermission
    ) async throws -> Bool {
        if try await check(productId: productId, permission: permission) {
            _ = repository.consumeOneTimeGrant(productId: productId, permission: permission)

            return true
        }

        let granted = try await requestPermission(productId: productId, permission: permission)

        if granted {
            _ = repository.consumeOneTimeGrant(productId: productId, permission: permission)
        }

        return granted
    }

    public func check(
        productId: String,
        permission: ProductPermission
    ) async throws -> Bool {
        switch permission {
        case let .networkAccess(domain):
            try await networkHandler.isGranted(productId: productId, domain: domain)
        case let .accountAccess(targetProductId):
            try await accountHandler.isGranted(
                productId: productId,
                targetProductId: targetProductId
            )
        case let .deviceCapability(capability):
            try await deviceHandler.isGranted(productId: productId, capability: capability)
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess:
            try await remoteHandler.isGranted(productId: productId, permission: permission)
        }
    }
}

private extension Array where Element: Equatable {
    func removingDuplicates() -> [Element] {
        var result: [Element] = []
        for element in self where !result.contains(element) {
            result.append(element)
        }
        return result
    }
}
