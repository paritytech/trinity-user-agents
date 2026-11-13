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
    struct CardProduct {
        let key: PocketCardKey
        let pageQuery: String
        let view: SPAViewProtocol
        let surface: PocketCardSurface
        /// The host session id of the page `view` was built with: the id of the tap that built it.
        let hostSessionID: UUID
    }

    private var held: CardProduct?

    /// The product for `key` at `pageQuery`, built by `make` unless the one
    /// already held is it. A product built here keeps `hostSessionID`; one
    /// already held keeps the id it was built with.
    func product(
        for key: PocketCardKey,
        pageQuery: String = "",
        hostSessionID: UUID = UUID(),
        make: (PocketCardSurface) -> SPAViewProtocol?
    ) -> CardProduct? {
        if let held, held.key == key, held.pageQuery == pageQuery { return held }

        release()

        let surface = PocketCardSurface()
        guard let view = make(surface) else { return nil }

        let product = CardProduct(
            key: key,
            pageQuery: pageQuery,
            view: view,
            surface: surface,
            hostSessionID: hostSessionID
        )
        held = product
        return product
    }

    /// Has `load` take the held product for `key` to `pageQuery`, unless its
    /// page already shows it.
    func reload(_ key: PocketCardKey, at pageQuery: String, using load: (SPAViewProtocol) -> Void) {
        guard let held, held.key == key, held.pageQuery != pageQuery else { return }

        load(held.view)
        self.held = CardProduct(
            key: key,
            pageQuery: pageQuery,
            view: held.view,
            surface: held.surface,
            hostSessionID: held.hostSessionID
        )
    }

    /// The host session id of the product held for `key`, if one is.
    func hostSessionID(for key: PocketCardKey) -> UUID? {
        guard let held, held.key == key else { return nil }

        return held.hostSessionID
    }

    func isOnDisplay(_ key: PocketCardKey) -> Bool {
        guard let held, held.key == key else { return false }

        return held.surface.isOnDisplay
    }

    /// Gives up a product held for a card the collection no longer has, since a
    /// card that is gone has no next tap.
    func keepOnly(_ isStillHeld: (PocketCardKey) -> Bool) {
        guard let held, !isStillHeld(held.key) else { return }

        release()
    }

    func release() {
        held = nil
    }
}
