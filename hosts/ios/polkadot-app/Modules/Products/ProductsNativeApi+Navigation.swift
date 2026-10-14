import Foundation
import Products

// MARK: - Navigation & Network Access

extension ProductsNativeApi {
    // Web plus schemes that only open a system app prefilled (mail / dialer / messages / maps) — the user still
    // confirms any action.
    private static let allowedExternalSchemes: Set<String> = ["http", "https", "mailto", "tel", "sms", "maps"]

    func navigateTo(destination: String) async throws {
        if let destinationPage = hostProvider.page(navigationDestination: destination) {
            try await navigationRouter.navigateTo(destination: destinationPage)
            return
        }

        // Non-.dot destinations: open external links in the matching system app.
        // Allowlisted so a product can't fire arbitrary-scheme URLs (e.g. custom deep links) at the system.
        guard
            let url = URL(string: destination),
            let scheme = url.scheme?.lowercased(),
            Self.allowedExternalSchemes.contains(scheme)
        else {
            return
        }

        guard try await permissionGuard.consumePermission(
            productId: productId,
            permission: .deviceCapability(.openUrl)
        ) else {
            throw ProductNativeApiError.permissionDenied
        }

        try await navigationRouter.openExternalURL(url)
    }

    func allowNetworkAccess(url: String) async throws -> Bool {
        guard let parsed = URL(string: url) else {
            return false
        }

        // Same-product short-circuit: a product can always access its own assets.
        if let requestedProduct = hostProvider.host(url: parsed)?.toDotDomain(), requestedProduct == productId {
            return true
        }

        // Otherwise consult a previously-granted network permission for the host.
        guard let host = parsed.host else {
            // if url is relative then allow as it is still current host
            return true
        }

        let permission = ProductPermission.networkAccess(domain: host)
        return try await permissionGuard.consumePermission(productId: productId, permission: permission)
    }
}
