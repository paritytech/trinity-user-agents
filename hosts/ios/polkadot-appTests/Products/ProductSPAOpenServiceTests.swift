import Foundation
import Products
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct ProductSPAOpenServiceTests {
    @Test(arguments: [DeeplinkTestSchemes.active, DeeplinkTestSchemes.other, "https", "polkadot"])
    func opensSupportedProductLinksWithoutLosingTheirRoute(_ scheme: String) async throws {
        let (pages, continuation) = AsyncStream<ProductPage>.makeStream()
        let navigator = ProductLinkNavigator { continuation.yield($0) }
        let service = makeService(navigator: navigator)
        let url = try #require(URL(string: "\(scheme)://app.arena.browse.dot/page?ref=abc#section"))

        try #require(service.handle(url: url))
        var opened = pages.makeAsyncIterator()
        let page = try #require(await opened.next())

        #expect(page.host.toDotDomain().rawValue == "app.arena.browse.dot")
        #expect(page.page == "/page?ref=abc#section")
    }

    @Test(arguments: [DeeplinkTestSchemes.active, DeeplinkTestSchemes.other, "https"])
    func rejectsOtherNetworks(_ scheme: String) throws {
        let service = makeService(navigator: ProductLinkNavigator { _ in Issue.record("Opened an off-network product") })
        let url = try #require(URL(string: "\(scheme)://browse.paseo.li/page?ref=abc#section"))

        #expect(!service.handle(url: url))
    }

    @Test func rejectsForeignSchemes() throws {
        let service = makeService(navigator: ProductLinkNavigator { _ in Issue.record("Opened a foreign scheme") })
        #expect(!service.handle(url: try #require(URL(string: "foreignapp://browse.dot/page"))))
    }

    private func makeService(navigator: ProductLinkNavigator) -> ProductSPAOpenService {
        ProductSPAOpenService(
            moduleNavigator: navigator,
            hostProvider: ProductHostFactory(tldProvider: ProductLinkTldProvider())
        )
    }
}

private struct ProductLinkTldProvider: DotNsTldProviding {
    func currentTld() -> String? { "dot" }
    func resolveTld() async throws -> String { "dot" }
    func refresh() {}
    func reset() {}
}

@MainActor
private final class ProductLinkNavigator: ModuleNavigating {
    private let onOpen: (ProductPage) -> Void

    init(onOpen: @escaping (ProductPage) -> Void) {
        self.onOpen = onOpen
    }

    func openProduct(page: ProductPage) { onOpen(page) }
    func openChat(_: ChatOpenModel) {}
    func presentModally(_: UIViewController) {}
    func presentFullScreen(_: UIViewController) {}
    func presentOverFullScreen(_: UIViewController, animatedBy _: UIViewControllerTransitioningDelegate) {}
    func presentProduct(page _: ProductPage) {}
    func openPocket() {}
    func openScanPanel() {}
}
