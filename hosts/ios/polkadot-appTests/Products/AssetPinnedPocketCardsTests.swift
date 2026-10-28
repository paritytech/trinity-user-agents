import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct AssetPinnedPocketCardsTests {
    let pinned = AssetPinnedPocketCards(tld: "paseo")

    /// Humanity is backed by the governance-reserved personhood product, whose
    /// id is a function of the network the app is on.
    @Test
    func placesHumanityOnThePersonhoodProduct() async {
        #expect(pinned.cards() == [PinnedPocketCard(productId: "peopl.paseo", cardId: "humanity", title: "Humanity")])
    }

    @Test
    func namesAPinnedCardWithoutAwaiting() {
        let key = PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "humanity"))

        #expect(pinned.pinned(key)?.privileged == true)
    }

    /// The same card id on a product that does not back it is not pinned, so a
    /// product cannot claim permanence by naming someone else's card.
    @Test
    func doesNotPinTheSameCardIdOnAnotherProduct() {
        let impostor = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "humanity"))

        #expect(pinned.pinned(impostor) == nil)
    }

    @Test
    func doesNotPinAnUnknownCardOnTheRightProduct() {
        let unknown = PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(pinned.pinned(unknown) == nil)
    }

    /// The bundled face is shipped with the app, so a failure to read it is a
    /// build defect rather than product input, but it must actually decode
    /// through the same path a live face takes.
    @Test
    func bundledFaceDecodesThroughTheRendererPath() async throws {
        let face = try #require(await pinned.face(for: PocketCardId(value: "humanity")))

        guard case .box = face else {
            Issue.record("expected the bundled face to be a box, got \(face)")
            return
        }
    }

    /// A missing file is a hole in the card at first launch, which nothing
    /// else would notice.
    @Test
    func everyImageTheBundledFaceDrawsShipsWithTheApp() throws {
        let url = try #require(Bundle.main.url(forResource: "pocket-humanity", withExtension: "json"))
        let json = try String(contentsOf: url, encoding: .utf8)
        let paths = json.matches(of: #/"tag"\s*:\s*"Archive"\s*,\s*"value"\s*:\s*"([^"]+)"/#)
            .map { String($0.1) }

        #expect(!paths.isEmpty)
        for path in paths {
            #expect(AssetPinnedPocketCards.image(at: path) != nil, "not shipped: \(path)")
        }
    }

    @Test(arguments: [
        "pocket/wordmark-light.png", "pocket/../pocket/card-mark.webp", "card-mark.webp", "pocket-card-mark.webp"
    ])
    func aPathTheAppDoesNotShipIsLeftToTheProductsArchive(path: String) {
        #expect(AssetPinnedPocketCards.image(at: path) == nil)
    }
}
