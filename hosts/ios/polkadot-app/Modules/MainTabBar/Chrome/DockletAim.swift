import CoreGraphics
import Foundation

/// A finger that grabs a tucked Docklet's Grab zone. The bar turns to point at the finger. The progress it aims for
/// blends in from the grab's progress and pose, so the first frame after the grab matches the bar as it was caught.
struct DockletAim {
    /// Distance the finger may travel from the grab before the grab's progress and pose have blended out.
    private static let blendDistance: CGFloat = 80

    private let drawbridge: DockletDrawbridge
    private let touch: CGPoint
    private let startProgress: CGFloat
    /// The progress the finger aims at when it grabs the bar.
    private let startAim: CGFloat
    /// The grabbed pose minus the plain pose at the grab's progress.
    private let startResidual: DockletPose
    private var finger: CGPoint
    private(set) var progress: CGFloat

    init(touch: CGPoint, progress: CGFloat, screenPose: DockletPose, drawbridge: DockletDrawbridge) {
        self.drawbridge = drawbridge
        self.touch = touch
        startProgress = progress
        startAim = Self.aimedProgress(at: touch, progress: progress, drawbridge: drawbridge)
        startResidual = screenPose.minus(drawbridge.plainPose(progress: progress))
        finger = touch
        self.progress = progress
    }

    /// Follows the finger to a new progress.
    mutating func track(finger location: CGPoint) {
        finger = location
        let aimed = Self.aimedProgress(at: location, progress: progress, drawbridge: drawbridge)
        progress = (aimed + (startProgress - startAim) * blend).clampedToUnit
    }

    /// The plain pose at the progress, plus the grab's residual while the finger is near the grab point.
    func pose() -> DockletPose {
        drawbridge.plainPose(progress: progress).plus(startResidual, scaledBy: blend)
    }
}

private extension DockletAim {
    /// The share of the grab's progress and pose that still applies: all of it at the grab, none once the finger has
    /// travelled `blendDistance`.
    var blend: CGFloat {
        max(0, 1 - hypot(finger.x - touch.x, finger.y - touch.y) / Self.blendDistance)
    }

    /// The aim solved twice, because the pivot slides out with progress and the first pass estimates that slide.
    static func aimedProgress(at point: CGPoint, progress: CGFloat, drawbridge: DockletDrawbridge) -> CGFloat {
        let estimate = rawProgress(at: point, progress: progress, drawbridge: drawbridge)
        return rawProgress(at: point, progress: estimate, drawbridge: drawbridge)
    }

    /// The progress at which the bar, turned about the pivot and slid out to `progress`, points at `point`.
    static func rawProgress(at point: CGPoint, progress: CGFloat, drawbridge: DockletDrawbridge) -> CGFloat {
        let pivot = drawbridge.pivot
        let slidPivotX = pivot.x + drawbridge.plainPose(progress: progress).offset.dx
        let angle = atan2(max(0, pivot.y - point.y), max(1, slidPivotX - point.x))
        return angle / DockletDrawbridge.quarterTurn
    }
}
