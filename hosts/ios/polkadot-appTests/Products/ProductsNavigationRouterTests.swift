import Foundation
import Products
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

@MainActor
@Suite("ProductsNavigationRouter Tests")
struct ProductsNavigationRouterTests {
    private final class StubView: UIViewController, ControllerBackedProtocol {}

    @Test("Opens the app's deep link in-app instead of leaving to the system")
    func opensAppDeeplinkInApp() async throws {
        let linkHandler = MockDeferredLinkHandler()
        let router = ProductsNavigationRouter(deepLinkHandler: linkHandler)
        let anchor = StubView()
        router.setPresentationView(anchor)
        let url = try #require(URL(string: "\(DeeplinkTestSchemes.active)://pay?address=5GrwvaEF&amount=100"))

        try await router.openExternalURL(url)

        #expect(linkHandler.handledURLs == [url])
    }

    @Test("Rejects the deep link while no presentation view is attached")
    func rejectsDeeplinkWhenNotReady() async throws {
        let linkHandler = MockDeferredLinkHandler()
        let router = ProductsNavigationRouter(deepLinkHandler: linkHandler)
        let url = try #require(URL(string: "\(DeeplinkTestSchemes.active)://pay?amount=1"))

        await #expect(throws: ProductNativeApiError.self) {
            try await router.openExternalURL(url)
        }
        #expect(linkHandler.handledURLs.isEmpty)
    }
}
