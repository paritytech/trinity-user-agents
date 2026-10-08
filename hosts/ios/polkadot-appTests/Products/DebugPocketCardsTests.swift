import Foundation
import Keystore_iOS
import Products
import Testing
@testable import polkadot_app

struct DebugPocketCardsTests {
    // MARK: - The page a card opens

    /// The page is loaded exactly as typed, and the card query is how it
    /// learns which card it sits under.
    @Test
    func opensTheWidgetPageForTheCardItSitsUnder() throws {
        let store = storeHolding(card(widgetUrl: "http://127.0.0.1:5173/index.html"))

        let url = try #require(store.widgetURL(for: loyaltyKey))

        #expect(url.absoluteString == "http://127.0.0.1:5173/index.html?card=loyalty")
    }

    /// A page can be told how to behave through its own query, such as the
    /// test page's `hideOnLoad`, so the card query must join it, not replace it.
    @Test
    func keepsTheWidgetPagesOwnQuery() throws {
        let store = storeHolding(card(widgetUrl: "http://127.0.0.1:5173/index.html?hideOnLoad"))

        let url = try #require(store.widgetURL(for: loyaltyKey))

        #expect(url.absoluteString == "http://127.0.0.1:5173/index.html?hideOnLoad&card=loyalty")
    }

    /// The card is opened under the id its product resolved to, which need
    /// not be the casing typed into the debug menu.
    @Test
    func findsTheWidgetPageWhateverTheProductsCasing() {
        let store = storeHolding(card(productId: "GAME.paseo", widgetUrl: "http://127.0.0.1:5173/index.html"))

        #expect(store.widgetURL(for: loyaltyKey) != nil)
    }

    /// Without a page of its own the card opens the product's published widget.
    @Test
    func opensNoPageOfItsOwnWhenNoneWasTyped() {
        let store = storeHolding(card(widgetUrl: nil))

        #expect(store.widgetURL(for: loyaltyKey) == nil)
    }

    // MARK: - The face a card opens with

    /// A developer working on the page itself wants it in front, not behind
    /// the face, so the switch must reach the lookup that runs when the card opens.
    @Test
    func opensWithTheFaceAwayWhenTheCardIsToldTo() async {
        let store = storeHolding(card(faceShown: false))
        let cards = PublishedPocketCards(
            products: StubProductResolver(alwaysFailing: URLError(.timedOut)),
            debugCards: { store.cards(for: $0) }
        )

        let faceShown = await PocketCardFaceOnOpen.faceShown(for: loyaltyKey, cards: cards)

        #expect(!faceShown)
    }

    /// An empty widget field must leave the card on its product's own page
    /// rather than on an address that loads nothing.
    @Test @MainActor
    func savesWhatTheMenuWasToldAboutThePageAndTheFace() {
        let store = DebugPocketCards(settingsManager: InMemorySettingsManager())
        let viewModel = DebugPocketCardsViewModel(store: store)
        viewModel.productId = "game.paseo"
        viewModel.cardId = "loyalty"
        viewModel.title = "Loyalty"
        viewModel.faceUrl = faceUrl
        viewModel.opensWithFaceAway = true

        viewModel.save()

        #expect(store.cards() == [card(widgetUrl: nil, faceShown: false)])
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
    widgetUrl: String? = nil,
    faceShown: Bool? = nil
) -> DebugPocketCard {
    DebugPocketCard(
        productId: productId,
        cardId: "loyalty",
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
