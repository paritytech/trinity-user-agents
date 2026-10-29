import Testing
import UIKit
@testable import polkadot_app

/// A card's page may draw with the user's finger, a scratch card or a slider,
/// so only the face and the navigation bar above it move the face.
@MainActor
struct PocketCardFaceDragTests {
    /// The page's drags are the page's whether the face is shown or hidden,
    /// or a scratch would carry the card away mid-stroke.
    @Test
    func onlyADragStartedOnTheFaceMovesIt() async throws {
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let window = try await presentCard(screen)
        let scrollView = try #require(screen.scrollView as? FaceScrollView)
        let faceHeight = PocketOpenedCardView.height

        #expect(scrollView.movesFace(from: CGPoint(x: 100, y: faceHeight / 2)))
        #expect(!scrollView.movesFace(from: CGPoint(x: 100, y: faceHeight + 100)))

        _ = screen.setFaceShown(false, animated: false)
        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == faceHeight })
        #expect(!scrollView.movesFace(from: CGPoint(x: 100, y: faceHeight + 10)))
        withExtendedLifetime(window) {}
    }

    /// Once the face is hidden the page offers no way to bring it back, so the
    /// navigation bar does. Let go before halfway, the face goes back where it was.
    @Test
    func aDragOnTheBarBringsAHiddenFaceBack() async throws {
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let window = try await presentCard(screen)
        let scrollView = try #require(screen.scrollView)
        let faceHeight = PocketOpenedCardView.height
        let bar = try #require(screen.navigationController?.navigationBar)
        #expect(bar.gestureRecognizers?.contains { $0.delegate === screen } == true)
        _ = screen.setFaceShown(false, animated: false)
        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == faceHeight })

        screen.dragFace(by: faceHeight * 0.4)
        #expect(screen.setFaceShown(false, animated: false) == .userMoving)
        screen.releaseFace()
        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == faceHeight })

        screen.dragFace(by: faceHeight * 0.6)
        screen.releaseFace()
        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == 0 })
        #expect(screen.setFaceShown(false, animated: false) == .applied)
        withExtendedLifetime(window) {}
    }
}
