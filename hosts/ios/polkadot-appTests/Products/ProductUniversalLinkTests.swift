import Foundation
@testable import Products
import Testing

@testable import polkadot_app

@Suite("Product share link")
struct ProductUniversalLinkTests {
    @Test("A product on Paseo is shared on paseo.li, not on the brand's mainnet root")
    func paseoProductSharesOnItsOwnNetwork() throws {
        let host = try #require(ProductHost.parse("dim2.paseo", tld: "paseo"))

        let url = try #require(AppConfig.ProductUniversalLink.url(for: host))

        #expect(url.absoluteString == "https://dim2.paseo.li")
    }

    @Test("A product on mainnet is shared on dot.li")
    func mainnetProductSharesOnDotLi() throws {
        let host = try #require(ProductHost.parse("dim2.dot", tld: "dot"))

        let url = try #require(AppConfig.ProductUniversalLink.url(for: host))

        #expect(url.absoluteString == "https://dim2.dot.li")
    }

    @Test("The shared link opens as the product it was shared from")
    func sharedLinkOpensTheSameProduct() throws {
        let host = try #require(ProductHost.parse("dim2.paseo", tld: "paseo"))
        let url = try #require(AppConfig.ProductUniversalLink.url(for: host))

        let page = ProductPage.fromUrl(url, tld: "paseo")

        #expect(page?.host.toDotDomain() == "dim2.paseo")
    }
}
