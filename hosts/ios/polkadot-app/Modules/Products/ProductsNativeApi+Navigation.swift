import Foundation
import Products

// MARK: - Navigation & Network Access

extension ProductsNativeApi {
    func navigateTo(destination: String) async throws {
        switch navigationTargetResolver.resolve(destination: destination) {
        case let .productPage(page):
            try await navigationRouter.navigateTo(destination: page)
        case let .appDeeplink(url):
            try await navigationRouter.openExternalURL(url)
        case let .externalURL(url):
            guard try await permissionGuard.consumePermission(
                productId: productId,
                permission: .deviceCapability(.openUrl)
            ) else {
                throw ProductNativeApiError.permissionDenied
            }

            try await navigationRouter.openExternalURL(url)
        case .unsupported:
            return
        }
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
