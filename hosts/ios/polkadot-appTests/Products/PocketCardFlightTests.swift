import Products
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place there to the top of its
/// screen, and back when closed, so the user sees one card move.
@MainActor
struct PocketCardFlightTests {
    /// The Pocket draws the card while it flies, at its own depth in the stack,
    /// so the cards below it stay in front of it. Once the screen is up, the
    /// Pocket's card waits under the screen's at the same spot with the rest of
    /// the Pocket cleared away, so a close starts with no jump. However the
    /// screen is closed, the Pocket must get its card and its other cards back
    /// in place, or the stack is left broken.
    @Test(arguments: [true, false])
    func keepsDrawingTheOpenCardInThePocket(closingAnimated: Bool) async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        let window = try await presentCard(screen)
        let screenCard = screen.cardFrame(in: window)
        let whileOpen = origin.placement
        await closeCard(screen, animated: closingAnimated)

        let lifted = PocketPlacement(
            openCard: loyaltyCard.key,
            offset: CGSize(width: screenCard.minX - pocketSlot.minX, height: screenCard.minY - pocketSlot.minY),
            cleared: true
        )
        #expect([whileOpen, origin.placement] == [lifted, PocketPlacement()])
        withExtendedLifetime((window, origin)) {}
    }

    /// A card the Pocket cannot place fades with its screen instead of flying,
    /// and the Pocket is left as it was.
    @Test
    func leavesThePocketInPlaceWhenItCannotPlaceTheCard() async throws {
        let origin = FakePocketCardOrigin(slot: nil)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        let window = try await presentCard(screen)
        let whileOpen = origin.placement
        await closeCard(screen)

        #expect([whileOpen, origin.placement] == [PocketPlacement(openCard: loyaltyCard.key), PocketPlacement()])
        withExtendedLifetime((window, origin)) {}
    }

    /// A Pocket that can no longer place the card when it closes shows through
    /// a fading screen instead, so it must be whole again while it shows,
    /// not only once the screen has gone.
    @Test
    func putsThePocketBackBeforeAFadingCloseShowsIt() async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        let window = try await presentCard(screen)

        var wholeWhileShowing = false
        origin.onMove = { placement in
            wholeWhileShowing = wholeWhileShowing || !placement.cleared && screen.view.window != nil
        }
        origin.slot = nil
        screen.dismiss(animated: true)
        let closed = await eventually { screen.view.window == nil }

        #expect([wholeWhileShowing, closed] == [true, true])
        withExtendedLifetime((window, origin)) {}
    }

    /// Opening a product from a card closes the card and switches tab in the
    /// same turn. The Pocket the card would fly into is then gone, so the
    /// close must fade instead: a Pocket off screen cannot draw the card.
    @Test
    func fadesWhenThePocketGoesAwayInTheTurnTheCloseStarts() async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        let window = try await presentCard(screen)
        let movesWhileOpen = origin.moves.count

        screen.dismiss(animated: true)
        origin.slot = nil
        let closed = await eventually { screen.view.window == nil }

        let clearedWhileClosing = origin.moves.dropFirst(movesWhileOpen).contains { $0.cleared }
        #expect([closed, clearedWhileClosing] == [true, false])
        withExtendedLifetime((window, origin)) {}
    }

    /// Whatever closes the card, even a dismissal from under a sheet the card's
    /// product put over it, the Pocket gets its cards and its chrome back and
    /// is told it is uncovered.
    @Test
    func restoresThePocketWhenClosedFromUnderASheet() async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let presenter = AppearanceRecordingController()
        let window = try showing(presenter)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        await present(cardNavigation(screen), from: presenter)
        await present(UIViewController(), from: screen)

        await withCheckedContinuation { closed in
            presenter.dismiss(animated: false) { closed.resume() }
        }

        #expect(origin.placement == PocketPlacement())
        #expect(presenter.calls.suffix(4) == ["willDisappear", "didDisappear", "willAppear", "didAppear"])
        withExtendedLifetime((window, origin)) {}
    }

    /// A screen opened full screen over the card hides the Pocket too, but the
    /// Pocket was already told it is covered, and must not be told twice.
    @Test
    func tellsThePocketOnceWhenAnotherScreenOpensOverTheCard() async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let presenter = AppearanceRecordingController()
        let window = try showing(presenter)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        await present(cardNavigation(screen), from: presenter)
        let callsBefore = presenter.calls.count

        let cover = UIViewController()
        cover.modalPresentationStyle = .fullScreen
        await present(cover, from: screen)
        await closeCard(cover)
        await closeCard(screen)

        #expect(Array(presenter.calls.dropFirst(callsBefore - 2)) == [
            "willDisappear", "didDisappear", "willAppear", "didAppear"
        ])
        withExtendedLifetime((window, origin)) {}
    }

    /// A card covers the Pocket without taking it out of the window, so that the
    /// Pocket can draw the card while it flies. UIKit then no longer tells the
    /// Pocket when it is covered and uncovered, so the screen must, or what
    /// pauses while the Pocket is hidden, like the tab bar's tips, keeps running
    /// under the card.
    @Test
    func tellsThePocketBehindItWhenItCoversAndUncoversIt() async throws {
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let presenter = AppearanceRecordingController()
        let window = try showing(presenter)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)

        await present(cardNavigation(screen), from: presenter)
        await closeCard(screen)

        #expect(presenter.calls.suffix(4) == ["willDisappear", "didDisappear", "willAppear", "didAppear"])
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

/// Waits for `condition` for up to five seconds, letting the main actor run so
/// that UIKit can carry a transition on meanwhile.
@MainActor
private func eventually(_ condition: () -> Bool) async -> Bool {
    for _ in 0 ..< 250 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return false
}

/// Records the appearance calls UIKit, or a screen covering it, sends it.
private final class AppearanceRecordingController: UIViewController {
    private(set) var calls: [String] = []

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        calls.append("willAppear")
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        calls.append("didAppear")
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        calls.append("willDisappear")
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        calls.append("didDisappear")
    }
}

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
    let faceAlpha: CGFloat?
    let productTransform: CGAffineTransform
    var productAlphaAfterClosing: CGFloat?
    var productTransformAfterClosing: CGAffineTransform?
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
        faceTransform: face?.view.transform,
        faceAlpha: face?.view.alpha,
        productTransform: product.controller.view.transform
    )
    await closeCard(screen)
    look.productAlphaAfterClosing = product.controller.view.alpha
    look.productTransformAfterClosing = product.controller.view.transform

    withExtendedLifetime(window) {}
    return look
}
