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
        withExtendedLifetime(window) {}
    }

    /// A card the Pocket cannot place, such as one opened from a link, has no
    /// slot to fly from, so it opens as any screen does and hides nothing.
    @Test
    func opensACardThePocketCannotPlaceWithoutHidingIt() async throws {
        let origin = FakePocketCardOrigin(slot: nil)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        let window = try await presentCard(screen)

        #expect(origin.openCard == nil)
        withExtendedLifetime((window, origin)) {}
    }

    /// The flight turns everything but the card see-through so the Pocket shows
    /// behind it. Once the card lands, the screen must be the one an open
    /// without a flight gives, or the page sits on nothing.
    @Test
    func leavesTheScreenAsAnOpenWithoutAFlightWouldOnceTheCardLands() async throws {
        let flown = try await openedLook(slot: slot)
        let unflown = try await openedLook(slot: nil)

        #expect(flown == unflown)
    }

    /// The product outlives its screen and is shown again by the next open,
    /// which may have no flight to make it visible again.
    @Test
    func leavesTheProductVisibleAfterFlyingTheCardBack() async throws {
        let product = StubSPAView()
        let origin = FakePocketCardOrigin(slot: slot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, origin: origin)

        let window = try await presentCard(screen)
        await closeCard(screen)

        #expect(product.controller.view.alpha == 1)
        withExtendedLifetime((window, origin)) {}
    }

    /// The card flies with its face shown and lands where the Pocket's card
    /// would be taken out. A published `faceShown: false` that answers during
    /// the flight folds the face away after landing, as on Android, rather
    /// than sending the card off the top mid-flight.
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

/// What a flight turns see-through or moves, read off a screen once it has opened.
private struct ScreenLook: Equatable {
    let productAlpha: CGFloat
    let navigationBarAlpha: CGFloat?
    let background: UIColor?
    let navigationBackground: UIColor?
    let clipsToScreen: Bool?
    let faceTransform: CGAffineTransform?
}

@MainActor
private func openedLook(slot: CGRect?) async throws -> ScreenLook {
    let product = StubSPAView()
    let origin = FakePocketCardOrigin(slot: slot)
    let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, origin: origin)
    let window = try await presentCard(screen)
    let traits = screen.traitCollection
    let face = screen.children.compactMap { $0 as? UIHostingController<PocketOpenedCardView> }.first

    defer { withExtendedLifetime((window, origin)) {} }

    return ScreenLook(
        productAlpha: product.controller.view.alpha,
        navigationBarAlpha: screen.navigationController?.navigationBar.alpha,
        background: screen.view.backgroundColor?.resolvedColor(with: traits),
        navigationBackground: screen.navigationController?.view.backgroundColor?.resolvedColor(with: traits),
        clipsToScreen: screen.scrollView?.clipsToBounds,
        faceTransform: face?.view.transform
    )
}
