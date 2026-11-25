import CoreGraphics
import Foundation
import Testing
@testable import polkadot_app

/// A swipe from the edge scrubs the close of an opened card, so every moment
/// of it has to be a moment of the close the timed flight plays.
struct PocketFlightChoreographyTests {
    private let toSlot = CGPoint(x: 12, y: 420)
    private let width: CGFloat = 393

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
            clearance: 1,
            screenAlpha: 1
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
            clearance: 0,
            screenAlpha: 1
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
            clearance: 0.5,
            screenAlpha: 1
        )

        #expect(choreography.frame(progress: 0.5) == halfWay)
    }

    /// The page leaves ahead of the card, as in the timed close, so the
    /// Pocket's cards never come back over it.
    @Test(arguments: [(0.1, 0.5), (0.2, 0), (0.6, 0)] as [(CGFloat, CGFloat)])
    func thePageIsGoneAFifthOfTheWayBack(progress: CGFloat, pageAlpha: CGFloat) {
        #expect(choreography.frame(progress: progress).pageAlpha == pageAlpha)
    }

    /// A finger can leave the screen's edges, and the card must not pass its
    /// slot or its open place when it does.
    @Test(arguments: [(-40, 0), (0, 0), (196.5, 0.5), (393, 1), (600, 1)] as [(CGFloat, CGFloat)])
    func progressFollowsTheFingerAndStopsAtBothEnds(travel: CGFloat, progress: CGFloat) {
        #expect(PocketFlightChoreography.progress(travel: travel, width: width) == progress)
    }

    /// A touch at the edge must not close a card by accident, and a flick must
    /// not need a long drag. A slow drag is not a flick however far it would coast.
    @Test(arguments: [
        (118, 0, true),
        (117, 0, false),
        (0, 400, true),
        (40, 1_200, true),
        (40, 200, false),
        (100, 250, false)
    ] as [(CGFloat, CGFloat, Bool)])
    func aReleaseClosesWhenItCameFarEnoughOrWasFlung(travel: CGFloat, velocity: CGFloat, commits: Bool) {
        #expect(PocketFlightChoreography.commits(travel: travel, velocity: velocity, width: width) == commits)
    }

    /// The release takes the flight's time for the part of the way still to
    /// go, and a release with almost nothing left still animates.
    @Test(arguments: [(1, 0.5), (0.5, 0.25), (0.1, 0.15), (0, 0.15)] as [(CGFloat, TimeInterval)])
    func aReleaseTakesItsShareOfTheFlightsTime(remaining: CGFloat, duration: TimeInterval) {
        #expect(choreography.releaseDuration(remaining: remaining) == duration)
    }

    /// Under Reduce Motion, or with no slot to fly to, the close is a fade:
    /// the screen's visibility follows the finger and nothing moves.
    @Test
    func aFadeFollowsTheFingerWithNothingMoving() {
        let fading = PocketFlightFrame(
            itemOffset: .zero,
            pocketOffset: .zero,
            pageAlpha: 1,
            clearance: 0,
            screenAlpha: 0.75
        )

        #expect(PocketFlightChoreography(toSlot: nil).frame(progress: 0.25) == fading)
    }

    /// A released fade takes its share of the crossfade's time, which is
    /// short enough to need no floor.
    @Test(arguments: [(1, 0.18), (0.5, 0.09), (0, 0)] as [(CGFloat, TimeInterval)])
    func aReleasedFadeTakesItsShareOfTheCrossfadesTime(remaining: CGFloat, duration: TimeInterval) {
        #expect(PocketFlightChoreography(toSlot: nil).releaseDuration(remaining: remaining) == duration)
    }
}
