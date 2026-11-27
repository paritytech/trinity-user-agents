import CoreGraphics
import Testing
@testable import polkadot_app

struct BackSwipeTests {
    private let width: CGFloat = 393

    /// A finger can leave the screen's edges, and the card must not pass its
    /// slot or its open place when it does.
    @Test(arguments: [(-40, 0), (0, 0), (196.5, 0.5), (393, 1), (600, 1)] as [(CGFloat, CGFloat)])
    func progressFollowsTheFingerAndStopsAtBothEnds(travel: CGFloat, progress: CGFloat) {
        #expect(BackSwipe.progress(travel: travel, width: width) == progress)
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
        #expect(BackSwipe.commits(travel: travel, velocity: velocity, width: width) == commits)
    }
}
