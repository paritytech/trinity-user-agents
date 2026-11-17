import Foundation
import Products
import TrUAPIHost

protocol WebDeeplinkNormalizing {
    func appSchemeDeeplink(from url: URL) -> URL?
}

/// Rewrites a bare share-host web link of the stack to the app scheme deeplink, taking
/// the action from the first path segment (`https://dot.li/pay?x=1` -> `polkadotapp://pay?x=1`).
struct WebDeeplinkNormalizer: WebDeeplinkNormalizing {
    private let appScheme: String
    private let shareHosts: Set<String>
    private let webSchemes: Set<String> = ["https", "http"]

    init(
        appScheme: String = AppConfig.DeepLink.scheme,
        stack: Stack? = TrUAPIBundledStack.opened
    ) {
        self.appScheme = appScheme
        shareHosts = Set(stack?.productShareHosts() ?? [])
    }

    func appSchemeDeeplink(from url: URL) -> URL? {
        guard
            let scheme = url.scheme?.lowercased(),
            webSchemes.contains(scheme),
            let host = url.host()?.lowercased(),
            shareHosts.contains(host),
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
