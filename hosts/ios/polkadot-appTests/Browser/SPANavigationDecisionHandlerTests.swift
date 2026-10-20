import Foundation
import Products
import Testing
@testable import polkadot_app

@Suite("SPANavigationDecisionHandler Tests")
struct SPANavigationDecisionHandlerTests {
    private let hostProvider = ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))

    private func makeDotNsHandler() throws -> DotNsNavigationDecisionHandler {
        let baseHost = try #require(hostProvider.host(rawString: "shop.dot"))
        return DotNsNavigationDecisionHandler(baseHost: baseHost, hostProvider: hostProvider)
    }

    private func makePayDeeplink() throws -> URL {
        try #require(URL(string: "\(DeeplinkTestSchemes.active)://pay?address=5GrwvaEF&amount=100"))
    }

    @Test("dotNS mode hands the app's deep link to the native handler")
    func dotNsOpensDeeplink() throws {
        let handler = try makeDotNsHandler()
        let url = try makePayDeeplink()

        #expect(handler.decide(url: url) == .openDeeplink(url))
    }

    @Test("dotNS mode keeps same-product navigation in the WebView")
    func dotNsAllowsSameProduct() throws {
        let handler = try makeDotNsHandler()
        let url = try #require(URL(string: "https://shop.dot/checkout"))

        #expect(handler.decide(url: url) == .allow)
    }

    @Test("dotNS mode intercepts navigation to another product")
    func dotNsInterceptsOtherProduct() throws {
        let handler = try makeDotNsHandler()
        let url = try #require(URL(string: "https://wallet.dot"))

        #expect(handler.decide(url: url) == .intercept(url))
    }

    @Test("Direct-URL mode hands the app's deep link to the native handler")
    func directURLOpensDeeplink() throws {
        let handler = try DirectURLNavigationDecisionHandler(
            baseURL: #require(URL(string: "http://localhost:3000"))
        )
        let url = try makePayDeeplink()

        #expect(handler.decide(url: url) == .openDeeplink(url))
    }

    @Test("Foreign custom schemes are not treated as app deep links")
    func foreignSchemeIsNotDeeplink() throws {
        let handler = try makeDotNsHandler()
        let url = try #require(URL(string: "otherapp://pay?amount=1"))

        #expect(handler.decide(url: url) != .openDeeplink(url))
    }

    @MainActor
    @Test("Wireframe forwards the deep link to the native handler")
    func wireframeForwardsDeeplink() throws {
        let linkHandler = MockDeferredLinkHandler()
        let wireframe = SPAWireframe(deepLinkHandler: linkHandler)
        let url = try makePayDeeplink()

        wireframe.openDeeplink(url)

        #expect(linkHandler.handledURLs == [url])
    }
}
