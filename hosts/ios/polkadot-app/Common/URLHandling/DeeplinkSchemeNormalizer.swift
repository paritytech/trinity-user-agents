import Foundation

protocol DeeplinkSchemeNormalizing {
    func normalize(_ url: URL) -> URL
}

/// Rewrites known app deeplink schemes to the active build scheme so links generated
/// for one build flavor open in the other (e.g. a prod desktop pairing QR on a dev build).
struct DeeplinkSchemeNormalizer: DeeplinkSchemeNormalizing {
    private let activeScheme: String
    private let isKnownScheme: (String) -> Bool

    init(
        activeScheme: String = AppConfig.DeepLink.scheme,
        isKnownScheme: @escaping (String) -> Bool = AppConfig.DeepLink.isKnownScheme
    ) {
        self.activeScheme = activeScheme
        self.isKnownScheme = isKnownScheme
    }

    func normalize(_ url: URL) -> URL {
        guard
            let scheme = url.scheme?.lowercased(),
            isKnownScheme(scheme),
            scheme != activeScheme,
            var components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else {
            return url
        }

        components.scheme = activeScheme
        return components.url ?? url
    }
}
