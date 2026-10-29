import Foundation
import Products
import UIKit

/// Routes `polkadotapp://pocket/{add,open}?product=<id>&card=<id>`, and
/// `polkadotapp://pocket` alone for the collection, under any of this app's
/// schemes.
///
/// What the link means is entirely the core's to say. `parse_navigate` answers
/// `pocket` for an action it serves, having screened the card id exactly as
/// `remove_card` does, `pocketCollection` for the bare link, and `reject` for a
/// Pocket link it cannot make sense of, including an action it does not serve.
/// All three are the host's to answer. A link to anything else is not ours.
final class PocketOpenService: URLHandlingServiceProtocol {
    private let parser = PocketDeeplinkParser()
    private let present: @MainActor (PocketDeeplink) -> Void
    private let openCollection: @MainActor () -> Void
    private let refuse: @MainActor (String) -> Void

    init(
        present: @escaping @MainActor (PocketDeeplink) -> Void,
        openCollection: @escaping @MainActor () -> Void = {},
        refuse: @escaping @MainActor (String) -> Void = { _ in }
    ) {
        self.present = present
        self.openCollection = openCollection
        self.refuse = refuse
    }

    func handle(url: URL) -> Bool {
        switch parser.classify(url) {
        case let .pocket(link):
            Task { @MainActor in present(link) }
            return true
        case .collection:
            Task { @MainActor in openCollection() }
            return true
        case .malformed:
            Task { @MainActor in refuse(String(localized: .Products.pocketDeeplinkMalformed)) }
            return true
        case .notOurs:
            return false
        }
    }
}

extension PocketOpenService {
    /// The chain's Pocket handler: an add link opens the approval sheet, an open
    /// link takes the user to the card the Pocket already holds, and a bare link
    /// opens the Pocket.
    static func makeDefault(
        flowState: SPAFlowState,
        moduleNavigator: ModuleNavigating
    ) -> PocketOpenService {
        PocketOpenService(
            present: { link in
                Task { @MainActor in
                    // A session with no Pocket has nowhere to put the card and
                    // nothing to open. The host claimed the link, so it says so
                    // rather than dropping it.
                    guard let pocket = ProductPocketService.current else {
                        PocketRefusalPresenter.show(String(localized: .Products.pocketDeeplinkNoPocket))
                        return
                    }

                    switch link.action {
                    case .add:
                        guard let view = PocketAddCardViewFactory.createView(
                            for: link,
                            flowState: flowState,
                            pocket: pocket
                        ) else {
                            return
                        }
                        // Presented directly rather than through the navigator,
                        // which wraps what it is given in a navigation
                        // controller and overrides the sheet's own
                        // presentation.
                        UIWindow.topWindow?.topmostViewController?.present(view, animated: true)
                    case .open:
                        PocketCardOpening.open(
                            link: link,
                            flowState: flowState,
                            navigator: moduleNavigator,
                            pocket: pocket
                        )
                    }
                }
            },
            openCollection: { moduleNavigator.openPocket() },
            refuse: { message in PocketRefusalPresenter.show(message) }
        )
    }
}
