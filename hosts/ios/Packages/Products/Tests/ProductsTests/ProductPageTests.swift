import Foundation
import Testing
@testable import Products

struct ProductPageTests {
    @Test func rootRouteBecomesNoPage() throws {
        let page = try #require(ProductPage.fromUrl(URL(string: "https://browse.dot/")!, tld: "dot"))
        #expect(page.page == nil)
    }

    @Test func foundationURLBecomesAProductPage() throws {
        let url = URL(string: "https://browse.dot/onboarding?ref=abc#section")!
        let page = try #require(ProductPage.fromUrl(url, tld: "dot"))

        #expect(page.host.toDotDomain().rawValue == "browse.dot")
        #expect(page.page == "/onboarding?ref=abc#section")
    }

    // MARK: - applied(to:)

    @Test func appliedWithoutPageReturnsRoot() {
        let host = ProductHost(name: "browse", root: "dot")!
        let base = URL(string: "product://browse.dot/index.html")!
        let result = ProductPage(host: host).applied(to: base)
        #expect(result.absoluteString == "product://browse.dot/")
    }

    @Test func appliedWithoutPageLeavesDeepRoute() {
        let host = ProductHost(name: "browse", root: "dot")!
        let current = URL(string: "product://browse.dot/#/onboarding?ref=abc")!
        let result = ProductPage(host: host).applied(to: current)
        #expect(result.absoluteString == "product://browse.dot/")
    }

    @Test func appliedReplacesEntryFileWithFragmentRoute() {
        let host = ProductHost(name: "browse", root: "dot")!
        let base = URL(string: "product://browse.dot/index.html")!
        let result = ProductPage(host: host, page: "/#/onboarding").applied(to: base)
        #expect(result.absoluteString == "product://browse.dot/#/onboarding")
    }

    @Test func appliedReplacesEntryFileWithPath() {
        let host = ProductHost(name: "browse", root: "dot")!
        let base = URL(string: "product://browse.dot/index.html")!
        let result = ProductPage(host: host, page: "/onboarding").applied(to: base)
        #expect(result.absoluteString == "product://browse.dot/onboarding")
    }

    @Test func appliedNormalizesPageMissingLeadingSlash() {
        let host = ProductHost(name: "browse", root: "dot")!
        let base = URL(string: "product://browse.dot/index.html")!
        let result = ProductPage(host: host, page: "#/onboarding").applied(to: base)
        #expect(result.absoluteString == "product://browse.dot/#/onboarding")
    }
}
