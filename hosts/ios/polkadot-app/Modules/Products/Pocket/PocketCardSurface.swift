import TrUAPIHost
import UIKit

/// Where a card's page reaches the screen that shows its face.
///
/// Held with the product rather than the screen, since the product outlives
/// the screens that show it: the card reopened is a new screen, and the page
/// already loaded under it must find that one.
@MainActor
final class PocketCardSurface: ExpandedCardFaceShowing {
    private weak var screen: PocketCardScreenViewController?

    var isOnDisplay: Bool {
        screen?.isOnDisplay == true
    }

    /// Points the surface at `screen`. Only a screen on display may take it,
    /// so one built but never shown cannot take it from the one the user sees.
    func claim(_ screen: PocketCardScreenViewController) {
        guard screen.isOnDisplay else { return }

        self.screen = screen
    }

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        guard let screen, screen.isOnDisplay else { return .notPresented }

        return screen.setFaceShown(shown, animated: true)
    }
}
