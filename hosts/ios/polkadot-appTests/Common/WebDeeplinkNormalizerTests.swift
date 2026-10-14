import Foundation
import Testing

@testable import polkadot_app

@Suite("WebDeeplinkNormalizer Tests")
struct WebDeeplinkNormalizerTests {
    private let normalizer = WebDeeplinkNormalizer(appScheme: "polkadotapp")

    @Test("Maps bare host first path segment to action")
    func mapsBareHostAction() throws {
        let url = try #require(URL(string: "https://dot.li/pair?handshake=0xdeadbeef"))

        let normalized = try #require(normalizer.appSchemeDeeplink(from: url))

        #expect(normalized.scheme == "polkadotapp")
        #expect(normalized.host() == "pair")
        #expect(normalized.path().isEmpty)
        #expect(normalized.query() == "handshake=0xdeadbeef")
    }

    @Test("Maps bare paseo host first path segment to action")
    func mapsBarePaseoHostAction() throws {
        let url = try #require(URL(string: "https://paseo.li/pair"))

        let normalized = try #require(normalizer.appSchemeDeeplink(from: url))

        #expect(normalized.scheme == "polkadotapp")
        #expect(normalized.host() == "pair")
    }

    @Test("Keeps the rest of a nested action path")
    func keepsNestedActionPath() throws {
        let url = try #require(URL(string: "https://dot.li/fiatOnramp/buySuccess?sessionId=1"))

        let normalized = try #require(normalizer.appSchemeDeeplink(from: url))

        #expect(normalized.host() == "fiatOnramp")
        #expect(normalized.path() == "/buySuccess")
        #expect(normalized.query() == "sessionId=1")
    }

    @Test("Maps bare host on http scheme too")
    func mapsHttpScheme() throws {
        let url = try #require(URL(string: "http://dot.li/pair"))

        #expect(normalizer.appSchemeDeeplink(from: url)?.host() == "pair")
    }

    @Test("Returns nil without action path")
    func rejectsEmptyPath() throws {
        let bare = try #require(URL(string: "https://dot.li"))
        let slash = try #require(URL(string: "https://paseo.li/"))

        #expect(normalizer.appSchemeDeeplink(from: bare) == nil)
        #expect(normalizer.appSchemeDeeplink(from: slash) == nil)
    }

    @Test("Ignores subdomain hosts")
    func rejectsSubdomainHost() throws {
        let url = try #require(URL(string: "https://pair.dot.li/x"))

        #expect(normalizer.appSchemeDeeplink(from: url) == nil)
    }

    @Test("Ignores foreign hosts")
    func rejectsForeignHost() throws {
        let url = try #require(URL(string: "https://polkadot.network/pair"))

        #expect(normalizer.appSchemeDeeplink(from: url) == nil)
    }

    @Test("Ignores app scheme urls")
    func rejectsAppScheme() throws {
        let url = try #require(URL(string: "polkadotapp://pair"))

        #expect(normalizer.appSchemeDeeplink(from: url) == nil)
    }
}
