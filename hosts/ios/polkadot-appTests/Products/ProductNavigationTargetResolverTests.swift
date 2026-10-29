import Foundation
import Products
import Testing
@testable import polkadot_app

@Suite("ProductNavigationTargetResolver Tests")
struct ProductNavigationTargetResolverTests {
    private let resolver = ProductNavigationTargetResolver(
        hostProvider: ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))
    )

    @Test("Resolves the app's pay deep link to an in-app deep link")
    func resolvesPayDeeplink() throws {
        let destination = "\(DeeplinkTestSchemes.active)://pay?address=5GrwvaEF&amount=100"

        guard case let .appDeeplink(url) = resolver.resolve(destination: destination) else {
            Issue.record("Expected an app deep link")
            return
        }

        #expect(url.absoluteString == destination)
    }

    @Test("Resolves another build flavor's deep link to an in-app deep link")
    func resolvesOtherFlavorDeeplink() {
        let destination = "\(DeeplinkTestSchemes.other)://pay/cheque?id=abc"

        guard case .appDeeplink = resolver.resolve(destination: destination) else {
            Issue.record("Expected an app deep link")
            return
        }
    }

    @Test("Resolves a .dot destination to a product page")
    func resolvesProductPage() {
        guard case let .productPage(page) = resolver.resolve(destination: "wallet.dot/send") else {
            Issue.record("Expected a product page")
            return
        }

        #expect(page.host.toDotDomain() == "wallet.dot")
    }

    @Test("Resolves allowlisted schemes to external URLs", arguments: [
        "https://example.com",
        "mailto:hello@example.com",
        "tel:+123456789"
    ])
    func resolvesExternalURL(destination: String) {
        guard case .externalURL = resolver.resolve(destination: destination) else {
            Issue.record("Expected an external URL")
            return
        }
    }

    @Test("Rejects schemes outside the allowlist", arguments: [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "otherapp://pay?amount=1"
    ])
    func rejectsUnsupportedScheme(destination: String) {
        guard case .unsupported = resolver.resolve(destination: destination) else {
            Issue.record("Expected the destination to be unsupported")
            return
        }
    }
}
