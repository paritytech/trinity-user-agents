import CoreGraphics
import Foundation

/// Decides whether a pan folds the Docklet sideways. A bar at rest takes a wider cone than one caught mid-settle.
enum DockletDirectionLock {
    /// Translation length at which the angle decides the pan. Shorter drags use the horizontal-versus-vertical test.
    private static let decisionDistance: CGFloat = 10
    /// Angles, in degrees above the horizon, that a resting bar accepts as sideways. Negative angles point down.
    private static let restingCone: ClosedRange<CGFloat> = -45 ... 30
    /// Half-width, in degrees, of the cone that a bar caught mid-settle accepts.
    private static let caughtHalfCone: CGFloat = 60

    static func isSideways(translation: CGVector, isSettling: Bool) -> Bool {
        guard hypot(translation.dx, translation.dy) >= decisionDistance else {
            return abs(translation.dx) > abs(translation.dy)
        }
        let angle = atan2(-translation.dy, abs(translation.dx)) * 180 / .pi
        return isSettling ? abs(angle) <= caughtHalfCone : restingCone.contains(angle)
    }
}
