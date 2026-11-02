import CoreGraphics
import Foundation

/// A closed-form spring that settles the Docklet's hide progress on a rest. It is critically damped, or under-damped
/// after a fast release so it overshoots its rest once.
struct DockletSpring {
    /// Gives a critically damped settle that is close to rest after about 0.32 s.
    static let angularFrequency: CGFloat = 2 * .pi / 0.32
    /// Gives the quiet hide's critically damped settle a period of 0.42 s.
    static let quietAngularFrequency: CGFloat = 2 * .pi / 0.42
    /// Damping ratio of a fast settle, which overshoots its rest once.
    static let underDampedRatio: CGFloat = 0.78
    /// Longest a settle may run, in seconds, even if the spring has not yet come to rest.
    private static let hardStopSeconds: TimeInterval = 1.4
    /// Shortest a settle may run, in seconds, before it is allowed to land.
    private static let minimumSettleSeconds: TimeInterval = 0.08
    /// Distance from the rest under which a settle counts as landed.
    private static let landingTolerance: CGFloat = 0.0015

    var start: TimeInterval
    let rest: DockletRest
    let initialOffset: CGFloat
    let initialVelocity: CGFloat
    let dampingRatio: CGFloat
    let landingHaptic: DockletHaptic?
    let angularFrequency: CGFloat
    /// Set on a quiet settle, whose first frame only starts the clock.
    var isAwaitingFirstFrame: Bool

    /// Whether a settle `elapsed` seconds in, with this offset and unwind share left, has landed.
    static func hasLanded(after elapsed: TimeInterval, offset: CGFloat, unwindShare: CGFloat) -> Bool {
        elapsed > hardStopSeconds
            || (elapsed > minimumSettleSeconds
                && abs(offset) < landingTolerance
                && unwindShare < landingTolerance)
    }

    /// Distance from the rest after `elapsed` seconds.
    func offset(after elapsed: TimeInterval) -> CGFloat {
        let time = CGFloat(elapsed)
        let omega = angularFrequency
        guard dampingRatio < 1 else {
            let linear = initialOffset + (initialVelocity + omega * initialOffset) * time
            return linear * exp(-omega * time)
        }
        let dampedOmega = omega * (1 - dampingRatio * dampingRatio).squareRoot()
        let sineCoefficient = (initialVelocity + dampingRatio * omega * initialOffset) / dampedOmega
        let oscillation = initialOffset * cos(dampedOmega * time) + sineCoefficient * sin(dampedOmega * time)
        return exp(-dampingRatio * omega * time) * oscillation
    }

    /// Share of the held pose's extra that remains `elapsed` seconds into the settle.
    func unwindShare(after elapsed: TimeInterval) -> CGFloat {
        let time = CGFloat(elapsed)
        return (1 + angularFrequency * time) * exp(-angularFrequency * time)
    }
}
