import Products
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A card's page asks for the face above it to move, and the app keeps that
/// page loaded after its card is closed. Only a card the user can see may
/// move, and a page asking from behind a closed card must hear so at once
/// rather than wait on a screen that is gone.
@MainActor
struct PocketCardSurfaceTests {
    /// Walks one card through its life. A screen built but never shown must
    /// not take the surface from the one the user sees, and a card that has
    /// started to close must not move as it goes.
    @Test
    func answersForTheCardOnlyWhileItIsOnDisplay() {
        let surface = PocketCardSurface()
        let neverShown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let navigation = UINavigationController(rootViewController: UIViewController())

        surface.claim(neverShown)
        let beforeShown = surface.setFaceShown(false)
        navigation.pushViewController(screen, animated: false)
        surface.claim(screen)
        surface.claim(neverShown)
        let whileShown = surface.setFaceShown(false)
        navigation.popViewController(animated: false)

        #expect([beforeShown, whileShown, surface.setFaceShown(false)] == [.notPresented, .applied, .notPresented])
    }
}
