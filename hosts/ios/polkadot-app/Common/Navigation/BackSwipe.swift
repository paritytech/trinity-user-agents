import UIKit

/// A pop a back swipe can scrub. It draws itself at a progress while the finger moves, and runs the rest of the
/// way, or the way back, when the finger lets go.
@MainActor
protocol BackSwipeScrubbing: UIViewControllerInteractiveTransitioning {
    /// Draws the pop at `progress`, from 0, the screen up, to 1, the screen gone.
    func update(progress: CGFloat)

    /// Runs the rest of the pop, or the way back to the screen, from where the finger left it.
    func release(progress: CGFloat, commits: Bool)
}

/// How a back swipe's travel reads: how far the pop has come, and whether a release finishes it.
enum BackSwipe {
    private static let commitTravelFraction: CGFloat = 0.3
    private static let flingReachFraction: CGFloat = 0.5
    private static let flingSpeed: CGFloat = 300
    /// UIKit's normal deceleration, per millisecond.
    private static let decelerationRate: CGFloat = 0.998

    /// How far a swipe has closed the screen, held between 0 and 1 however far the finger goes.
    static func progress(travel: CGFloat, width: CGFloat) -> CGFloat {
        min(max(travel / max(width, 1), 0), 1)
    }

    /// Whether a released swipe closes the screen: it came far enough, or it was
    /// flung, coasting past half the width at a flick's speed.
    static func commits(travel: CGFloat, velocity: CGFloat, width: CGFloat) -> Bool {
        let coasted = travel + velocity * decelerationRate / (1 - decelerationRate) / 1_000
        let isFlung = coasted > width * flingReachFraction && velocity > flingSpeed

        return travel > width * commitTravelFraction || isFlung
    }
}
