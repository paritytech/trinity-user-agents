import Foundation
import Testing
@testable import polkadot_app

struct PocketDeeplinkParserTests {
    let parser = PocketDeeplinkParser()

    private func classify(_ link: String) -> PocketLinkClassification {
        parser.classify(URL(string: link)!)
    }

    /// A product's paths are its own App routes, whatever they spell. The host
    /// claims none of them.
    @Test
    func doesNotClaimAProductsOwnPath() {
        #expect(classify("polkadot://game.dot/-/pocket/add?card=loyalty") == .notOurs)
    }

    /// A deeplink's host is lowercase, and the app matches it exactly, as it does
    /// chat, pay and pair, so it reads a Pocket link the way Android does.
    @Test
    func doesNotClaimAMixedCaseHost() {
        #expect(classify("polkadotapp://Pocket/add?product=game.dot&card=loyalty") == .notOurs)
    }

    /// Under a web scheme `pocket` is an ordinary host name, not the Pocket.
    @Test
    func doesNotClaimAWebUrl() {
        #expect(classify("https://pocket/add?product=game.dot&card=loyalty") == .notOurs)
    }
}
