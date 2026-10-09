import Foundation
import Observation

@Observable
@MainActor
final class DebugPocketCardsViewModel {
    private(set) var cards: [DebugPocketCard] = []
    private(set) var refusal: String?

    var productId = ""
    var cardId = ""
    var title = ""
    var faceUrl = "http://127.0.0.1:5173/pocket/devicehood.json"

    private let store: any DebugPocketCardsStoring

    init(store: any DebugPocketCardsStoring = DebugPocketCards()) {
        self.store = store
    }

    var canSave: Bool {
        !productId.isEmpty && !cardId.isEmpty && !title.isEmpty && !faceUrl.isEmpty
    }

    func load() {
        cards = store.cards()
    }

    /// The id is screened here rather than at the deeplink, because a card
    /// id the core refuses reads as no card at all, which looks like the
    /// card simply never arriving.
    func save() {
        let card = DebugPocketCard(productId: productId, cardId: cardId, title: title, faceUrl: faceUrl)

        guard card.definition != nil else {
            refusal = "'\(cardId)' is not a card id the core will accept"
            return
        }

        refusal = nil
        store.save(card)
        cardId = ""
        title = ""
        load()
    }

    func delete(at offsets: IndexSet) {
        for index in offsets {
            store.delete(cards[index])
        }
        load()
    }
}
