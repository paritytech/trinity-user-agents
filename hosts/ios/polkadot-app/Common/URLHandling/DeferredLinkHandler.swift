import Foundation

protocol DeferredLinkHandling {
    func handle(with url: URL)
    func register(_ handler: URLHandlingServiceProtocol)
}

final class DeferredLinkHandler {
    static let shared = DeferredLinkHandler()

    private let webDeeplinkNormalizer: WebDeeplinkNormalizing
    private weak var handler: URLHandlingServiceProtocol?
    private var deeplinkURL: URL?

    init(webDeeplinkNormalizer: WebDeeplinkNormalizing = WebDeeplinkNormalizer()) {
        self.webDeeplinkNormalizer = webDeeplinkNormalizer
    }
}

extension DeferredLinkHandler: DeferredLinkHandling {
    func handle(with url: URL) {
        deeplinkURL = url
        handlePendingUrl()
    }

    func register(_ handler: URLHandlingServiceProtocol) {
        guard self.handler !== handler else {
            return
        }
        self.handler = handler
        handlePendingUrl()
    }
}

private extension DeferredLinkHandler {
    func handlePendingUrl() {
        guard let handler, let deeplinkURL else {
            return
        }

        let candidates = [
            webDeeplinkNormalizer.appSchemeDeeplink(from: deeplinkURL),
            deeplinkURL
        ].compactMap { $0 }

        _ = candidates.first { handler.handle(url: $0) }
        self.deeplinkURL = nil
    }
}
