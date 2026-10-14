import Foundation
import Testing

@testable import polkadot_app

@Suite("DeeplinkSchemeNormalizer Tests")
struct DeeplinkSchemeNormalizerTests {
    private let normalizer = DeeplinkSchemeNormalizer()
    private let activeScheme = DeeplinkTestSchemes.active
    private let otherScheme = DeeplinkTestSchemes.other

    @Test("Rewrites the other build flavor scheme to the active one")
    func rewritesOtherFlavorScheme() throws {
        let url = try #require(URL(string: "\(otherScheme)://pair?handshake=0xdeadbeef"))

        let normalized = normalizer.normalize(url)

        #expect(normalized.scheme == activeScheme)
        #expect(normalized.host() == "pair")
        #expect(normalized.query() == "handshake=0xdeadbeef")
    }

    @Test("Keeps the active scheme untouched")
    func keepsActiveScheme() throws {
        let url = try #require(URL(string: "\(activeScheme)://pair?handshake=0xdeadbeef"))

        #expect(normalizer.normalize(url) == url)
    }

    @Test("Keeps foreign schemes untouched")
    func keepsForeignScheme() throws {
        let url = try #require(URL(string: "https://polkadot.network/pair"))

        #expect(normalizer.normalize(url) == url)
    }
}
