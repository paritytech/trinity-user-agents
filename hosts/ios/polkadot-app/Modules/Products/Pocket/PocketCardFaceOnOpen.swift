import Foundation
import Products

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
        let published = await withCheckedContinuation { continuation in
            let first = FirstAnswer(continuation)
            Task {
                let card = try? await cards.find(productId: key.productId, cardId: key.cardId)
                await first.give(card?.definition.faceShown)
            }
            Task {
                try? await Task.sleep(for: timeout)
                await first.give(nil)
            }
        }

        return published ?? true
    }
}

/// Resumes its continuation with whichever answer arrives first.
private actor FirstAnswer {
    private var continuation: CheckedContinuation<Bool?, Never>?

    init(_ continuation: CheckedContinuation<Bool?, Never>) {
        self.continuation = continuation
    }

    func give(_ answer: Bool?) {
        continuation?.resume(returning: answer)
        continuation = nil
    }
}
