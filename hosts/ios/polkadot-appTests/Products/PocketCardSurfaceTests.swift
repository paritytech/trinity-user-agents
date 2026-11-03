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
    /// not take the surface from the one the user sees, a full-screen view
    /// the page presents over the card leaves the card on display, and a card
    /// that has started to close is no longer on display.
    @Test
    func isOnDisplayOnlyWhileTheClaimedScreenIs() async throws {
        let surface = PocketCardSurface()
        let neverShown = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let presenter = UIViewController()
        let window = try showing(presenter)

        #expect(!surface.isOnDisplay)
        surface.claim(neverShown)
        #expect(!surface.isOnDisplay)

        let (presented, finishPresenting) = AsyncStream<Void>.makeStream()
        presenter.present(cardNavigation(screen), animated: true) { finishPresenting.finish() }
        surface.claim(screen)
        #expect(surface.isOnDisplay)
        for await _ in presented {}

        surface.claim(neverShown)
        #expect(surface.isOnDisplay)

        let cover = UIViewController()
        cover.modalPresentationStyle = .fullScreen
        await present(cover, from: screen)
        #expect(surface.isOnDisplay)
        await closeCard(cover)

        let (closed, finishClosing) = AsyncStream<Void>.makeStream()
        screen.dismiss(animated: true) { finishClosing.finish() }
        #expect(!surface.isOnDisplay)
        for await _ in closed {}
        withExtendedLifetime((window, neverShown)) {}
    }
}
