import Foundation
import Products
import Testing
@testable import polkadot_app

/// Where a card's product opens. Every host builds this the same way, so a
/// product is handed the same address whichever host it is running on.
struct PocketCardLaunchUrlTests {
    @Test
    func opensTheProductAtItsOwnPageNamingTheCard() throws {
        let key = PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "humanity"))

        let url = try #require(key.launchUrl)

        #expect(url.absoluteString == "https://peopl.paseo?card=humanity")
    }

    /// A card id may carry characters that are legal in an id but not in a
    /// query, and the product reads the id back out of this address.
    @Test
    func encodesACardIdThatNeedsIt() throws {
        let key = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "gold card"))

        let url = try #require(key.launchUrl)

        #expect(url.absoluteString == "https://game.paseo?card=gold%20card")
    }
}
