import Foundation
import Testing
@testable import Products

struct ProductHostTests {
    // MARK: - parse(_:tld:)

    @Test func simpleDotDomain() {
        let host = ProductHost.parse("browse.dot", tld: "dot")
        #expect(host != nil)
    }

    @Test func dotLiDomain() {
        let host = ProductHost.parse("browse.dot.li", tld: "dot")
        #expect(host != nil)
    }

    @Test func subdomainDotDomain() {
        let host = ProductHost.parse("sub.browse.dot", tld: "dot")
        #expect(host != nil)
    }

    @Test func subdomainDotLiDomain() {
        let host = ProductHost.parse("sub.browse.dot.li", tld: "dot")
        #expect(host != nil)
    }

    @Test func deeplyNestedSubdomain() {
        let host = ProductHost.parse("a.b.c.browse.dot", tld: "dot")
        #expect(host != nil)
    }

    @Test func deeplyNestedSubdomainDotLi() {
        let host = ProductHost.parse("a.b.c.browse.dot.li", tld: "dot")
        #expect(host != nil)
    }

    @Test func paseoLiDomain() {
        let host = ProductHost.parse("browse.paseo.li", tld: "dot")
        #expect(host != nil)
    }

    @Test func subdomainPaseoLiDomain() {
        let host = ProductHost.parse("sub.browse.paseo.li", tld: "dot")
        #expect(host != nil)
    }

    @Test func rejectsBarePaseoDomain() {
        #expect(ProductHost.parse("browse.paseo", tld: "dot") == nil)
    }

    @Test func rejectsPaseoLiWithoutName() {
        #expect(ProductHost.parse(".paseo.li", tld: "dot") == nil)
    }

    @Test func rejectsPlainString() {
        #expect(ProductHost.parse("browse", tld: "dot") == nil)
    }

    @Test func rejectsEmptyString() {
        #expect(ProductHost.parse("", tld: "dot") == nil)
    }

    @Test func rejectsDotOnly() {
        #expect(ProductHost.parse(".dot", tld: "dot") == nil)
    }

    @Test func rejectsWrongTld() {
        #expect(ProductHost.parse("browse.com", tld: "dot") == nil)
    }

    @Test func rejectsPartialDotLi() {
        #expect(ProductHost.parse("browse.li", tld: "dot") == nil)
    }

    @Test func rejectsDotLiWithoutName() {
        #expect(ProductHost.parse(".dot.li", tld: "dot") == nil)
    }

    @Test func browseTestParsesWithTestTld() {
        let host = ProductHost.parse("browse.test", tld: "test")
        #expect(host != nil)
    }

    @Test func browseTestDotReturnsNilWithWrongTld() {
        let host = ProductHost.parse("browse.dot", tld: "test")
        #expect(host == nil)
    }

    @Test func browseTestLiParsesInShareDomains() {
        let host = ProductHost.parse("browse.test.li", tld: "test")
        #expect(host != nil)
    }

    @Test func browsePaseoLiRoundTripsCorrectly() {
        let host = ProductHost.parse("browse.paseo.li", tld: "dot")
        #expect(host?.toDotDomain() == "browse.paseo")
    }

    @Test func subBrowsePaseoLiRoundTripsCorrectly() {
        let host = ProductHost.parse("sub.browse.paseo.li", tld: "dot")
        #expect(host?.toDotDomain() == "sub.browse.paseo")
    }

    // MARK: - init?(name:root:)

    @Test func directConstructionSimple() {
        let host = ProductHost(name: "browse", root: "dot")
        #expect(host != nil)
        #expect(host?.toDotDomain() == "browse.dot")
    }

    @Test func directConstructionWithSubdomain() {
        let host = ProductHost(name: "sub.browse", root: "paseo")
        #expect(host != nil)
        #expect(host?.toDotDomain() == "sub.browse.paseo")
    }

    @Test func directConstructionWithDeepSubdomain() {
        let host = ProductHost(name: "a.b.c.browse", root: "dot")
        #expect(host != nil)
        #expect(host?.toDotDomain() == "a.b.c.browse.dot")
    }

    @Test func directConstructionRejectsEmptyName() {
        #expect(ProductHost(name: "", root: "dot") == nil)
    }

    @Test func directConstructionRejectsEmptyRoot() {
        #expect(ProductHost(name: "browse", root: "") == nil)
    }

    @Test func directConstructionRejectsRootWithDot() {
        #expect(ProductHost(name: "browse", root: "dot.li") == nil)
    }

    @Test func directConstructionRejectsNameWithEmptyLabel() {
        #expect(ProductHost(name: "a..b", root: "dot") == nil)
    }

    // MARK: - name

    @Test func nameForSimpleDotDomain() {
        let host = ProductHost.parse("browse.dot", tld: "dot")
        #expect(host?.name == "browse")
    }

    @Test func nameForDotLiDomain() {
        let host = ProductHost.parse("browse.dot.li", tld: "dot")
        #expect(host?.name == "browse")
    }

    @Test func nameForSubdomainDotDomain() {
        let host = ProductHost.parse("sub.browse.dot", tld: "dot")
        #expect(host?.name == "sub.browse")
    }

