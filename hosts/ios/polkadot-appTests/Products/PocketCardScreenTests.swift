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
    /// The page lays itself out into the viewport it is given, so it is sized
    /// to the screen the face leaves it, or its bottom rows would sit
    /// off-screen with nothing to scroll them into view. The scroll stays a
    /// screenful plus the face either way, since fitting the page must not
    /// take away the room the face scrolls into.
    @Test
    func sizesThePageToTheScreenTheFaceLeaves() throws {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)
        let scrollView = try #require(screen.scrollView)
        let contentHeight = faceHeight + screenSize.height

        #expect(product.controller.view.frame.height == screenSize.height - faceHeight)
        #expect(scrollView.contentSize.height == contentHeight)

        let outcome = screen.setFaceShown(false, animated: false)
        screen.view.layoutIfNeeded()

        #expect(outcome == .applied)
        #expect(scrollView.contentOffset.y == faceHeight)
        #expect(product.controller.view.frame.height == screenSize.height)
        #expect(scrollView.contentSize.height == contentHeight)
    }

    /// A card whose product publishes its face away, and answers while the
    /// card is still coming up, must open onto the page alone, not show the
    /// face and then scroll it off.
    @Test
    func opensWithThePublishedFaceAwayWhenTheAnswerComesFirst() throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product)
        let presenter = UIViewController()
        let window = try showing(presenter)
        presenter.present(cardNavigation(screen), animated: true)

        screen.applyOpeningFace(shown: false)

        #expect(waitUntil(on: screen) {
            screen.scrollView?.contentOffset.y == faceHeight
                && product.controller.view.frame.height == screen.scrollView?.bounds.height
        })
        withExtendedLifetime(window) {}
    }

    /// The card is shown before its product is asked, so an answer that comes
    /// once the card is on screen must still fold the face away.
    @Test
    func foldsTheFaceWhenTheAnswerComesAfterTheCardIsShown() async throws {
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let window = try await presentCard(screen)
        let scrollView = try #require(screen.scrollView)

        screen.applyOpeningFace(shown: false)

        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == faceHeight })
        withExtendedLifetime(window) {}
    }

    enum FaceMover { case page, user }

    /// The product's answer can come after the page has asked for the face or
    /// the user has moved it, and must not take the face from where they left
    /// it. Checked at once, before any move could finish: a move started
    /// would already have grown the page.
    @Test(arguments: [FaceMover.page, .user])
    func ignoresThePublishedFaceOnceTheFaceHasMoved(by mover: FaceMover) async throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product)
        let window = try await presentCard(screen)
        let scrollView = try #require(screen.scrollView)
        switch mover {
        case .page:
            _ = screen.setFaceShown(true, animated: false)
        case .user:
            scrollView.delegate?.scrollViewWillBeginDragging?(scrollView)
            scrollView.delegate?.scrollViewDidEndDragging?(scrollView, willDecelerate: false)
        }

        screen.applyOpeningFace(shown: false)
        screen.view.layoutIfNeeded()

        #expect(scrollView.contentOffset.y == 0)
        #expect(product.controller.view.frame.height == scrollView.bounds.height - faceHeight)
        withExtendedLifetime(window) {}
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

    /// A page can change its mind before the face has started to move. The
    /// face must end where the last request put it, since that request was
    /// answered `.applied`.
    @Test
    func endsWhereTheLastRequestPutTheFaceWhenAskedTwiceInOneFrame() throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product)
        let window = try showing(screen)
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
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())

        screen.loadViewIfNeeded()

        #expect(screen.title == "Loyalty")
    }

    /// The product is kept warm for the next tap on the same card, which only
    /// works if closing the card let go of it. Presented as the app presents
    /// it, inside a navigation controller.
    @Test
    func handsTheProductBackWhenTheCardIsClosed() async throws {
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product)
        let window = try await presentCard(screen)

        await closeCard(screen)

        #expect(product.controller.parent == nil)
        #expect(product.controller.view.superview == nil)
        withExtendedLifetime(window) {}
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

private let screenSize = CGSize(width: 393, height: 800)

@MainActor
private func laidOutScreen(product: SPAViewProtocol) -> PocketCardScreenViewController {
    let screen = PocketCardScreenViewController(card: loyaltyCard, product: product)

    screen.view.frame = CGRect(origin: .zero, size: screenSize)
    screen.view.layoutIfNeeded()

    return screen
}
