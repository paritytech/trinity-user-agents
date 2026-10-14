import Foundation

/// Handles `.networkAccess` permissions: domain allow-listing, subdomain
/// matching, and one-time grants.
public final class NetworkAccessPermissionHandler: Sendable {
    // TODO: Revisit for release
    private static let allowedDomains: Set<String> = [
        "fonts.googleapis.com",
        "fonts.gstatic.com"
    ]

    private let repository: ProductPermissionRepositoryProtocol
    private let requester: ProductPermissionRequesting

    public init(
        repository: ProductPermissionRepositoryProtocol,
        requester: ProductPermissionRequesting
    ) {
        self.repository = repository
        self.requester = requester
    }

    public func isGranted(productId: String, domain: String) async throws -> Bool {
        try await getPermissionState(productId: productId, domain: domain).isAllowed
    }

    public func request(productId: String, domain: String) async throws -> Bool {
        let permissionState = try await getPermissionState(productId: productId, domain: domain)

        switch permissionState {
        case .allowedAlways,
             .allowedOnce:
            return true
        case .denied:
            return false
        case .notDetermined:
            return try await promptPermission(productId: productId, domain: domain)
        }
    }
}

private extension NetworkAccessPermissionHandler {
    func getPermissionState(productId: String, domain: String) async throws -> ProductPermissionState {
        let permission = ProductPermission.networkAccess(domain: domain)

        let permissionState = try await repository.getPermissionState(
            productId: productId,
            permission: permission
        )

        guard permissionState == .notDetermined else {
            return permissionState
        }

        let allowedDomainOrSubdomain = try await checkAllowedDomains(productId: productId, domain: domain)

        if allowedDomainOrSubdomain {
            return .allowedAlways
        } else {
            return .notDetermined
        }
    }

    func checkAllowedDomains(productId: String, domain: String) async throws -> Bool {
        if Self.allowedDomains.contains(domain) { return true }

        let candidates = Self.generateDomainCandidates(for: domain)
        return try await repository.isAnyAlwaysGranted(
            productId: productId,
            typeName: ProductPermission.networkAccessTypeName,
            keys: candidates
        )
    }

    func promptPermission(productId: String, domain: String) async throws -> Bool {
        let permission = ProductPermission.networkAccess(domain: domain)

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

extension NetworkAccessPermissionHandler {
    /// Generates stored-pattern candidates that would grant access to a domain.
    /// Includes: exact match, wildcard subdomain patterns (`*.parent`), and
    /// universal wildcard (`*`).
    ///
    /// Bare parent domains are NOT included: access to `a.example.com` must be
    /// granted explicitly either for `a.example.com` or via the wildcard
    /// `*.example.com`. A grant for `example.com` alone does not imply access
    /// to its subdomains.
    ///
    /// `"deep.api.example.com"` →
    /// `["deep.api.example.com", "*.api.example.com", "*.example.com", "*"]`
    static func generateDomainCandidates(for domain: String) -> [String] {
        var candidates = [domain]

        let parts = domain.split(separator: ".").map(String.init)
        for index in 1 ..< max(parts.count - 1, 1) {
            let parent = parts.dropFirst(index).joined(separator: ".")
            candidates.append("*.\(parent)")
        }

        candidates.append("*")
        return candidates
    }

    /// Extracts the host from a URL string, returning `nil` on parse failure.
    static func extractDomain(from urlString: String) -> String? {
        URL(string: urlString)?.host
    }
}
