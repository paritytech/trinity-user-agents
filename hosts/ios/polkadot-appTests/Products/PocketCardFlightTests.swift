import Products
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place there to the top of its
/// screen, and back when closed, so the user sees one card move.
@MainActor
struct PocketCardFlightTests {
    /// The screen draws the card for as long as it is up, so the Pocket's own
    /// copy is hidden for that long. However the screen is closed, the Pocket
    /// must get its card back, or a slot in it stays empty.
    @Test(arguments: [true, false])
    func hidesTheCardInThePocketWhileItsScreenIsUp(closingAnimated: Bool) async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        let window = try await presentCard(screen)
        let whileOpen = origin.openCard
        await closeCard(screen, animated: closingAnimated)

        #expect([whileOpen, origin.openCard] == [loyaltyCard.key, nil])
        withExtendedLifetime((window, origin)) {}
    }

    /// The flight makes all but the card see-through so the Pocket shows
    /// behind it, whether the card moves or fades. The screen it lands must be
    /// the one an open without a flight gives, and the product, which the next
    /// open reuses, visible after it.
    @Test(arguments: [pocketSlot, nil])
    func leavesTheScreenAndProductAsAnOpenWithoutAFlight(slot: CGRect?) async throws {
        let origin = FakePocketCardOrigin(slot: slot)

        let flown = try await lookOfACardOpenedAndClosed(from: origin)
        let unflown = try await lookOfACardOpenedAndClosed(from: nil)

        #expect(flown == unflown)
        withExtendedLifetime(origin) {}
    }
}

// MARK: - Fixtures

/// What a flight turns see-through or moves: read off the opened screen, and
/// off the product once the screen has closed.
private struct ScreenLook: Equatable {
    let productAlpha: CGFloat
    let navigationAlpha: CGFloat?
    let navigationBarAlpha: CGFloat?
    let background: UIColor?
    let navigationBackground: UIColor?
    let clipsToScreen: Bool?
    let faceTransform: CGAffineTransform?
    var productAlphaAfterClosing: CGFloat?
}

@MainActor
private func lookOfACardOpenedAndClosed(from origin: PocketCardOrigin?) async throws -> ScreenLook {
    let product = StubSPAView()
    let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, origin: origin)
    let window = try await presentCard(screen)
    let traits = screen.traitCollection
    let face = screen.children.compactMap { $0 as? UIHostingController<PocketOpenedCardView> }.first

    var look = ScreenLook(
        productAlpha: product.controller.view.alpha,
        navigationAlpha: screen.navigationController?.view.alpha,
        navigationBarAlpha: screen.navigationController?.navigationBar.alpha,
        background: screen.view.backgroundColor?.resolvedColor(with: traits),
        navigationBackground: screen.navigationController?.view.backgroundColor?.resolvedColor(with: traits),
        clipsToScreen: screen.scrollView?.clipsToBounds,
        faceTransform: face?.view.transform
    )
    await closeCard(screen)
    look.productAlphaAfterClosing = product.controller.view.alpha

    withExtendedLifetime(window) {}
    return look
}
