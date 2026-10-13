import Foundation
import Products
import Testing
@testable import polkadot_app

/// Classification goes through the core's own `parse_navigate`, so every host
/// reads a Pocket link the same way rather than parsing the string itself.
struct PocketDeeplinkParserTests {
    let parser = PocketDeeplinkParser()

    @Test
    func readsAnAddLink() {
        #expect(parser.classify("polkadot://game.dot/-/pocket/add?card=loyalty") == .pocket(PocketDeeplink(
            productHost: "game.dot",
            action: .add,
            cardId: PocketCardId(value: "loyalty"),
            canonicalUrl: "polkadot://game.dot/-/pocket/add?card=loyalty"
        )))
    }

    @Test
    func readsAnOpenLink() throws {
        guard case let .pocket(link) = parser.classify("polkadot://game.dot/-/pocket/open?card=loyalty") else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(link.action == .open)
    }

    /// The card id arrives screened, so nothing downstream screens it again.
    @Test
    func carriesTheCardIdTheCoreScreened() throws {
        guard case let .pocket(link) = parser.classify("polkadot://game.dot/-/pocket/add?card=cafe%CC%81") else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(link.cardId.value == "caf\u{e9}")
    }

    /// The reserved `-` segment belongs to the host. An ordinary product path is
    /// not a Pocket link and must fall through to the App.
    @Test
    func doesNotClaimAnOrdinaryProductLink() {
        #expect(parser.classify("polkadot://game.dot/some/page") == .notOurs)
    }

    /// A Pocket verb this core does not serve is the core's to route, and it
    /// routes it to the App so a link minted for a newer host still opens.
    @Test
    func doesNotClaimAnActionTheCoreDoesNotServe() {
        #expect(parser.classify("polkadot://game.dot/-/pocket/pin?card=loyalty") == .notOurs)
    }

    /// A known action whose card is missing is malformed, not a target this
    /// core lacks, so the host answers it rather than opening the product.
    @Test
    func readsAMissingCardAsMalformed() {
        #expect(parser.classify("polkadot://game.dot/-/pocket/add") == .malformed)
    }

    @Test
    func doesNotClaimAnUnrelatedUrl() {
        #expect(parser.classify("https://example.com/-/pocket/add?card=loyalty") == .notOurs)
    }
}
