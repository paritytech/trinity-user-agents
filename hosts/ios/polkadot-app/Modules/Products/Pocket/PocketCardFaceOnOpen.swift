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
}
