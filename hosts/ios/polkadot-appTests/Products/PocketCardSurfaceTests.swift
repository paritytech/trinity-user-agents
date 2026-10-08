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
    @Test
    func answersNotPresentedWithNoScreen() {
        #expect(PocketCardSurface().setFaceShown(false) == .notPresented)
    }

    /// A screen that was built but never shown, or one already taken off
    /// screen, is not something the user is looking at.
    @Test
    func answersNotPresentedForAScreenOutsideAWindow() {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)
        screen.loadViewIfNeeded()

        #expect(surface.setFaceShown(false) == .notPresented)
    }

    @Test
    func handsTheRequestToTheScreenOnDisplay() {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)

        withExtendedLifetime(showing(screen)) {
            #expect(surface.setFaceShown(false) == .applied)
        }
    }

    /// The surface outlives the screen, so a screen that handed its product
    /// back must stop answering for the card.
    @Test
    func forgetsAScreenThatHandedItsProductBack() throws {
        let surface = PocketCardSurface()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)
        try #require(surface.screen === screen)

        screen.handBackProduct()

        #expect(surface.screen == nil)
    }

    /// The same card reopened gets a new screen before the old one is torn
    /// down, and the old one letting go must not take the new one's place.
    @Test
    func keepsTheNewerScreenWhenAnOlderOneHandsBack() {
        let surface = PocketCardSurface()
        let older = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)
        let newer = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), surface: surface)

        older.handBackProduct()

        #expect(surface.screen === newer)
    }
}
