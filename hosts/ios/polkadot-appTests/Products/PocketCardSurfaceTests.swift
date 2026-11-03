import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card's product outlives the screens that show it, and a card pressed
/// again while its product is on display reloads that product rather than
/// building a second screen. The surface must therefore name the screen the
/// user sees, not one built and never shown, and stop naming it once it
/// starts to close.
@MainActor
struct PocketCardSurfaceTests {
    /// Walks one card through its life. A screen built but never shown must
    /// not take the surface from the one the user sees, and a card that has
    /// been popped is no longer on display.
    @Test
    func isOnDisplayOnlyWhileTheClaimedScreenIs() {
        let surface = PocketCardSurface()
        let neverShown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let navigation = UINavigationController(rootViewController: UIViewController())

        surface.claim(neverShown)
        let beforeShown = surface.isOnDisplay
        navigation.pushViewController(screen, animated: false)
        surface.claim(screen)
        surface.claim(neverShown)
        let whileShown = surface.isOnDisplay
        navigation.popViewController(animated: false)

        #expect([beforeShown, whileShown, surface.isOnDisplay] == [false, true, false])
    }
}
