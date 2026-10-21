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
    /// Walks one card through its life. The page can ask while its screen is
    /// still on its way up, and while the page covers the card with a
    /// full-screen view of its own, such as a contact picker. A screen built
    /// but never shown must not take the surface from the one the user sees,
    /// and a card that has started to close must not move as it goes.
    @Test
    func answersForTheCardOnlyWhileItIsOnDisplay() async throws {
        let surface = PocketCardSurface()
        let neverShown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let (navigation, window) = try walletStack()

        #expect(surface.setFaceShown(false) == .notPresented)
        surface.claim(neverShown)
        #expect(surface.setFaceShown(false) == .notPresented)

        navigation.pushViewController(screen, animated: true)
        surface.claim(screen)
        #expect(surface.setFaceShown(false) == .applied)
        await transitionFinished(in: navigation)
        #expect(waitUntil(on: screen) { screen.scrollView?.contentOffset.y == PocketOpenedCardView.height })

        surface.claim(neverShown)
        #expect(surface.setFaceShown(false) == .applied)

        let cover = UIViewController()
        cover.modalPresentationStyle = .fullScreen
        await present(cover, from: screen)
        #expect(surface.setFaceShown(false) == .applied)
        await dismiss(cover)

        navigation.popViewController(animated: true)
        #expect(surface.setFaceShown(false) == .notPresented)
        await transitionFinished(in: navigation)
        withExtendedLifetime((window, neverShown)) {}
    }
}
