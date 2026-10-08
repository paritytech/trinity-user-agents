import Foundation
import Products
import StructuredConcurrency

/// Whether a card opens with its face shown, as its product published it.
enum PocketCardFaceOnOpen {
    /// A card whose product cannot be asked in time opens with its face shown,
    /// since a face hidden by mistake is not one the user knows to pull back.
    static func faceShown(
        for key: PocketCardKey,
        cards: any PublishedPocketCardsResolving,
        timeout: Duration = .milliseconds(500)
    ) async -> Bool {
        // Raced through a continuation rather than a task group, which would wait
        // out a resolver that ignores cancellation.
        var racers: [Task<Void, Never>] = []
        let published = try? await withCheckedThrowingContinuation { continuation in
            let first = CheckedContinuationGuard<Bool?>(continuation)
            racers = [
                Task {
                    let card = try? await cards.find(productId: key.productId, cardId: key.cardId)
                    first.resume(returning: card?.definition.faceShown)
                },
                Task {
                    try? await Task.sleep(for: timeout)
                    first.resume(returning: nil)
                }
            ]
        }
        racers.forEach { $0.cancel() }

        return published ?? true
    }
}
