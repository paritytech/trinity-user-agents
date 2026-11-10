import Foundation
import PolkadotUI
import Products
import TrUAPIHost

enum PocketPublishError: Error, Equatable, CustomStringConvertible {
    case noPocket
    case unknownCard
    /// The product answered with a manifest nothing can read. A settled answer
    /// rather than a read that did not land: asking again gives the same one.
    case unreadableProduct

    var description: String {
        switch self {
        case .noPocket: "this product publishes no Pocket cards"
        case .unknownCard: "this product publishes no such card"
        case .unreadableProduct: "this product's manifest cannot be read"
        }
    }
}

/// A card as its product's worker manifest publishes it. The face is read from
/// the worker's own archive, which resolves under `workerContentId` rather than
/// under the product's base name.
struct PublishedPocketCard {
    let productId: ProductId
    let productName: String
    let workerContentId: ProductId
    let definition: PocketCardDefinition
}

/// Looks a card up in the worker manifest of the product that claims to back it.
protocol PublishedPocketCardsResolving: Sendable {
    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard
}

/// A published card as the approval sheet shows it.
struct PocketAddCardOffer {
    let key: PocketCardKey
    let productName: String
    let title: String
    let face: RendererNode
}

/// Loads what the user is being asked to approve, and stores exactly that.
///
/// The offer is loaded once: the face shown in the sheet is the face kept for
/// the card, so what the user approved is what the Pocket draws.
struct PocketAddCardInteractor {
    private let publishedCards: any PublishedPocketCardsResolving
    private let previews: PocketPreviewLoader
    private let store: any PocketCardStore

    init(
        publishedCards: any PublishedPocketCardsResolving,
        previews: PocketPreviewLoader,
        store: any PocketCardStore
    ) {
        self.publishedCards = publishedCards
        self.previews = previews
        self.store = store
    }

    func loadOffer(productId: ProductId, cardId: PocketCardId) async throws -> PocketAddCardOffer {
        let published = try await publishedCards.find(productId: productId, cardId: cardId)
        let face = try await previews.load(
            contentId: published.workerContentId,
            preview: published.definition.preview
        )

        return PocketAddCardOffer(
            key: PocketCardKey(productId: published.productId, cardId: cardId),
            productName: published.productName,
            title: published.definition.title,
            face: face
        )
    }

    /// The card is stored with the face the sheet showed, so what the Pocket
    /// draws next is what the user approved. Only the host places a privileged
    /// card, so an approved one never is.
    func approve(_ offer: PocketAddCardOffer) async throws {
        try await store.add(
            PocketCardEntry(key: offer.key, title: offer.title, privileged: false),
            face: offer.face
        )
    }
}
