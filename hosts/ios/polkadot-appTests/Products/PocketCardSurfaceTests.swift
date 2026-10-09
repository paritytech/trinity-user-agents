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
    /// A screen never built, or built but never presented, is not something
    /// the user is looking at, and cannot take the surface by claiming it.
    @Test
    func answersNotPresentedWithNoScreenOnDisplay() {
        let surface = PocketCardSurface()
        let withoutScreen = surface.setFaceShown(false)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        screen.loadViewIfNeeded()
        surface.claim(screen)

        withExtendedLifetime(screen) {
            #expect([withoutScreen, surface.setFaceShown(false)] == [.notPresented, .notPresented])
        }
    }

    /// A warm page hears its card reopen while the new screen is still on its
    /// way up, before it is in a window, and the face must still move.
    @Test
    func handsTheRequestToAScreenStillBeingPresented() {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let presenter = UIViewController()
        let window = showing(presenter)
        presenter.present(cardNavigation(screen), animated: true)
        surface.claim(screen)

        #expect(surface.setFaceShown(false) == .applied)
        #expect(waitUntil(on: screen) { screen.scrollView?.contentOffset.y == PocketOpenedCardView.height })
        withExtendedLifetime(window) {}
    }

    /// A card's page can cover the card with a full-screen view of its own,
    /// such as a contact picker. The card is still open under it, so its face
    /// still moves.
    @Test
    func keepsAnsweringForACardCoveredByAFullScreenView() async throws {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let presenter = UIViewController()
        let window = try showingInScene(presenter)
        let navigation = cardNavigation(screen)
        await present(navigation, from: presenter)
        surface.claim(screen)
        let cover = UIViewController()
        cover.modalPresentationStyle = .fullScreen

        await present(cover, from: navigation)

        #expect(surface.setFaceShown(false) == .applied)
        withExtendedLifetime(window) {}
    }

    /// A card that has started to close is on its way out, and its face must
    /// not move as it goes.
    @Test
    func answersNotPresentedOnceTheCardStartsClosing() async throws {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let presenter = UIViewController()
        let window = try showingInScene(presenter)
        await present(cardNavigation(screen), from: presenter)
        surface.claim(screen)
        let (closed, finishClosing) = AsyncStream<Void>.makeStream()

        presenter.dismiss(animated: true) { finishClosing.finish() }
        let whileClosing = surface.setFaceShown(false)
        for await _ in closed {}

        #expect(whileClosing == .notPresented)
        withExtendedLifetime(window) {}
    }

    /// The surface outlives its screens. A screen built but never shown must
    /// not take it from the one the user sees.
    @Test
    func keepsTheScreenOnDisplayWhenAnotherIsBuilt() async {
        let surface = PocketCardSurface()
        let shown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let presenter = UIViewController()
        let window = showing(presenter)
        await present(cardNavigation(shown), from: presenter)
        surface.claim(shown)
        let neverShown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())

        surface.claim(neverShown)

        #expect(surface.setFaceShown(false) == .applied)
        withExtendedLifetime((window, neverShown)) {}
    }
}
