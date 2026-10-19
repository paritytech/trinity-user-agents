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
        #expect(classify("polkadot://pocket/add?product=game.dot&card=loyalty") == .pocket(PocketDeeplink(
            productHost: "game.dot",
            action: .add,
            cardId: PocketCardId(value: "loyalty"),
            canonicalUrl: "polkadot://pocket/add?product=game.dot&card=loyalty"
        )))
    }

    /// A link from a QR code or another app arrives under this app's own
    /// scheme, and has to read as the same link the core mints.
    @Test
    func readsALinkUnderTheAppsOwnScheme() throws {
        let link = "\(DeeplinkTestSchemes.active)://pocket/open?product=game.dot&card=loyalty"
        guard case let .pocket(deeplink) = classify(link) else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(deeplink.action == .open)
        #expect(deeplink.canonicalUrl == "polkadot://pocket/open?product=game.dot&card=loyalty")
    }

    /// The card id arrives screened, so nothing downstream screens it again.
    @Test
    func carriesTheCardIdTheCoreScreened() throws {
        guard case let .pocket(link) = classify("polkadot://pocket/add?product=game.dot&card=cafe%CC%81") else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(link.cardId.value == "caf\u{e9}")
    }

    /// A bare `polkadot://pocket` names the collection itself, so a product can
    /// send the user to the Pocket without naming a card.
    @Test
    func readsACollectionLink() {
        #expect(classify("polkadot://pocket") == .collection)
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
        #expect(classify("polkadot://pocket/pin?product=game.dot&card=loyalty") == .malformed)
    }

    @Test
    func readsAMissingCardAsMalformed() {
        #expect(classify("polkadot://pocket/add?product=game.dot") == .malformed)
    }

    /// Under a web scheme `pocket` is an ordinary host name, not the Pocket.
    @Test
    func doesNotClaimAWebUrl() {
        #expect(classify("https://pocket/add?product=game.dot&card=loyalty") == .notOurs)
    }
}
