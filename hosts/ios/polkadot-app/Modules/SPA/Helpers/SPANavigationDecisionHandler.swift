import WebKit
import Products

enum SPANavigationDecision: Equatable {
    case allow
    case intercept(URL)
    case openDeeplink(URL)
}

protocol SPANavigationDecisionHandling {
    func decide(for navigationAction: WKNavigationAction) -> SPANavigationDecision
}

/// Debug direct-URL mode: allow same-host navigation, intercept everything else.
final class DirectURLNavigationDecisionHandler: SPANavigationDecisionHandling {
    private let baseURL: URL
    private let deeplinkMatcher: AppDeeplinkMatching

    init(baseURL: URL, deeplinkMatcher: AppDeeplinkMatching = AppDeeplinkMatcher()) {
        self.baseURL = baseURL
        self.deeplinkMatcher = deeplinkMatcher
    }

    func decide(for navigationAction: WKNavigationAction) -> SPANavigationDecision {
        guard let url = navigationAction.request.url else { return .allow }
        return decide(url: url)
    }

    func decide(url: URL) -> SPANavigationDecision {
        if deeplinkMatcher.isAppDeeplink(url) {
            return .openDeeplink(url)
        }

        if url.host == baseURL.host, url.port == baseURL.port {
            return .allow
        }
        return .intercept(url)
    }
}

final class DotNsNavigationDecisionHandler: SPANavigationDecisionHandling {
    private let baseHost: ProductHost
    private let hostProvider: ProductHostProviding
    private let deeplinkMatcher: AppDeeplinkMatching

    init(
        baseHost: ProductHost,
        hostProvider: ProductHostProviding,
        deeplinkMatcher: AppDeeplinkMatching = AppDeeplinkMatcher()
    ) {
        self.baseHost = baseHost
        self.hostProvider = hostProvider
        self.deeplinkMatcher = deeplinkMatcher
    }

    func decide(for navigationAction: WKNavigationAction) -> SPANavigationDecision {
        guard let url = navigationAction.request.url else { return .allow }
        return decide(url: url)
    }

    func decide(url: URL) -> SPANavigationDecision {
        if deeplinkMatcher.isAppDeeplink(url) {
            return .openDeeplink(url)
        }

        guard let host = hostProvider.host(url: url) else {
            return .allow
        }

        if host.toDotDomain() != baseHost.toDotDomain() {
            return .intercept(url)
        }

        return .allow
    }
}
