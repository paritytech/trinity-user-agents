import Foundation

/// Grants every permission without prompting for an allowlisted set of products.
/// Wraps the real requester for everything else.
///
/// Intended for builds that ship no Apps settings screen: there a prompt would
/// record a grant the user can never review or revoke.
public struct AutoAllowProductPermissionRequester: ProductPermissionRequesting {
    private let allowedLabels: Set<String>
    private let wrapped: ProductPermissionRequesting

    public init(allowedLabels: Set<String>, wrapped: ProductPermissionRequesting) {
        self.allowedLabels = allowedLabels
        self.wrapped = wrapped
    }

    public func prompt(
        productId: String,
        permission: ProductPermission
    ) async -> PermissionDecision {
        guard !isAutoAllowed(productId: productId) else { return .allowAlways }

        return await wrapped.prompt(productId: productId, permission: permission)
    }

    public func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async -> PermissionDecision {
        guard !isAutoAllowed(productId: productId) else { return .allowAlways }

        return await wrapped.promptBatched(productId: productId, permissions: permissions)
    }
}

private extension AutoAllowProductPermissionRequester {
    /// Matches the label only, dropping the root: the root is the chain TLD and
    /// differs per network, and resolving it here would put a network call in
    /// front of an auto-allow.
    func isAutoAllowed(productId: String) -> Bool {
        guard let name = ProductHost.name(fromDotDomain: productId) else { return false }

        return allowedLabels.contains(name)
    }
}
