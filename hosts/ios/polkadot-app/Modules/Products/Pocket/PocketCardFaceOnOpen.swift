import Foundation
import Products
import StructuredConcurrency

/// Whether a card opens with its face shown, as its product published it.
enum PocketCardFaceOnOpen {
    /// A card whose product cannot be asked in time opens with its face shown,
    /// since a face hidden by mistake is not one the user knows to pull back.
    /// The card is already on screen while it waits, so the bound only keeps a
    /// hung chain read from folding the face long after.
    static func faceShown(
        for key: PocketCardKey,
        cards: any PublishedPocketCardsResolving,
        timeout: Duration = .seconds(5)
    ) async -> Bool {
        let published = try? await withTimeout(timeout) {
            try await cards.find(productId: key.productId, cardId: key.cardId).definition.faceShown
        }

        return published ?? true
    }

    /// Gives an opened card's screen the face its product published, once the
    /// product answers, unless the card has been closed meanwhile.
    @MainActor
    static func apply(
        to screen: PocketCardScreenViewController,
        for key: PocketCardKey,
        cards: any PublishedPocketCardsResolving
    ) async {
        let shown = await faceShown(for: key, cards: cards)
        guard screen.isOnDisplay else { return }

        screen.applyOpeningFace(shown: shown)
    }
}
