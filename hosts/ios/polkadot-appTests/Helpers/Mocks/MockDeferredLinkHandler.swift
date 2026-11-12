import Foundation
@testable import polkadot_app

final class MockDeferredLinkHandler: DeferredLinkHandling {
    private(set) var handledURLs: [URL] = []

    func handle(with url: URL) {
        handledURLs.append(url)
    }

    func register(_: URLHandlingServiceProtocol) {}
}
