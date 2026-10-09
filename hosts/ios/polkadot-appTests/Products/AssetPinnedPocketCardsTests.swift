import Foundation
import Products
import Testing
@testable import polkadot_app

struct AssetPinnedPocketCardsTests {
    let pinned = AssetPinnedPocketCards(tld: "paseo")

    /// Humanity is backed by the governance-reserved personhood product, whose
    /// id is a function of the network the app is on.
    @Test
    func placesHumanityOnThePersonhoodProduct() async {
        #expect(await pinned.cards() == [
            PocketCardEntry(
                key: PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "humanity")),
                title: "Humanity",
                privileged: true
            )
        ])
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

        guard case let .column(_, _, children) = face else {
            Issue.record("expected the bundled face to be a column, got \(face)")
            return
        }
        #expect(children.count == 2)
    }
}
