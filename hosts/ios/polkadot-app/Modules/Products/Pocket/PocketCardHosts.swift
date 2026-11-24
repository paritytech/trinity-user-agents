import Foundation
import Products
import UIKit

/// The product hosted under the opened card.
///
/// Held weakly: the navigation stack owns the screen, so closing the card frees its web view and runtime, and
/// the next open builds them again from the cached archive.
@MainActor
final class PocketCardHosts {
    private struct Opened {
        let key: PocketCardKey
        let pageQuery: String
        weak var view: ExpandedPocketCardViewProtocol?
    }

    private var opened: Opened?

    func open(
        _ key: PocketCardKey,
        pageQuery: String = "",
        make: () -> ExpandedPocketCardViewProtocol?
    ) -> ExpandedPocketCardViewProtocol? {
        guard let view = make() else { return nil }

        opened = Opened(key: key, pageQuery: pageQuery, view: view)
        return view
    }

    /// Has `load` take the card on screen to `pageQuery`, unless its page already shows it.
    func reload(_ key: PocketCardKey, at pageQuery: String, using load: (ExpandedPocketCardViewProtocol) -> Void) {
        guard let opened, opened.key == key, opened.pageQuery != pageQuery, let view = opened.view else { return }

        load(view)
        self.opened = Opened(key: key, pageQuery: pageQuery, view: view)
    }

    func isOnDisplay(_ key: PocketCardKey) -> Bool {
        guard let opened, opened.key == key else { return false }

        return opened.view?.isOnDisplay ?? false
    }
}
