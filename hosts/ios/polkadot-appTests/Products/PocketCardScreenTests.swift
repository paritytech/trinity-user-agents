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

    /// A card that declares its face away must open onto the page alone, not
    /// show the face and then scroll it off. A screen can be laid out before it
    /// is given its size, and a face placed then would be lost when it arrives.
    @Test
    func opensWithTheFaceAwayOnceItHasItsSize() {
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

    /// The user owns the face from the start of a drag until it stops moving,
    /// not only while the finger is down: a request honoured while a released
    /// face still glides would be overridden by that glide, and the page told
    /// it had worked.
    @Test
    func refusesToMoveTheFaceFromADragUntilItsGlideEnds() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)

        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        let whileDragging = screen.setFaceShown(false, animated: false)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)
        let whileGliding = screen.setFaceShown(false, animated: false)

        #expect([whileDragging, whileGliding] == [.userMoving, .userMoving])
        #expect(scrollView.contentOffset.y == 0)
    }

    /// The page is only refused while the user moves the face; once it rests,
    /// whether its glide ran out or a tap stopped it, the page may move it again.
    /// A tap that stops a glide ends a drag with no end-of-glide callback.
    @Test
    func honoursRequestsAgainOnceTheFaceComesToRest() throws {
        let screen = laidOutScreen(product: StubSPAView())
        let scrollView = try #require(screen.scrollView)

        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)
        scrollView.delegate?.scrollViewDidEndDecelerating?(scrollView)

        #expect(screen.setFaceShown(false, animated: false) == .applied)
        #expect(scrollView.contentOffset.y == faceHeight)

        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: true)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: false)

        #expect(screen.setFaceShown(true, animated: false) == .applied)
        #expect(scrollView.contentOffset.y == 0)
    }

    /// The page is fitted only once the face rests, so while the user carries
    /// the face it must reach the bottom of the screen, or a blank strip would
    /// open under it. That holds when the drag cut short a move the page
    /// started, whose end then arrives mid-drag.
    @Test
    func laysThePageUnderTheWholeScreenWhileTheUserCarriesTheFace() throws {
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

    /// A page can change its mind before the face has started to move. The
    /// face must end where the last request put it, since that request was
    /// answered `.applied`.
    @Test
    func endsWhereTheLastRequestPutTheFaceWhenAskedTwiceInOneFrame() throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, surface: PocketCardSurface())
        let window = showing(screen)
        let scrollView = try #require(screen.scrollView)
        let visibleHeight = scrollView.bounds.height

        #expect(screen.setFaceShown(false, animated: true) == .applied)
        #expect(screen.setFaceShown(true, animated: true) == .applied)
        // The face ends where it started, so there is no end state to wait for.
        RunLoop.main.run(until: Date().addingTimeInterval(1))
        screen.view.layoutIfNeeded()

        #expect(scrollView.contentOffset.y == 0)
        #expect(product.controller.view.frame.height == visibleHeight - faceHeight)
        withExtendedLifetime(window) {}
    }

    /// A drag takes the face away from a move the page started, so once the
    /// user lets go, the page asking for that place again must be obeyed
    /// rather than taken as a move already under way.
    @Test
    func honoursARepeatedRequestAfterADragCutsThePageMoveShort() throws {
        let screen = PocketCardScreenViewController(
            card: loyaltyCard,
            product: StubSPAView(),
            surface: PocketCardSurface()
        )
        let window = showing(screen)
        let scrollView = try #require(screen.scrollView)
        _ = screen.setFaceShown(false, animated: true)
        scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
        scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: false)

        #expect(screen.setFaceShown(false, animated: false) == .applied)

        #expect(scrollView.contentOffset.y == faceHeight)
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
private func laidOutScreen(product: SPAViewProtocol) -> PocketCardScreenViewController {
    let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, surface: PocketCardSurface())
    layOut(screen)

    return screen
}

@MainActor
private func layOut(_ screen: PocketCardScreenViewController) {
    screen.view.frame = CGRect(origin: .zero, size: screenSize)
    screen.view.layoutIfNeeded()
}
