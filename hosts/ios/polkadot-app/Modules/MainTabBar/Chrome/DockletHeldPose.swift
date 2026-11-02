import CoreGraphics
import Foundation

/// The Docklet's pose: a turn about its pivot, then an offset. `DockletGeometry.absolute(_:)` turns it into a screen
/// transform.
struct DockletPose: Equatable {
    static let zero = DockletPose(angle: 0, offset: .zero)

    var angle: CGFloat
    var offset: CGVector

    /// This pose plus `other` scaled by `factor`, component by component.
    func plus(_ other: DockletPose, scaledBy factor: CGFloat) -> DockletPose {
        DockletPose(
            angle: angle + factor * other.angle,
            offset: CGVector(dx: offset.dx + factor * other.offset.dx, dy: offset.dy + factor * other.offset.dy)
        )
    }

    func minus(_ other: DockletPose) -> DockletPose {
        plus(other, scaledBy: -1)
    }
}

/// A finger holding the Docklet. The bar turns about the point under the finger and follows the finger, leading it
/// towards the right edge. It resists past the wrong-way limit and above the tucked bar, and it cannot sink through
/// the floor.
struct DockletHold {
    /// How far outside the bar's frame a touch may land and still grab the bar.
    private static let grabSlop: CGFloat = 16
    /// Share of the progress-driven quarter turn that a held bar takes.
    private static let turnShare: CGFloat = 0.5
    /// Bisection steps that find the largest turn share keeping the bar above the floor.
    private static let fitSteps = 12
    /// Distance the bar leads the finger towards the right edge at full progress.
    private static let leadDistance: CGFloat = 56
    /// Distance the finger keeps inside the bar's leading end.
    private static let leadInset: CGFloat = 14
    /// Furthest the bar follows a finger that moves left of the x where its progress reaches shown.
    private static let wrongWayLimit: CGFloat = 40
    /// Furthest the bar follows a finger above the tucked bar's top.
    private static let ceilingLimit: CGFloat = 40
    /// Furthest the bar sinks below the floor under a finger that drags below it.
    private static let floorLimit: CGFloat = 36

    /// The rest-frame point under the finger at grab, or nil when the touch missed the bar.
    private let grabbed: CGPoint?
    /// The finger x at which progress reaches shown. Movement left of it is the wrong way.
    private let zeroX: CGFloat
    private let startAngle: CGFloat
    private let startProgress: CGFloat
    private var finger: CGPoint
    private var progress: CGFloat

    init(touch: CGPoint, progress: CGFloat, travel: CGFloat, screenPose: DockletPose, geometry: DockletGeometry) {
        let local = touch.applying(geometry.absolute(screenPose).inverted())
        let reach = geometry.dockFrame.insetBy(dx: -Self.grabSlop, dy: -Self.grabSlop)
        grabbed = reach.contains(local) ? local : nil
        zeroX = touch.x - progress * travel
        startAngle = screenPose.angle
        startProgress = progress
        finger = touch
        self.progress = progress
    }

    mutating func track(finger location: CGPoint, progress current: CGFloat) {
        finger = location
        progress = current
    }

    /// The held pose, or the plain pose when the grab missed the bar.
    func pose(geometry: DockletGeometry) -> DockletPose {
        guard let grabbed else {
            return geometry.plainPose(progress: progress)
        }
        let floor = geometry.bounds.maxY + Self.eased(max(0, finger.y - geometry.bounds.maxY), limit: Self.floorLimit)
        let share = fittingTurnShare(grabbed: grabbed, allowedBottom: floor, geometry: geometry)
        var pose = heldPose(grabbed: grabbed, turnShare: share, geometry: geometry)
        let sink = geometry.lowestY(pose) - floor
        if sink > 0 {
            pose.offset.dy -= sink
        }
        return pose
    }
}

private extension DockletHold {
    /// Follows `over` one-for-one at first, then saturates at `limit`.
    static func eased(_ over: CGFloat, limit: CGFloat) -> CGFloat {
        limit * (1 - 1 / (1 + over / limit))
    }

    static func smoothstep(_ value: CGFloat) -> CGFloat {
        let clamped = min(max(value, 0), 1)
        return clamped * clamped * (3 - 2 * clamped)
    }

    func fittingTurnShare(grabbed: CGPoint, allowedBottom: CGFloat, geometry: DockletGeometry) -> CGFloat {
        let fullShare = Self.turnShare
        let fullPose = heldPose(grabbed: grabbed, turnShare: fullShare, geometry: geometry)
        guard geometry.lowestY(fullPose) > allowedBottom else {
            return fullShare
        }
        var fitting: CGFloat = 0
        var overflowing = fullShare
        for _ in 0 ..< Self.fitSteps {
            let middle = (fitting + overflowing) / 2
            if geometry.lowestY(heldPose(grabbed: grabbed, turnShare: middle, geometry: geometry)) <= allowedBottom {
                fitting = middle
            } else {
                overflowing = middle
            }
        }
        return fitting
    }

    /// Turns by `turnShare` of the progress made since the grab, then offsets so the grabbed point sits at the target.
    func heldPose(grabbed: CGPoint, turnShare: CGFloat, geometry: DockletGeometry) -> DockletPose {
        let angle = startAngle + turnShare * DockletGeometry.quarterTurn * (progress - startProgress)
        let target = targetPoint(grabbed: grabbed, geometry: geometry)
        let pivot = geometry.pivot
        let turned = CGPoint(x: grabbed.x - pivot.x, y: grabbed.y - pivot.y)
            .applying(CGAffineTransform(rotationAngle: angle))
        return DockletPose(
            angle: angle,
            offset: CGVector(dx: target.x - pivot.x - turned.x, dy: target.y - pivot.y - turned.y)
        )
    }

    func targetPoint(grabbed: CGPoint, geometry: DockletGeometry) -> CGPoint {
        let glowTop = geometry.glowTop
        let fingerX = finger.x >= zeroX
            ? finger.x
            : zeroX - Self.eased(zeroX - finger.x, limit: Self.wrongWayLimit)
        let fingerY = finger.y < glowTop
            ? glowTop - Self.eased(glowTop - finger.y, limit: Self.ceilingLimit)
            : finger.y
        return CGPoint(x: fingerX + lead(grabbed: grabbed, geometry: geometry), y: fingerY)
    }

    /// How far the bar leads the finger towards the right edge. The lead builds from the progress made since the
    /// grab, and the finger stays inside the bar's leading end.
    func lead(grabbed: CGPoint, geometry: DockletGeometry) -> CGFloat {
        let cue = Self.leadDistance * (Self.smoothstep(progress) - Self.smoothstep(startProgress))
        let cap = grabbed.x - geometry.dockFrame.minX - Self.leadInset
        return max(0, min(cue, cap))
    }
}

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
