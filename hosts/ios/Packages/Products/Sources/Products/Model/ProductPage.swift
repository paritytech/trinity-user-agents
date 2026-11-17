import Foundation
import TrUAPIHost

public struct ProductPage: Sendable {
    public let host: ProductHost
    public let page: String?

    public init(host: ProductHost, page: String? = nil) {
        self.host = host
        self.page = page
    }
}

extension ProductPage {
    static func fromUrl(_ url: URL, tld: String) -> ProductPage? {
        fromNavigationDestination(url.absoluteString, tld: tld)
    }

    static func fromNavigationDestination(_ dest: String, tld: String) -> ProductPage? {
        guard let link = parseProductLink(input: dest, networkSuffix: tld) else { return nil }
        return ProductPage(
            host: ProductHost(link: link),
            page: link.path == "/" ? nil : link.path
        )
    }

    /// The page's URL on the product `url` belongs to: its origin plus the page route, or the
    /// product root when there is no route. The root serves the entry file, so a page without a
    /// route always lands on the product's start no matter where `url` currently points.
    public func applied(to url: URL) -> URL {
        guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            return url
        }

        components.path = ""
        components.query = nil
        components.fragment = nil

        guard let hostURL = components.string else {
            return url
        }

        let route = page.map { $0.hasPrefix("/") ? $0 : "/" + $0 } ?? "/"

        return URL(string: hostURL + route) ?? url
    }
}
