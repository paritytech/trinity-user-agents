import CoreGraphics

/// The Drawbridge's drag state. A finger holds the bar by a grip while it is dragged, and the release residual keeps
/// the pose the bar was released with until its settle has run.
struct DockletDrawbridgeState {
    /// The grip that holds the bar: a finger holding it, or a finger aiming at it from the Grab zone.
    private enum Grip {
        case hold(DockletHold)
        case aim(DockletAim)

        /// The held pose, or the aimed pose.
        func pose(drawbridge: DockletDrawbridge) -> DockletPose {
            switch self {
            case let .hold(hold):
                hold.pose(drawbridge: drawbridge)
            case let .aim(aim):
                aim.pose()
            }
        }
    }

    private var grip: Grip?
    /// The held pose's extra turn and offset over the plain pose at release. It fades as the settle runs.
    private var releaseResidual: DockletPose = .zero

    /// Grips the bar at `touch` and returns the drag travel that a full hide takes from there.
    mutating func beginHold(
        touch: CGPoint,
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry
    ) -> CGFloat {
        let drawbridge = DockletDrawbridge(geometry: geometry)
        let travel = drawbridge.hideTravel(touchX: touch.x)
        let hold = DockletHold(
            touch: touch,
            progress: progress,
            travel: travel,
            screenPose: currentPose(progress: progress, unwindShare: unwindShare, geometry: geometry),
            drawbridge: drawbridge
        )
        grip = .hold(hold)
        return travel
    }

    /// Grips the bar at a touch in the Grab zone, so the bar turns to the finger. Returns the drag travel.
    mutating func beginAim(
        touch: CGPoint,
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry
    ) -> CGFloat {
        let drawbridge = DockletDrawbridge(geometry: geometry)
        let aim = DockletAim(
            touch: touch,
            progress: progress,
            screenPose: currentPose(progress: progress, unwindShare: unwindShare, geometry: geometry),
            drawbridge: drawbridge
        )
        grip = .aim(aim)
        return drawbridge.hideTravel(touchX: touch.x)
    }

    /// Follows the finger and returns the new progress. A hold takes its progress from the drag's translation.
    mutating func track(finger: CGPoint, translation: CGVector, startProgress: CGFloat, travel: CGFloat) -> CGFloat {
        switch grip {
        case var .hold(hold)?:
            let progress = DockletMotion.rubberBanded(startProgress + translation.dx / travel)
            hold.track(finger: finger, progress: progress)
            grip = .hold(hold)
            return progress
        case var .aim(aim)?:
            aim.track(finger: finger)
            grip = .aim(aim)
            return aim.progress
        case nil:
            return startProgress
        }
    }

    /// Records the residual the bar is released with at `progress`, and drops the grip.
    mutating func release(progress: CGFloat, geometry: DockletGeometry) {
        let drawbridge = DockletDrawbridge(geometry: geometry)
        let heldPose = grip?.pose(drawbridge: drawbridge) ?? drawbridge.plainPose(progress: progress)
        releaseResidual = heldPose.minus(drawbridge.plainPose(progress: progress))
        grip = nil
    }

    /// Drops the grip, for a change that takes the bar off the finger.
    mutating func dropGrip() {
        grip = nil
    }

    /// Clears the residual the bar was released with.
    mutating func resetRelease() {
        releaseResidual = .zero
    }

    /// The transform for a view anchored at `anchor`.
    func transform(
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry,
        anchor: CGPoint
    ) -> CGAffineTransform {
        let pose = currentPose(progress: progress, unwindShare: unwindShare, geometry: geometry)
        return DockletDrawbridge(geometry: geometry).transform(pose, anchor: anchor)
    }

    /// The grip's pose, or the plain pose without a grip, plus the residual scaled by the unwind share.
    private func currentPose(progress: CGFloat, unwindShare: CGFloat, geometry: DockletGeometry) -> DockletPose {
        let drawbridge = DockletDrawbridge(geometry: geometry)
        let base = grip?.pose(drawbridge: drawbridge) ?? drawbridge.plainPose(progress: progress)
        return base.plus(releaseResidual, scaledBy: unwindShare)
    }
}
