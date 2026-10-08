import Products
import SwiftUI
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

/// A card opened is the card and its product on one screen, sharing one
/// scroll: the card is what there is to scroll past to give the product the
/// whole screen, and the product is always sized to the part of the screen
/// the card leaves it, so the page's own bottom edge is never off-screen.
@MainActor
struct PocketCardScreenTests {
    /// The page lays itself out into the viewport it is given: given a
    /// screenful under the face, its bottom rows would sit off-screen with
    /// nothing to scroll them into view.
    @Test
    func givesTheProductOnlyTheScreenTheFaceLeaves() {
        let product = StubSPAView()

        _ = laidOutScreen(product: product)

        #expect(product.controller.view.frame.height == screenSize.height - faceHeight)
    }

    /// The face is what there is to scroll past, whatever the product's own
    /// size: were the scroll sized by the page, fitting the page would take
    /// away the room the face scrolls into.
    @Test
    func keepsRoomToScrollTheFaceAwayInEitherState() {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = screen.scrollView
        let contentHeight = faceHeight + screenSize.height

        #expect(scrollView?.contentSize.height == contentHeight)

        _ = screen.setFaceShown(false, animated: false)
        screen.view.layoutIfNeeded()

        #expect(scrollView?.contentSize.height == contentHeight)
    }

    /// The page asking for the face away is the point of the feature: the
    /// face leaves the screen and the page takes all of it.
    @Test
    func hidesTheFaceAndGivesThePageTheWholeScreen() {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)

        let outcome = screen.setFaceShown(false, animated: false)
        screen.view.layoutIfNeeded()

