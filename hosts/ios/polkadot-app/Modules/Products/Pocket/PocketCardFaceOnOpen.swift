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
