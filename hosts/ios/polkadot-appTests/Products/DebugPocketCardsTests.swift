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
    @Test(arguments: [
        ("index.html", "index.html?card=loyalty"),
        ("index.html?hideOnLoad", "index.html?hideOnLoad&card=loyalty"),
        ("index.html?card=other&hideOnLoad", "index.html?hideOnLoad&card=loyalty"),
        ("index.html?x=a%2Bb", "index.html?x=a%2Bb&card=loyalty")
    ])
    func opensTheWidgetPageForTheCardItSitsUnder(typed: String, opened: String) throws {
        let store = storeHolding(card(widgetUrl: "http://127.0.0.1:5173/\(typed)"))

        let url = try #require(store.widgetURL(for: loyaltyKey))

        #expect(url.absoluteString == "http://127.0.0.1:5173/\(opened)")
    }

    /// A product's cards can each have their own page, so one card's page
    /// must not open under another.
    @Test
    func opensOnlyThePageTypedForThatCard() {
        let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
        store.save(card(widgetUrl: "http://127.0.0.1:5173/index.html"))
        store.save(card(cardId: "gold", widgetUrl: nil))
        let gold = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "gold"))

        #expect(store.widgetURL(for: gold) == nil)
    }

    /// The card is opened under the id its product resolved to, which need
    /// not be the casing typed into the debug menu.
    @Test
    func findsTheWidgetPageWhateverTheProductsCasing() {
        let store = storeHolding(card(productId: "GAME.paseo", widgetUrl: "http://127.0.0.1:5173/index.html"))

        #expect(store.widgetURL(for: loyaltyKey) != nil)
    }

    // MARK: - What the menu was told

    /// What the menu was told must reach the stored card: a typed page
    /// without the whitespace a paste brings along, an empty widget field as
    /// no page at all, which leaves the card on its product's own page rather
    /// than on an address that loads nothing, and the face switch, which must
    /// reach the card the lookup finds on open.
    @Test(arguments: [
        ("", nil),
        ("  http://127.0.0.1:5173/index.html \n", "http://127.0.0.1:5173/index.html")
    ] as [(String, String?)])
    @MainActor
    func savesWhatTheMenuWasToldAboutThePageAndTheFace(typed: String, stored: String?) {
        let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
        let viewModel = menuFilledIn(saving: store)
        viewModel.widgetUrl = typed
        viewModel.opensWithFaceAway = true

        viewModel.save()

        #expect(store.cards() == [card(widgetUrl: stored, faceShown: false)])
        #expect(store.cards(for: "game.paseo").map(\.faceShown) == [false])
    }

    /// A page address that cannot load would only show up as the card's
    /// error screen, far from where the typo was made.
    @Test(arguments: ["not a url", "127.0.0.1:5173/index.html", "ftp://127.0.0.1/index.html", "http:///index.html"])
    @MainActor
    func refusesAWidgetPageThatIsNotAWebAddress(typed: String) {
        let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
        let viewModel = menuFilledIn(saving: store)
        viewModel.widgetUrl = typed

        viewModel.save()

        #expect(viewModel.refusal != nil)
        #expect(store.cards().isEmpty)
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
    widgetUrl: String? = nil,
    faceShown: Bool? = nil
) -> DebugPocketCard {
    DebugPocketCard(
        productId: productId,
        cardId: cardId,
        title: "Loyalty",
        faceUrl: faceUrl,
        widgetUrl: widgetUrl,
        faceShown: faceShown
    )
}

private func storeHolding(_ card: DebugPocketCard) -> DebugPocketCards {
    let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
    store.save(card)
    return store
}

@MainActor
private func menuFilledIn(saving store: DebugPocketCards) -> DebugPocketCardsViewModel {
    let viewModel = DebugPocketCardsViewModel(store: store)
    viewModel.productId = "game.paseo"
    viewModel.cardId = "loyalty"
    viewModel.title = "Loyalty"
    viewModel.faceUrl = faceUrl
    return viewModel
}
