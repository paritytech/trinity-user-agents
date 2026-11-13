import Foundation
import Products

protocol WebDeeplinkNormalizing {
    func appSchemeDeeplink(from url: URL) -> URL?
}

/// Rewrites a bare share-host web link of the network to the app scheme deeplink, taking
/// the action from the first path segment (`https://dot.li/pay?x=1` -> `polkadotapp://pay?x=1`).
struct WebDeeplinkNormalizer: WebDeeplinkNormalizing {
    private let appScheme: String
    private let tldProvider: DotNsTldProviding
    private let webSchemes: Set<String> = ["https", "http"]

    init(
        appScheme: String = AppConfig.DeepLink.scheme,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.appScheme = appScheme
        self.tldProvider = tldProvider
    }

    func appSchemeDeeplink(from url: URL) -> URL? {
        // Which share hosts are this app's depends on the network, so none is rewritten before its TLD is known.
        guard
            let tld = tldProvider.currentTld(),
            let scheme = url.scheme?.lowercased(),
            webSchemes.contains(scheme),
            let host = url.host()?.lowercased(),
            ProductHost.shareHosts(tld: tld).contains(host),
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
