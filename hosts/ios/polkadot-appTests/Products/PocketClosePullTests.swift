import CoreGraphics
import PolkadotUI
import Testing
@testable import polkadot_app

/// How far a pull-down has come toward closing an opened card, and whether a release closes it.
struct PocketClosePullTests {
    private let screenHeight: CGFloat = 800

    /// A short pull springs back, so a card nudged by accident returns to the top rather than closing the screen.
    @Test
    func shortPullSpringsBack() {
        let distance = DSCardCollapse.distance(in: screenHeight)
        let pull = PocketClosePull(travel: distance / 4, screenHeight: screenHeight)

        #expect(!pull.closes)
        #expect(pull.progress > 0 && pull.progress < 1)
        #expect(pull.bodyAlpha > 0 && pull.bodyAlpha < 1)
    }

    /// The collapse line is exclusive: a pull that stops exactly on it springs back, and one a point past it closes.
    /// Closing on the line itself would drop a card the user has only just brought to it.
    @Test
    func closesOnlyPastTheCollapseDistance() {
        let distance = DSCardCollapse.distance(in: screenHeight)

        #expect(!PocketClosePull(travel: distance, screenHeight: screenHeight).closes)
        #expect(PocketClosePull(travel: distance + 1, screenHeight: screenHeight).closes)
    }

    /// From the collapse distance on, the body is gone and its blurred copy fully shown. Progress must stop at 1, or a
    /// long pull would push the body's alpha below zero and the blurred copy's above one.
    @Test
    func bodyIsGoneAndFullyBlurredFromTheDistanceOnward() {
        let distance = DSCardCollapse.distance(in: screenHeight)

        for travel in [distance, distance * 2] {
            let pull = PocketClosePull(travel: travel, screenHeight: screenHeight)
            #expect(pull.progress == 1)
            #expect(pull.bodyAlpha == 0)
        }
    }

    /// An upward drag is no pull, so the card stays put and the body stays in full view.
    @Test
    func upwardTravelCountsAsNoTravel() {
        let pull = PocketClosePull(travel: -40, screenHeight: screenHeight)

        #expect(!pull.closes)
        #expect(pull.progress == 0)
        #expect(pull.bodyAlpha == 1)
    }
}
