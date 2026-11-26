import CoreGraphics
import Foundation
import Testing
@testable import polkadot_app

/// A swipe from the edge scrubs the close of an opened card, so every moment
/// of it has to be a moment of the close the timed flight plays.
struct PocketFlightChoreographyTests {
    private let toSlot = CGPoint(x: 12, y: 420)

    private var choreography: PocketFlightChoreography {
        PocketFlightChoreography(toSlot: toSlot)
    }

    /// The swipe starts where the open flight left everything: the card at the
    /// top of its screen, drawn by the Pocket lifted out of its slot, with the
    /// rest of the Pocket cleared and the page showing.
    @Test
    func startsOnTheOpenedCard() {
        let opened = PocketFlightFrame(
            itemOffset: .zero,
            pocketOffset: CGSize(width: -12, height: -420),
            pageAlpha: 1,
            clearance: 1
        )

        #expect(choreography.frame(progress: 0) == opened)
    }

    /// The swipe ends where the timed close does: the card in its slot, the
    /// Pocket at rest and the page gone.
    @Test
    func endsWithTheCardInItsSlotAndThePocketAtRest() {
        let closed = PocketFlightFrame(
            itemOffset: toSlot,
            pocketOffset: .zero,
            pageAlpha: 0,
            clearance: 0
        )

        #expect(choreography.frame(progress: 1) == closed)
    }

    /// The card and the Pocket move by the same share of the way, so the Pocket
    /// keeps drawing the card exactly where the screen would.
    @Test
    func theCardAndThePocketAreHalfWayAtHalfTheSwipe() {
        let halfWay = PocketFlightFrame(
            itemOffset: CGPoint(x: 6, y: 210),
            pocketOffset: CGSize(width: -6, height: -210),
            pageAlpha: 0,
            clearance: 0.5
        )

        #expect(choreography.frame(progress: 0.5) == halfWay)
    }

    /// The page leaves ahead of the card, as in the timed close, so the
    /// Pocket's cards never come back over it.
    @Test(arguments: [(0.1, 0.5), (0.2, 0), (0.6, 0)] as [(CGFloat, CGFloat)])
    func thePageIsGoneAFifthOfTheWayBack(progress: CGFloat, pageAlpha: CGFloat) {
        #expect(choreography.frame(progress: progress).pageAlpha == pageAlpha)
    }

    /// The release takes the flight's time for the part of the way still to
    /// go, and a release with almost nothing left still animates.
    @Test(arguments: [(1, 0.5), (0.5, 0.25), (0.1, 0.15), (0, 0.15)] as [(CGFloat, TimeInterval)])
    func aReleaseTakesItsShareOfTheFlightsTime(remaining: CGFloat, duration: TimeInterval) {
        #expect(choreography.releaseDuration(remaining: remaining) == duration)
    }
}
