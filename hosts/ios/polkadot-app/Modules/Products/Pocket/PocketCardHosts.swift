import Foundation
import Products
import UIKit

/// The product hosted under an opened card.
///
/// A web view and a page load are what a card costs to open, so a card opened
/// again is worth not paying twice.
///
/// One at a time: opening another card takes the previous one down. A Pocket
/// can hold many cards, and one live web view each is not a cost worth carrying
/// for taps that may never come.
///
/// Held for one session only. The product is wired to the runtime provider of
/// the session that opened it, so one carried across a sign-out would be
/// handed to the next session still talking to the last one's core.
@MainActor
final class PocketCardHosts {
    /// A product and the surface its page reaches the card's screen through.
    struct CardProduct {
        let key: PocketCardKey
        let view: SPAViewProtocol
        let surface: PocketCardSurface
    }

    private var held: CardProduct?
    private var opening = false
    private var sessionEnds = 0

    /// The product for `key`, built by `make` unless the one already held is it.
    func product(for key: PocketCardKey, make: (PocketCardSurface) -> SPAViewProtocol?) -> CardProduct? {
        if let held, held.key == key { return held }

        held = nil

        let surface = PocketCardSurface()
        guard let view = make(surface) else { return nil }

        let product = CardProduct(key: key, view: view, surface: surface)
        held = product
        return product
    }

    /// Gives up a product held for a card the collection no longer has, since a
    /// card that is gone has no next tap.
    func keepOnly(_ isStillHeld: (PocketCardKey) -> Bool) {
        guard let held, !isStillHeld(held.key) else { return }

        self.held = nil
    }

    /// Runs `prepare`, then `present` with what it found. Ignored while an earlier
    /// open is under way, since a screen claims its product's surface when built
    /// and would take the page from the screen the user sees. Dropped when the
    /// session ends while it prepares, since what it would present belongs to that
    /// session.
    func openIfIdle<Prepared>(
        preparing prepare: @MainActor () async -> Prepared,
        then present: @MainActor (Prepared) -> Void
    ) async {
        guard !opening else { return }

        opening = true
        defer { opening = false }
        let sessionEndsBefore = sessionEnds
        let prepared = await prepare()
        guard sessionEnds == sessionEndsBefore else { return }

        present(prepared)
    }

    /// Gives up the held product and any open still preparing, as the session
    /// they belong to ends.
    func release() {
        held = nil
        sessionEnds += 1
    }
}
