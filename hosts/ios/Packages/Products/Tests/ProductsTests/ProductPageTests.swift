import Foundation
import Testing
@testable import Products

struct ProductPageTests {
    // MARK: - fromUrl(_:)

    @Test func fromUrlWithoutPage() {
        let page = ProductPage.fromUrl(URL(string: "https://browse.dot")!, tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == nil)
    }

    @Test func fromUrlWithRootPath() {
        let page = ProductPage.fromUrl(URL(string: "https://browse.dot/")!, tld: "dot")
        #expect(page?.page == nil)
    }

    @Test func fromUrlWithPath() {
        let page = ProductPage.fromUrl(URL(string: "https://browse.dot/onboarding/step")!, tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == "/onboarding/step")
    }

    @Test func fromUrlKeepsFragmentInRelativePart() {
        let page = ProductPage.fromUrl(URL(string: "https://browse.dot.li/#/onboarding")!, tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == "/#/onboarding")
    }

    @Test func fromUrlKeepsQueryInRelativePart() {
        let page = ProductPage.fromUrl(URL(string: "https://browse.dot/onboarding?ref=abc")!, tld: "dot")
        #expect(page?.page == "/onboarding?ref=abc")
    }

    @Test func fromUrlRejectsInvalidHost() {
        #expect(ProductPage.fromUrl(URL(string: "https://browse.com/page")!, tld: "dot") == nil)
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

    // MARK: - fromNavigationDestination(_:)

    @Test func fromNavigationDestinationUrlKeepsPage() {
        let page = ProductPage.fromNavigationDestination("https://browse.dot/onboarding", tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == "/onboarding")
    }

    @Test func fromNavigationDestinationUrlKeepsFragmentRoute() {
        let page = ProductPage.fromNavigationDestination("https://browse.dot.li/#/onboarding", tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == "/#/onboarding")
    }

    @Test func fromNavigationDestinationBareDomainHasNoPage() {
        let page = ProductPage.fromNavigationDestination("browse.dot", tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == nil)
    }

    @Test func fromNavigationDestinationRejectsExternalHost() {
        #expect(ProductPage.fromNavigationDestination("https://stg.revx.dev/editor", tld: "dot") == nil)
    }

    @Test func fromNavigationDestinationRejectsInvalid() {
        #expect(ProductPage.fromNavigationDestination("invalid", tld: "dot") == nil)
    }

    @Test func fromNavigationDestinationSchemelessHostKeepsFragmentRoute() {
        let page = ProductPage.fromNavigationDestination("getcash.paseo/#/", tld: "paseo")
        #expect(page?.host.toDotDomain() == "getcash.paseo")
        #expect(page?.page == "/#/")
    }

    @Test func fromNavigationDestinationSchemelessHostKeepsPath() {
        let page = ProductPage.fromNavigationDestination("browse.dot/onboarding?ref=abc", tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == "/onboarding?ref=abc")
    }

    @Test func fromNavigationDestinationSchemelessRootPathHasNoPage() {
        let page = ProductPage.fromNavigationDestination("browse.dot/", tld: "dot")
        #expect(page?.host.toDotDomain() == "browse.dot")
        #expect(page?.page == nil)
    }

    @Test func fromNavigationDestinationSchemelessExternalHostWithPath() {
        #expect(ProductPage.fromNavigationDestination("stg.revx.dev/editor", tld: "dot") == nil)
    }
}
