import Foundation
import Products

protocol WebDeeplinkNormalizing {
    func appSchemeDeeplink(from url: URL) -> URL?
}

/// Rewrites a bare share-host web link to the app scheme deeplink, taking the action
/// from the first path segment (`https://dot.li/pay?x=1` -> `polkadotapp://pay?x=1`).
struct WebDeeplinkNormalizer: WebDeeplinkNormalizing {
    private let appScheme: String
    private let webHosts: Set<String>
    private let webSchemes: Set<String> = ["https", "http"]

    init(
        appScheme: String = AppConfig.DeepLink.scheme,
        webHosts: Set<String> = ProductHost.shareHosts
    ) {
        self.appScheme = appScheme
        self.webHosts = webHosts
    }

    func appSchemeDeeplink(from url: URL) -> URL? {
        guard
            let scheme = url.scheme?.lowercased(),
            webSchemes.contains(scheme),
            let host = url.host()?.lowercased(),
            webHosts.contains(host),
            var components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else {
            return nil
        }

        let segments = components.path.split(separator: "/").map(String.init)

        guard let action = segments.first else {
            return nil
        }

        let rest = segments.dropFirst()

        components.scheme = appScheme
        components.host = action
        components.path = rest.isEmpty ? "" : "/" + rest.joined(separator: "/")

        return components.url
    }
}
