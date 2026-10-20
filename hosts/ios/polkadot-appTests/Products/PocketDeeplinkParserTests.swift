import Foundation
import Products
import Testing
@testable import polkadot_app

/// Classification goes through the core's own `parse_navigate`, so every host
/// reads a Pocket link the same way rather than parsing the string itself.
struct PocketDeeplinkParserTests {
    let parser = PocketDeeplinkParser()

    private func classify(_ link: String) -> PocketLinkClassification {
        parser.classify(URL(string: link)!)
    }

    @Test
    func readsAnAddLink() {
        #expect(classify("polkadotapp://pocket/add?product=game.dot&card=loyalty") == .pocket(PocketDeeplink(
            productHost: "game.dot",
            action: .add,
            cardId: PocketCardId(value: "loyalty"),
            canonicalUrl: "polkadotapp://pocket/add?product=game.dot&card=loyalty"
        )))
    }

    /// A build answers to its own flavor of the scheme, `polkadotappdev` in a
    /// debug build, and has to read a link under it as the one the core mints.
    @Test
    func readsALinkUnderTheAppsOwnScheme() throws {
        let link = "\(DeeplinkTestSchemes.active)://pocket/open?product=game.dot&card=loyalty"
        guard case let .pocket(deeplink) = classify(link) else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(deeplink.action == .open)
        #expect(deeplink.canonicalUrl == "polkadotapp://pocket/open?product=game.dot&card=loyalty")
    }

    /// The card id arrives screened, so nothing downstream screens it again.
    @Test
    func carriesTheCardIdTheCoreScreened() throws {
        guard case let .pocket(link) = classify("polkadotapp://pocket/add?product=game.dot&card=cafe%CC%81") else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(link.cardId.value == "caf\u{e9}")
    }

    /// A bare `polkadotapp://pocket` names the collection itself, so a product can
    /// send the user to the Pocket without naming a card.
    @Test
    func readsACollectionLink() {
        #expect(classify("polkadotapp://pocket") == .collection)
    }

    /// A product's paths are its own App routes, whatever they spell. The host
    /// claims none of them.
    @Test
    func doesNotClaimAProductsOwnPath() {
        #expect(classify("polkadot://game.dot/-/pocket/add?card=loyalty") == .notOurs)
    }

    /// Every Pocket link is the host's, so a Pocket action this core does not
    /// serve is answered here rather than left with nobody to answer it.
    @Test
    func readsAnActionTheCoreDoesNotServeAsMalformed() {
        #expect(classify("polkadotapp://pocket/pin?product=game.dot&card=loyalty") == .malformed)
    }

    @Test
    func readsAMissingCardAsMalformed() {
        #expect(classify("polkadotapp://pocket/add?product=game.dot") == .malformed)
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