        #expect(outcome == .applied)
        #expect(screen.scrollView?.contentOffset.y == faceHeight)
        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// A page may ask for the face where it already is, and is told it is
    /// there; the page must stay fitted rather than be left reaching under
    /// the face for a move that never happens. Shown in a window, since only
    /// there does an animated move to where the face already is end without
    /// a callback to fit the page.
    @Test
    func keepsThePageFittedWhenAskedForTheFaceWhereItIs() throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, surface: PocketCardSurface())
        let window = showing(screen)
        let scrollView = try #require(screen.scrollView)

        #expect(screen.setFaceShown(true, animated: true) == .applied)
        screen.view.layoutIfNeeded()

        #expect(product.controller.view.frame.height == scrollView.bounds.height - faceHeight)
        withExtendedLifetime(window) {}
    }

    /// A card that declares its face away must open onto the page alone, not
    /// show the face and then scroll it off.
    @Test
    func opensWithTheFaceAwayWhenTheCardAsks() {
        let product = StubSPAView()

        let screen = laidOutScreen(product: product, faceShown: false)

        #expect(screen.scrollView?.contentOffset.y == faceHeight)
        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// A warm page can ask as soon as its screen is built, before the screen
    /// knows its size; the request must not be lost.
    @Test
    func appliesARequestMadeBeforeTheScreenIsLaidOut() {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, surface: PocketCardSurface())

        let outcome = screen.setFaceShown(false, animated: true)
        layOut(screen)

        #expect(outcome == .applied)
        #expect(screen.scrollView?.contentOffset.y == faceHeight)
        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// A screen can be laid out before it is given its size; a face placed
    /// then would be lost when the size arrives.
    @Test
    func waitsForItsSizeToPlaceTheOpeningFace() {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(
            card: loyaltyCard,
            product: product,
            surface: PocketCardSurface(),
            faceShown: false
        )
        screen.view.frame = .zero
        screen.view.layoutIfNeeded()

        layOut(screen)

        #expect(screen.scrollView?.contentOffset.y == faceHeight)
        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// The user's finger decides while it is down; the page must be told so
    /// rather than fight it.
    @Test
    func refusesToMoveTheFaceWhileTheUserDragsIt() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)

        let outcome = screen.setFaceShown(false, animated: false)

        #expect(outcome == .userMoving)
        #expect(scrollView.contentOffset.y == 0)
    }

    /// The user owns the face until it stops moving, not only while the
    /// finger is down: a request honoured while a released face still glides
    /// would be overridden by that glide, and the page told it had worked.
    @Test
    func refusesToMoveTheFaceWhileItGlidesAfterADrag() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)

        let outcome = screen.setFaceShown(false, animated: false)

        #expect(outcome == .userMoving)
        #expect(scrollView.contentOffset.y == 0)
    }

    /// The page is only refused while the user moves the face; once it rests,
    /// the page may move it again.
    @Test
    func honoursRequestsAgainOnceTheFaceComesToRest() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)
        scrollView.delegate?.scrollViewDidEndDecelerating?(scrollView)

        let outcome = screen.setFaceShown(false, animated: false)

        #expect(outcome == .applied)
        #expect(scrollView.contentOffset.y == faceHeight)
    }

    /// A tap that stops a gliding face ends a drag with no glide after it and
    /// no end-of-glide callback; the face is at rest, so the page may move it.
    @Test
    func honoursRequestsAfterATapStopsTheGlidingFace() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: false)

        let outcome = screen.setFaceShown(false, animated: false)

        #expect(outcome == .applied)
        #expect(scrollView.contentOffset.y == faceHeight)
    }

    /// The page is fitted only once the face rests, so while the face scrolls
    /// away the page must already reach the bottom of the screen, or a blank
    /// strip would open under it.
    @Test
    func laysThePageUnderTheWholeScreenWhileTheUserDrags() throws {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)
        let scrollView = try #require(screen.scrollView)

        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.contentOffset.y = 100
        screen.view.layoutIfNeeded()

        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// A drag can take over a move the page started; that move's end must not
    /// fit the page while the user is still carrying the face.
    @Test
    func leavesThePageUnderTheWholeScreenWhenADragCutsAPageMoveShort() throws {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.contentOffset.y = 100

        scrollView.delegate?.scrollViewDidEndScrollingAnimation?(scrollView)
        screen.view.layoutIfNeeded()

        #expect(product.controller.view.frame.height == screenSize.height)
    }

    /// Wherever the user leaves the face, the page fits what is left of the
    /// screen once it rests, so its bottom edge is back in view.
    @Test
    func fitsThePageToWhereTheUserLeftTheFace() throws {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)
        let scrollView = try #require(screen.scrollView)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.contentOffset.y = 100
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: false)
        screen.view.layoutIfNeeded()

        #expect(product.controller.view.frame.height == screenSize.height - (faceHeight - 100))
    }

    /// The page is resized only once the face rests, since a resize on every
    /// frame of the move would relayout it each time; until then it reaches
    /// the bottom of the screen, so no blank strip opens under it. Shown in a
    /// window, since only there does the move animate.
    @Test
    func fitsThePageOnlyOnceTheFaceItMovedComesToRest() throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, surface: PocketCardSurface())
        let window = showing(screen)
        let scrollView = try #require(screen.scrollView)
        let visibleHeight = scrollView.bounds.height

        #expect(screen.setFaceShown(false, animated: true) == .applied)
        screen.view.layoutIfNeeded()

        #expect(product.controller.view.frame.height == visibleHeight)

        try #require(waitUntil(on: screen) { scrollView.contentOffset.y == faceHeight })

        #expect(product.controller.view.frame.height == visibleHeight)

        #expect(screen.setFaceShown(true, animated: true) == .applied)
        try #require(waitUntil(on: screen) {
            product.controller.view.frame.height == visibleHeight - faceHeight
        })

        #expect(scrollView.contentOffset.y == 0)
        withExtendedLifetime(window) {}
    }

    /// A product page that fits its screenful has nothing to scroll, and its
    /// own rubber-banding would swallow the gesture meant for the card.
    @Test
    func leavesTheCardsGestureToTheScreenWhenThePageFits() {
        let product = StubSPAView()

        _ = laidOutScreen(product: product)

        #expect(product.pageScrollView.bounces == false)
    }

    /// The screen borrows a product that outlives it, so the title belongs to
    /// the card rather than to whatever the product later calls itself.
    @Test
    func takesItsTitleFromTheCard() {
        let screen = PocketCardScreenViewController(
            card: loyaltyCard,
            product: StubSPAView(),
            surface: PocketCardSurface()
        )

        screen.loadViewIfNeeded()

        #expect(screen.title == "Loyalty")
    }

    /// The product is kept warm for the next tap on the same card, which only
    /// works if the screen it was shown on let go of it.
    @Test
    func handsTheProductBackFreeToShowAgain() {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)

        screen.handBackProduct()

        #expect(product.controller.parent == nil)
        #expect(product.controller.view.superview == nil)
    }

    /// A hidden face sits under the status bar. Were it to inset itself for
    /// the safe area there, it could carry the inset back when shown and draw
    /// pushed down, its foot cut off by the page.
    @Test
    func drawsTheFaceWithoutSafeAreaInsets() {
        let screen = laidOutScreen(product: StubSPAView())

        let face = screen.children.compactMap { $0 as? UIHostingController<PocketOpenedCardView> }.first

        #expect(face?.safeAreaRegions == [])
    }
}

// MARK: - Fixtures

private let faceHeight = PocketOpenedCardView.height

@MainActor
private func laidOutScreen(product: SPAViewProtocol, faceShown: Bool = true) -> PocketCardScreenViewController {
    let screen = PocketCardScreenViewController(
        card: loyaltyCard,
        product: product,
        surface: PocketCardSurface(),
        faceShown: faceShown
    )
    layOut(screen)

    return screen
}

@MainActor
private func layOut(_ screen: PocketCardScreenViewController) {
    screen.view.frame = CGRect(origin: .zero, size: screenSize)
    screen.view.layoutIfNeeded()
}

@MainActor
private func waitUntil(on screen: UIViewController, _ condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(5)

    while Date() < deadline {
        screen.view.layoutIfNeeded()

        if condition() { return true }

        RunLoop.main.run(until: Date().addingTimeInterval(0.02))
    }

    return false
}

private extension PocketCardScreenViewController {
    var scrollView: UIScrollView? {
        view.subviews.compactMap { $0 as? UIScrollView }.first
    }
}
