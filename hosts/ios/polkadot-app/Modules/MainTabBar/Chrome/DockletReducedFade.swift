import CoreGraphics
import Foundation

/// A Reduce Motion settle. The bar holds its pose while its content fades, then lands on its rest in one step.
/// Time is passed in by the caller, so the type reads no clock and imports no UIKit.
struct DockletReducedFade {
    /// Length of the fade in seconds, the same as the centre control's Reduce Motion fade in PolkadotUI.
    static let duration: TimeInterval = 0.22

    let start: TimeInterval
    let rest: DockletRest
    let landingHaptic: DockletHaptic?
    /// Set on a quiet hide, which rests with its content faded out.
    let isQuiet: Bool
    /// Share of the fade that has run, from 0 at its start to 1 at its landing.
    var share: CGFloat = 0

    /// Content opacity at the current share. It fades out on a tuck and in on a restore.
    var contentOpacity: CGFloat {
        rest == .tucked ? 1 - share : share
    }

    /// Whether the fade has run its full length by `time`.
    func hasLanded(at time: TimeInterval) -> Bool {
        time - start >= Self.duration
    }

    /// Moves the share to `time`, clamped between the start and the landing.
    mutating func advance(to time: TimeInterval) {
        let fraction = min(1, max(0, time - start) / Self.duration)
        share = CGFloat(fraction)
    }
}
