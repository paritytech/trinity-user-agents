import Foundation

protocol AppDeeplinkMatching: Sendable {
    func isAppDeeplink(_ url: URL) -> Bool
}

/// Recognises links addressed to this app (any build flavor's scheme), so web content
/// can hand them to the native deep-link handler instead of loading them.
struct AppDeeplinkMatcher: AppDeeplinkMatching {
    private let isKnownScheme: @Sendable (String) -> Bool

    init(isKnownScheme: @escaping @Sendable (String) -> Bool = { AppConfig.DeepLink.isKnownScheme($0) }) {
        self.isKnownScheme = isKnownScheme
    }

    func isAppDeeplink(_ url: URL) -> Bool {
        guard let scheme = url.scheme else {
            return false
        }

        return isKnownScheme(scheme)
    }
}
