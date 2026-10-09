import Foundation
import Products
import StructuredConcurrency

/// Whether a card opens with its face shown, as its product published it.
enum PocketCardFaceOnOpen {
    /// A product that has not answered within `timeout` leaves the face shown:
    /// the card is already on screen, and a face folded long after it opened,
    /// or hidden by mistake, is not one the user knows to pull back.
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
