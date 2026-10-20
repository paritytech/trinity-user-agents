import Foundation

protocol AppDeeplinkMatching: Sendable {
    func isAppDeeplink(_ url: URL) -> Bool
}

/// Recognises links addressed to this app, so web content and products can hand them to the
/// native deep-link handler instead of loading them: any build flavor's scheme, and the core's
/// `polkadot://pocket`, which names the app's Pocket rather than a product.
struct AppDeeplinkMatcher: AppDeeplinkMatching {
    private let isKnownScheme: @Sendable (String) -> Bool

    init(isKnownScheme: @escaping @Sendable (String) -> Bool = { AppConfig.DeepLink.isKnownScheme($0) }) {
        self.isKnownScheme = isKnownScheme
    }

    func isAppDeeplink(_ url: URL) -> Bool {
        guard let scheme = url.scheme else {
            return false
        }

        return isKnownScheme(scheme) || isCorePocketLink(url)
    }
}

private extension AppDeeplinkMatcher {
    func isCorePocketLink(_ url: URL) -> Bool {
        url.scheme?.lowercased() == PocketDeeplinkParser.coreScheme
            && url.host() == PocketDeeplinkParser.host
    }
}