    @Test func nameForSubdomainDotLiDomain() {
        let host = ProductHost.parse("sub.browse.dot.li", tld: "dot")
        #expect(host?.name == "sub.browse")
    }

    @Test func nameForPaseoLiDomain() {
        let host = ProductHost.parse("browse.paseo.li", tld: "dot")
        #expect(host?.name == "browse")
    }

    @Test func nameForSubdomainPaseoLiDomain() {
        let host = ProductHost.parse("sub.browse.paseo.li", tld: "dot")
        #expect(host?.name == "sub.browse")
    }

    @Test func nameForDeepSubdomain() {
        let host = ProductHost.parse("a.b.c.dot", tld: "dot")
        #expect(host?.name == "a.b.c")
    }

    // MARK: - toDotDomain()

    @Test func toDotDomainForSimple() {
        let host = ProductHost.parse("browse.dot", tld: "dot")
        #expect(host?.toDotDomain() == "browse.dot")
    }

    @Test func toDotDomainForDotLi() {
        let host = ProductHost.parse("browse.dot.li", tld: "dot")
        #expect(host?.toDotDomain() == "browse.dot")
    }

    @Test func toDotDomainForSubdomain() {
        let host = ProductHost.parse("sub.browse.dot", tld: "dot")
        #expect(host?.toDotDomain() == "sub.browse.dot")
    }

    @Test func toDotDomainForSubdomainDotLi() {
        let host = ProductHost.parse("sub.browse.dot.li", tld: "dot")
        #expect(host?.toDotDomain() == "sub.browse.dot")
    }

    @Test func toDotDomainForPaseoLi() {
        let host = ProductHost.parse("browse.paseo.li", tld: "dot")
        #expect(host?.toDotDomain() == "browse.paseo")
    }

    @Test func toDotDomainForSubdomainPaseoLi() {
        let host = ProductHost.parse("sub.browse.paseo.li", tld: "dot")
        #expect(host?.toDotDomain() == "sub.browse.paseo")
    }

    // MARK: - fromUrl(_:)

    @Test func fromUrlWithDotDomain() {
        let url = URL(string: "https://browse.dot/path")!
        let host = ProductHost.fromUrl(url, tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "browse")
    }

    @Test func fromUrlWithSubdomain() {
        let url = URL(string: "https://sub.browse.dot/path")!
        let host = ProductHost.fromUrl(url, tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "sub.browse")
    }

    @Test func fromUrlWithPaseoLiDomain() {
        let url = URL(string: "https://browse.paseo.li/path")!
        let host = ProductHost.fromUrl(url, tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "browse")
        #expect(host?.toDotDomain() == "browse.paseo")
    }

    @Test func fromUrlRejectsInvalidHost() {
        let url = URL(string: "https://browse.com/path")!
        #expect(ProductHost.fromUrl(url, tld: "dot") == nil)
    }

    // MARK: - fromNavigationDestination(_:)

    @Test func fromNavigationDestinationWithUrl() {
        let host = ProductHost.fromNavigationDestination("https://browse.dot/page", tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "browse")
    }

    @Test func fromNavigationDestinationWithRawString() {
        let host = ProductHost.fromNavigationDestination("browse.dot", tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "browse")
    }

    @Test func fromNavigationDestinationWithSubdomain() {
        let host = ProductHost.fromNavigationDestination("sub.browse.dot", tld: "dot")
        #expect(host != nil)
        #expect(host?.name == "sub.browse")
    }

    @Test func fromNavigationDestinationRejectsInvalid() {
        #expect(ProductHost.fromNavigationDestination("invalid", tld: "dot") == nil)
    }

    @Test func fromNavigationDestinationSchemelessHostWithFragmentRoute() {
        let host = ProductHost.fromNavigationDestination("getcash.paseo/#/", tld: "paseo")
        #expect(host?.name == "getcash")
        #expect(host?.toDotDomain() == "getcash.paseo")
    }

    @Test func fromNavigationDestinationSchemelessHostWithPath() {
        let host = ProductHost.fromNavigationDestination("browse.dot/onboarding?ref=abc", tld: "dot")
        #expect(host?.name == "browse")
    }

    @Test func fromNavigationDestinationSchemelessShareHostWithPath() {
        let host = ProductHost.fromNavigationDestination("browse.dot.li/#/onboarding", tld: "dot")
        #expect(host?.toDotDomain() == "browse.dot")
    }

    @Test func fromNavigationDestinationSchemelessExternalHostWithPath() {
        #expect(ProductHost.fromNavigationDestination("stg.revx.dev/editor", tld: "dot") == nil)
    }

    @Test func fromNavigationDestinationRejectsExternalHost() {
        #expect(ProductHost.fromNavigationDestination("https://stg.revx.dev/editor", tld: "dot")
            == nil)
    }

    @Test func fromNavigationDestinationRejectsExternalHostWithDotQuery() {
        let dest = "https://stg.revx.dev/editor?mod=dot-cli-mod-fixture.dot"
        #expect(ProductHost.fromNavigationDestination(dest, tld: "dot") == nil)
    }

    @Test func fromNavigationDestinationAcceptsDotUrlWithQuery() {
        let host = ProductHost.fromNavigationDestination(
            "https://browse.dot/editor?mod=other.dot",
            tld: "dot"
        )
        #expect(host?.name == "browse")
    }
}
