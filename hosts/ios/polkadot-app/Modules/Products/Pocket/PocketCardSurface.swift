import TrUAPIHost
import UIKit

/// Where a card's page reaches the screen that shows its face.
///
/// Held with the product rather than the screen, since the product outlives
/// the screens that show it: the card reopened is a new screen, and the page
/// already loaded under it must find that one.
@MainActor
final class PocketCardSurface {
    /// The screen showing the card now, if any.
    weak var screen: PocketCardScreenViewController?

    /// Asks the screen to move the face, or answers `.notPresented` when no card is on display.
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        guard let screen, screen.viewIfLoaded?.window != nil else { return .notPresented }

        return screen.setFaceShown(shown, animated: true)
    }
}
