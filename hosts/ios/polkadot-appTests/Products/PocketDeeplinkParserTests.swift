import Foundation
import Products
import Testing
import struct TrUAPIHost.ProductId
@testable import polkadot_app

/// Classification goes through the core's own `parse_navigate`, so every host
/// reads a Pocket link the same way rather than parsing the string itself.
struct PocketDeeplinkParserTests {
    let parser: PocketDeeplinkParser

    init() throws {
        parser = try PocketDeeplinkParser(stack: TrUAPIBundledStack.paseoNextV2.open())
    }

    private func classify(_ link: String) -> PocketLinkClassification {
        parser.classify(URL(string: link)!)
    }

    @Test
    func readsAnAddLink() throws {
        #expect(try classify("polkadotapp://pocket/add?product=game.paseo&card=loyalty") == .pocket(PocketDeeplink(
            productHost: ProductId("game.paseo"),
            action: .add,
            cardId: PocketCardId(value: "loyalty"),
            pageQuery: "",
            canonicalUrl: "polkadotapp://pocket/add?product=game.paseo&card=loyalty"
        )))
    }

    /// An open link can name the screen the card's page opens on. The params
    /// stay encoded, so `a%26b` reaches the page as one value, not two.
    @Test
    func carriesAnOpenLinksPageParamsAsEncoded() throws {
        guard case let .pocket(link) = classify(
            "polkadotapp://pocket/open?product=game.paseo&card=loyalty&open=scratch&tag=a%26b"
        ) else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(link.pageQuery == "open=scratch&tag=a%26b")
    }

    /// A build answers to its own flavor of the scheme, `polkadotappdev` in a
    /// debug build, and has to read a link under it as the one the core mints.
    @Test
    func readsALinkUnderTheAppsOwnScheme() throws {
        let link = "\(DeeplinkTestSchemes.active)://pocket/open?product=game.paseo&card=loyalty"
        guard case let .pocket(deeplink) = classify(link) else {
            Issue.record("expected a pocket link")
            return
        }

        #expect(deeplink.action == .open)
        #expect(deeplink.canonicalUrl == "polkadotapp://pocket/open?product=game.paseo&card=loyalty")
    }

    /// The card id arrives screened, so nothing downstream screens it again.
    @Test
    func carriesTheCardIdTheCoreScreened() throws {
        guard case let .pocket(link) = classify("polkadotapp://pocket/add?product=game.paseo&card=cafe%CC%81") else {
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
        #expect(classify("polkadot://game.paseo/-/pocket/add?card=loyalty") == .notOurs)
    }

    /// Every Pocket link is the host's, so a Pocket action this core does not
    /// serve is answered here rather than left with nobody to answer it.
    @Test
    func readsAnActionTheCoreDoesNotServeAsMalformed() {
        #expect(classify("polkadotapp://pocket/pin?product=game.paseo&card=loyalty") == .malformed)
    }

    @Test
    func readsAMissingCardAsMalformed() {
        #expect(classify("polkadotapp://pocket/add?product=game.paseo") == .malformed)
    }

    /// A deeplink's host is lowercase, and the app matches it exactly, as it does
    /// chat, pay and pair, so it reads a Pocket link the way Android does.
    @Test
    func doesNotClaimAMixedCaseHost() {
        #expect(classify("polkadotapp://Pocket/add?product=game.paseo&card=loyalty") == .notOurs)
    }

    /// Under a web scheme `pocket` is an ordinary host name, not the Pocket.
    @Test
    func doesNotClaimAWebUrl() {
        #expect(classify("https://pocket/add?product=game.paseo&card=loyalty") == .notOurs)
    }

    /// The stack serves products under its own TLD and reads a `dot` name as
    /// one of them, so a card of another network's product is a Pocket link
    /// the core refuses.
    @Test
    func readsACardOfAnotherNetworksProductAsMalformed() throws {
        guard case let .pocket(link) = classify("polkadotapp://pocket/add?product=game.dot&card=loyalty") else {
            Issue.record("expected a dot name to be read as the network's own product")
            return
        }
        #expect(try link.productHost == ProductId("game.paseo"))
        #expect(classify("polkadotapp://pocket/add?product=game.testnet&card=loyalty") == .malformed)
    }

    /// Which products are served is unknown without the stack, so no link is
    /// claimed when the bundled stack could not be opened.
    @Test
    func claimsNoLinkWithoutAStack() {
        let parser = PocketDeeplinkParser(stack: nil)
        #expect(parser.classify(URL(string: "polkadotapp://pocket")!) == .notOurs)
    }
}
