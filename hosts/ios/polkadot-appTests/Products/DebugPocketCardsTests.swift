import Foundation
import Keystore_iOS
import Products
import Testing
@testable import polkadot_app

struct DebugPocketCardsTests {
    // MARK: - The page a card opens

    /// The page is loaded as typed, and the card query is how it learns which
    /// card it sits under. Its own query, such as the test page's `hideOnLoad`,
    /// reaches it encoded as typed (decoded, `a%2Bb` would read as `a b`), and a
    /// stale card query typed into the address must not shadow the real one.
    /// The card is found under the id its product resolved to, whatever the
    /// casing typed into the menu, and one card's page never opens under
    /// another of the same product.
    @Test(arguments: [
        ("index.html", "index.html?card=loyalty"),
        ("index.html?hideOnLoad", "index.html?hideOnLoad&card=loyalty"),
        ("index.html?card=other&hideOnLoad", "index.html?hideOnLoad&card=loyalty"),
        ("index.html?x=a%2Bb", "index.html?x=a%2Bb&card=loyalty")
    ])
    func opensTheWidgetPageForTheCardItSitsUnder(typed: String, opened: String) throws {
        let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
        store.save(card(productId: "GAME.paseo", widgetUrl: "http://127.0.0.1:5173/\(typed)"))
        store.save(card(cardId: "gold"))
        let gold = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "gold"))

        let url = try #require(store.widgetURL(for: loyaltyKey))

        #expect(url.absoluteString == "http://127.0.0.1:5173/\(opened)")
        #expect(store.widgetURL(for: gold) == nil)
    }

    // MARK: - Cards typed in before

    /// A card saved before it could name a page or a face setting must still
    /// be offered, rather than every typed card vanishing at once.
    @Test
    func readsACardStoredBeforeItCouldNameAPage() throws {
        let settings = InMemorySettingsManager()
        let stored = """
        [{"productId": "game.paseo", "cardId": "loyalty", "title": "Loyalty", "faceUrl": "\(faceUrl)"}]
        """
        settings.set(anyValue: Data(stored.utf8), for: SettingsKey.debugPocketCards.rawValue)
        let store = DebugPocketCards(settingsManager: settings)

        let definition = try #require(store.cards(for: "game.paseo").first)

        #expect(definition == PocketCardDefinition(
            id: PocketCardId(value: "loyalty"),
            title: "Loyalty",
            preview: .url(faceUrl),
            faceShown: true
        ))
        #expect(store.widgetURL(for: loyaltyKey) == nil)
    }
}

// MARK: - Fixtures

private let faceUrl = "http://127.0.0.1:5173/faces/loyalty.json"

private let loyaltyKey = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

private func card(
    productId: String = "game.paseo",
    cardId: String = "loyalty",
    widgetUrl: String? = nil
) -> DebugPocketCard {
    DebugPocketCard(
        productId: productId,
        cardId: cardId,
        title: "Loyalty",
        faceUrl: faceUrl,
        widgetUrl: widgetUrl,
        faceShown: nil
    )
}
