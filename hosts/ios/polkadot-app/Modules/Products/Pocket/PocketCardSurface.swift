import TrUAPIHost
import UIKit

/// Where a card's page reaches the screen that shows its face.
///
/// Held with the product rather than the screen, since the product outlives
/// the screens that show it: the card reopened is a new screen, and the page
/// already loaded under it must find that one.
@MainActor
final class PocketCardSurface: ExpandedCardFaceShowing {
    weak var screen: PocketCardScreenViewController?

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        guard let screen, screen.isOnDisplay else { return .notPresented }

        return screen.setFaceShown(shown, animated: true)
    }
}

private extension PocketCardScreenViewController {
    /// In a window, or on its way into one. It is presented inside a
    /// navigation controller, so that is what is being presented.
    var isOnDisplay: Bool {
        viewIfLoaded?.window != nil || (navigationController ?? self).isBeingPresented
    }
}
