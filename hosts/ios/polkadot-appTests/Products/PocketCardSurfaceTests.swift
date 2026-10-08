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
    /// A screen never built, or built but not in a window, is not something
    /// the user is looking at.
    @Test
    func answersNotPresentedWithNoScreenOnDisplay() {
        let surface = PocketCardSurface()
        let withoutScreen = surface.setFaceShown(false)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)
        screen.loadViewIfNeeded()

        withExtendedLifetime(screen) {
            #expect([withoutScreen, surface.setFaceShown(false)] == [.notPresented, .notPresented])
        }
    }

    @Test
    func handsTheRequestToTheScreenOnDisplay() {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)

        withExtendedLifetime(showing(screen)) {
            #expect(surface.setFaceShown(false) == .applied)
        }
    }

    /// The surface outlives its screens, and the same card reopened gets a new
    /// screen before the old one is torn down, so only the screen the surface
    /// points at may let go of it.
    @Test
    func forgetsOnlyTheScreenItPointsAtWhenThatScreenHandsBack() {
        let surface = PocketCardSurface()
        let older = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)
        let newer = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)

        older.handBackProduct()
        #expect(surface.screen === newer)

        newer.handBackProduct()
        withExtendedLifetime(newer) {
            #expect(surface.screen == nil)
        }
    }
}
