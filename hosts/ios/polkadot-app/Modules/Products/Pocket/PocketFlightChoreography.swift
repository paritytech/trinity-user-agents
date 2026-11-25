import CoreGraphics
import Foundation

/// Everything a closing flight draws at one moment.
struct PocketFlightFrame: Equatable {
    /// How far the screen draws the item from its open place.
    let itemOffset: CGPoint
    /// How far the Pocket draws the item from its slot.
    let pocketOffset: CGSize
    let pageAlpha: CGFloat
    /// How far the rest of the Pocket is cleared away: 0 at rest, 1 cleared.
    let clearance: CGFloat
    /// How visible the whole screen is, for a close that fades instead of flying.
    let screenAlpha: CGFloat
}

/// A close as a function of its progress, from 0, the item open on its screen,
/// to 1, the item back in its slot. A finger sets a frame directly, and a
/// release runs to the first or the last one, so a swipe shows the close the
/// timed flight plays.
struct PocketFlightChoreography: Equatable {
    /// The page is gone this far into a close, before the Pocket's cards come back over it.
    static let pageFadeOutFraction: CGFloat = 0.2

    private static let commitTravelFraction: CGFloat = 0.3
    private static let flingReachFraction: CGFloat = 0.5
    private static let flingSpeed: CGFloat = 300
    /// UIKit's normal deceleration, per millisecond.
    private static let decelerationRate: CGFloat = 0.998
    private static let shortestRelease: TimeInterval = 0.15

    /// From the item's open place to its slot. Nil for a close that fades, with nowhere to fly or under Reduce Motion.
    let toSlot: CGPoint?

    func frame(progress: CGFloat) -> PocketFlightFrame {
        let remaining = 1 - progress
        guard let toSlot else {
            return PocketFlightFrame(
                itemOffset: .zero,
                pocketOffset: .zero,
                pageAlpha: 1,
                clearance: 0,
                screenAlpha: remaining
            )
        }

        return PocketFlightFrame(
            itemOffset: CGPoint(x: toSlot.x * progress, y: toSlot.y * progress),
            pocketOffset: CGSize(width: -toSlot.x * remaining, height: -toSlot.y * remaining),
            pageAlpha: 1 - min(progress / Self.pageFadeOutFraction, 1),
            clearance: remaining,
            screenAlpha: 1
        )
    }

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

    /// The motion's own time for the part of the way still to go. A flight is never so short that the release
    /// flashes.
    func releaseDuration(remaining: CGFloat) -> TimeInterval {
        guard toSlot != nil else { return PocketFlight.crossfadeDuration * Double(remaining) }

        return max(PocketFlightTiming.duration * Double(remaining), Self.shortestRelease)
    }
}
