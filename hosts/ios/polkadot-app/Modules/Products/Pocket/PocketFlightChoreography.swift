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
}

/// A close as a function of its progress, from 0, the item open on its screen,
/// to 1, the item back in its slot. A finger sets a frame directly, and a
/// release runs to the first or the last one, so a swipe shows the close the
/// timed flight plays.
struct PocketFlightChoreography: Equatable {
    /// The page is gone this far into a close, before the Pocket's cards come back over it.
    static let pageFadeOutFraction: CGFloat = 0.2

    private static let shortestRelease: TimeInterval = 0.15

    /// From the item's open place to its slot.
    let toSlot: CGPoint

    func frame(progress: CGFloat) -> PocketFlightFrame {
        let remaining = 1 - progress
        return PocketFlightFrame(
            itemOffset: CGPoint(x: toSlot.x * progress, y: toSlot.y * progress),
            pocketOffset: CGSize(width: -toSlot.x * remaining, height: -toSlot.y * remaining),
            pageAlpha: 1 - min(progress / Self.pageFadeOutFraction, 1),
            clearance: remaining
        )
    }

    /// The motion's own time for the part of the way still to go. A flight is never so short that the release
    /// flashes.
    func releaseDuration(remaining: CGFloat) -> TimeInterval {
        max(PocketFlightTiming.duration * Double(remaining), Self.shortestRelease)
    }
}
