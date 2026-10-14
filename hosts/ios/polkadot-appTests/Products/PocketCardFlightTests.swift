import Products
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place in the Pocket to the top
/// of its screen, and back when closed, so the user sees one card move rather
/// than a second copy slide up over the first.
@MainActor
struct PocketCardFlightTests {
    /// The screen draws the card for as long as it is up, so the Pocket's own
    /// copy is hidden for that long. However the screen is closed, the Pocket
    /// must get its card back, or a slot in it stays empty.
    @Test(arguments: [true, false])
    func hidesTheCardInThePocketWhileItsScreenIsUp(closingAnimated: Bool) async throws {
        let origin = FakePocketCardOrigin(slot: slot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        let window = try await presentCard(screen)
        let whileOpen = origin.openCard
        await closeCard(screen, animated: closingAnimated)

        #expect([whileOpen, origin.openCard] == [loyaltyCard.key, nil])
        withExtendedLifetime((window, origin)) {}
    }

    /// The flight makes all but the card see-through so the Pocket shows
    /// behind it. The screen it lands must be the one an open without a flight
    /// gives, and the product, which the next open reuses, visible after it.
    @Test
    func leavesTheScreenAndProductAsAnOpenWithoutAFlight() async throws {
        let flown = try await lookOfACardOpenedAndClosed(slot: slot)
        let unflown = try await lookOfACardOpenedAndClosed(slot: nil)

        #expect(flown == unflown)
    }

    /// A published `faceShown: false` that answers during the flight folds the
    /// face away once the card lands, as on Android, rather than sending the
    /// card off the top mid-flight.
    @Test
    func foldsAPublishedFaceAwayOnlyOnceTheCardLands() async throws {
        let origin = FakePocketCardOrigin(slot: slot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        let presenter = UIViewController()
        let window = try showing(presenter)
        var offsetOnLanding: CGFloat?

        await withCheckedContinuation { landed in
            presenter.present(cardNavigation(screen), animated: true) {
                offsetOnLanding = screen.scrollView?.contentOffset.y
                landed.resume()
            }
            screen.applyOpeningFace(shown: false)
        }

        #expect(offsetOnLanding == 0)
        #expect(waitUntil(on: screen) { screen.scrollView?.contentOffset.y == PocketOpenedCardView.height })
        withExtendedLifetime((window, origin)) {}
    }
}

// MARK: - Fixtures

private let slot = CGRect(x: 24, y: 420, width: 345, height: PocketCardSize.height)

@MainActor
private final class FakePocketCardOrigin: PocketCardOrigin {
    private let slot: CGRect?
    private(set) var openCard: PocketCardKey?

    init(slot: CGRect?) {
        self.slot = slot
    }

    func frame(of _: PocketCardKey) -> CGRect? {
        slot
    }

    func setOpenCard(_ key: PocketCardKey?) {
        openCard = key
    }
}

/// What a flight turns see-through or moves: read off the opened screen, and
/// off the product once the screen has closed.
private struct ScreenLook: Equatable {
    let productAlpha: CGFloat
    let navigationBarAlpha: CGFloat?
    let background: UIColor?
    let navigationBackground: UIColor?
    let clipsToScreen: Bool?
    let faceTransform: CGAffineTransform?
    var productAlphaAfterClosing: CGFloat?
}

@MainActor
private func lookOfACardOpenedAndClosed(slot: CGRect?) async throws -> ScreenLook {
    let product = StubSPAView()
    let origin = FakePocketCardOrigin(slot: slot)
    let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, origin: origin)
    let window = try await presentCard(screen)
    let traits = screen.traitCollection
    let face = screen.children.compactMap { $0 as? UIHostingController<PocketOpenedCardView> }.first

    var look = ScreenLook(
        productAlpha: product.controller.view.alpha,
        navigationBarAlpha: screen.navigationController?.navigationBar.alpha,
        background: screen.view.backgroundColor?.resolvedColor(with: traits),
        navigationBackground: screen.navigationController?.view.backgroundColor?.resolvedColor(with: traits),
        clipsToScreen: screen.scrollView?.clipsToBounds,
        faceTransform: face?.view.transform
    )
    await closeCard(screen)
    look.productAlphaAfterClosing = product.controller.view.alpha

    withExtendedLifetime((window, origin)) {}
    return look
}
