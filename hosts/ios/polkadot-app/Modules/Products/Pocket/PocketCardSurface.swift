import UIKit

/// The screen a card's product is on display in, if any.
///
/// Held with the product rather than the screen, since the product outlives
/// the screens that show it: the card reopened is a new screen, and the page
/// already loaded under it must find that one.
@MainActor
final class PocketCardSurface {
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
}
