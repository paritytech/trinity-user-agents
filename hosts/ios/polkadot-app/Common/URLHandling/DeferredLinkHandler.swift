import Foundation

protocol DeferredLinkHandling {
    /// A link arriving now; the handler reads its arrival.
    func handle(with url: URL)
    /// A link whose arrival was read earlier, where the system handed it over.
    func handle(with url: URL, arrival: LinkArrival)
    func register(_ handler: URLHandlingServiceProtocol)
}

final class DeferredLinkHandler {
    static let shared = DeferredLinkHandler()

    private let webDeeplinkNormalizer: WebDeeplinkNormalizing
    private let arrivalClock: LinkArrivalClock
    private weak var handler: URLHandlingServiceProtocol?
    /// Kept with its arrival, so a link that waits for the handler is still timed from when it came in.
    private var pending: (url: URL, arrival: LinkArrival)?

    init(
        webDeeplinkNormalizer: WebDeeplinkNormalizing = WebDeeplinkNormalizer(),
        arrivalClock: LinkArrivalClock = SystemLinkArrivalClock()
    ) {
        self.webDeeplinkNormalizer = webDeeplinkNormalizer
        self.arrivalClock = arrivalClock
    }
}

extension DeferredLinkHandler: DeferredLinkHandling {
    func handle(with url: URL) {
        handle(with: url, arrival: arrivalClock.read())
    }

    func handle(with url: URL, arrival: LinkArrival) {
        pending = (url, arrival)
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
        guard let handler, let pending else {
            return
        }

        let candidates = [
            webDeeplinkNormalizer.appSchemeDeeplink(from: pending.url),
            pending.url
        ].compactMap { $0 }

        _ = candidates.first { handler.handle(url: $0, arrival: pending.arrival) }
        self.pending = nil
    }
}
