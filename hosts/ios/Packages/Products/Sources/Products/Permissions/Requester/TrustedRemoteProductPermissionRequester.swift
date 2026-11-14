import Foundation

/// Grants remote access and notification app consent without prompting to
/// products the host trusts. Notification OS authorization stays in the handler.
///
/// The core already grants a first-party product every `RemotePermission`
/// without prompting, but only along the path it owns: the request a product
/// makes through the protocol. This app also mediates product network access in
/// its own code — the container's `fetch` shim reaches
/// `NetworkAccessPermissionHandler`, which never enters the core — so without
/// this the same product is stopped here for access the core would have granted.
///
/// Distinct from `AutoAllowProductPermissionRequester`, which grants *every*
/// permission and is scoped to builds with no Apps settings screen. This one is
/// narrower on both axes and is not tied to that build flag: it covers remote
/// access and notification app consent. Other device capabilities, account
/// access, balance and identity disclosure keep prompting for a trusted product.
/// Stored refusals are resolved before this requester; it never overrides them.
public struct TrustedRemoteProductPermissionRequester: ProductPermissionRequesting {
    private let isTrustedForRemoteAccess: @Sendable (String) -> Bool
    private let wrapped: ProductPermissionRequesting

    public init(
        isTrustedForRemoteAccess: @escaping @Sendable (String) -> Bool,
        wrapped: ProductPermissionRequesting
    ) {
        self.isTrustedForRemoteAccess = isTrustedForRemoteAccess
        self.wrapped = wrapped
    }

    public func prompt(
        productId: String,
        permission: ProductPermission
    ) async -> PermissionDecision {
        if permission == .deviceCapability(.notifications), isTrustedForRemoteAccess(productId) {
            return .allowAlways
        }

        guard !grantsWithoutPrompting(productId: productId, permissions: [permission]) else {
            return .allowAlways
        }

        return await wrapped.prompt(productId: productId, permission: permission)
    }

    public func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async -> PermissionDecision {
        guard !grantsWithoutPrompting(productId: productId, permissions: permissions) else {
            return .allowAlways
        }

        return await wrapped.promptBatched(productId: productId, permissions: permissions)
    }
}

private extension TrustedRemoteProductPermissionRequester {
    /// A batch is granted only when every permission in it is remote access.
    ///
    /// One decision answers the whole batch, so a batch mixing remote access
    /// with a device capability has to reach the user: granting it here would
    /// hand over the camera on the strength of a network grant. An empty batch
    /// asks for nothing and is not something to grant.
    func grantsWithoutPrompting(productId: String, permissions: [ProductPermission]) -> Bool {
        guard !permissions.isEmpty, permissions.allSatisfy(\.isRemoteAccess) else { return false }

        return isTrustedForRemoteAccess(productId)
    }
}
