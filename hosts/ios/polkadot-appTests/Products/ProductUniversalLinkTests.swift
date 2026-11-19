import Foundation
@testable import Products
import Testing

@testable import polkadot_app

@Suite("Product share link")
struct ProductUniversalLinkTests {
    @Test("A product share host is handed off as an HTTPS URL")
    func buildsAWebUrlForTheProductShareHost() throws {
        let host = try #require(ProductHost(name: "dim2", root: "paseo"))

        let url = try #require(AppConfig.ProductUniversalLink.url(for: host))

        #expect(url.absoluteString == "https://dim2.paseo.li")
    }
}
