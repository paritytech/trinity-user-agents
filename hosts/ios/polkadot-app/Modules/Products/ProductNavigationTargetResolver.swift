import Foundation
import Products

enum ProductNavigationTarget {
    case productPage(ProductPage)
    case appDeeplink(URL)
    case externalURL(URL)
    case unsupported
}

protocol ProductNavigationTargetResolving {
    func resolve(destination: String) -> ProductNavigationTarget
}

/// Classifies a product's `navigateTo` destination. External links are allowlisted so a product
/// can't fire arbitrary-scheme URLs at the system; this app's own deep links stay in-app.
struct ProductNavigationTargetResolver: ProductNavigationTargetResolving {
    // Web plus schemes that only open a system app prefilled (mail / dialer / messages / maps) — the user still
    // confirms any action.
    private static let allowedExternalSchemes: Set<String> = ["http", "https", "mailto", "tel", "sms", "maps"]

    private let hostProvider: ProductHostProviding
    private let deeplinkMatcher: AppDeeplinkMatching

    init(hostProvider: ProductHostProviding, deeplinkMatcher: AppDeeplinkMatching = AppDeeplinkMatcher()) {
        self.hostProvider = hostProvider
        self.deeplinkMatcher = deeplinkMatcher
    }

    func resolve(destination: String) -> ProductNavigationTarget {
        if let page = hostProvider.page(navigationDestination: destination) {
            return .productPage(page)
        }

        guard let url = URL(string: destination), let scheme = url.scheme?.lowercased() else {
            return .unsupported
        }

        if deeplinkMatcher.isAppDeeplink(url) {
            return .appDeeplink(url)
        }

        return Self.allowedExternalSchemes.contains(scheme) ? .externalURL(url) : .unsupported
    }
}
