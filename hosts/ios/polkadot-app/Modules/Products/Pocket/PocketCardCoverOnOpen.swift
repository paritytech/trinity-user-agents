import Foundation
import Products
import StructuredConcurrency

/// Whether a card opens covered by its page, as its product published it.
enum PocketCardCoverOnOpen {
    /// A product that has not answered within `timeout` leaves the card uncovered:
    /// the card is already on screen, and a face covered long after it opened,
    /// or by mistake, is not one the user knows to pull back.
    static func startsCovered(
        for key: PocketCardKey,
        cards: any PublishedPocketCardsResolving,
        timeout: Duration = .seconds(5),
        clock: any Clock<Duration> = ContinuousClock()
    ) async -> Bool {
        let published = try? await withTimeout(timeout, clock: clock) {
            try await cards.find(productId: key.productId, cardId: key.cardId).definition.startsCovered
        }

        return published ?? false
    }
}
