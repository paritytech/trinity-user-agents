import CoreGraphics

/// The way the Docklet travels between Shown and Tucked, and where it rests when Tucked. The app uses one Path.
/// Each case holds that Path's own drag state.
enum DockletPath {
    case drawbridge(DockletDrawbridgeState)

    /// Grips the bar at `touch` and returns the drag travel that a full hide takes from there.
    mutating func beginHold(
        touch: CGPoint,
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry
    ) -> CGFloat {
        switch self {
        case var .drawbridge(state):
            let travel = state.beginHold(touch: touch, progress: progress, unwindShare: unwindShare, geometry: geometry)
            self = .drawbridge(state)
            return travel
        }
    }

    /// Grips the bar at a touch in the Grab zone and returns the drag travel that a full hide takes from there.
    mutating func beginAim(
        touch: CGPoint,
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry
    ) -> CGFloat {
        switch self {
        case var .drawbridge(state):
            let travel = state.beginAim(touch: touch, progress: progress, unwindShare: unwindShare, geometry: geometry)
            self = .drawbridge(state)
            return travel
        }
    }

    /// Follows the finger and returns the new progress.
    mutating func track(finger: CGPoint, translation: CGVector, startProgress: CGFloat, travel: CGFloat) -> CGFloat {
        switch self {
        case var .drawbridge(state):
            let progress = state.track(
                finger: finger,
                translation: translation,
                startProgress: startProgress,
                travel: travel
            )
            self = .drawbridge(state)
            return progress
        }
    }

    /// Records the residual the bar is released with at `progress`, and drops the grip.
    mutating func release(progress: CGFloat, geometry: DockletGeometry) {
        switch self {
        case var .drawbridge(state):
            state.release(progress: progress, geometry: geometry)
            self = .drawbridge(state)
        }
    }

    /// Drops the grip, for a change that takes the bar off the finger.
    mutating func dropGrip() {
        switch self {
        case var .drawbridge(state):
            state.dropGrip()
            self = .drawbridge(state)
        }
    }

    /// Clears the residual the bar was released with.
    mutating func resetRelease() {
        switch self {
        case var .drawbridge(state):
            state.resetRelease()
            self = .drawbridge(state)
        }
    }

    /// The transform for a view anchored at `anchor`, posed at `progress` with the grip or the release residual.
    func transform(
        progress: CGFloat,
        unwindShare: CGFloat,
        geometry: DockletGeometry,
        anchor: CGPoint
    ) -> CGAffineTransform {
        switch self {
        case let .drawbridge(state):
            state.transform(
                progress: progress,
                unwindShare: unwindShare,
                geometry: geometry,
                anchor: anchor
            )
        }
    }

    /// Whether a pan with `translation` may fold the bar. A bar caught mid-settle takes a wider cone.
    func acceptsPan(translation: CGVector, isSettling: Bool) -> Bool {
        switch self {
        case .drawbridge:
            DockletDirectionLock.isSideways(translation: translation, isSettling: isSettling)
        }
    }

    /// The screen edge the Path's Docklet rests past.
    var edge: DockletEdge {
        switch self {
        case .drawbridge:
            DockletDrawbridge.edge
        }
    }

    /// The Edge glow strip that the Path leaves on its edge when the Docklet is tucked.
    func edgeGlowRect(geometry: DockletGeometry) -> CGRect {
        switch self {
        case .drawbridge:
            DockletDrawbridge(geometry: geometry).edgeGlowRect
        }
    }

    /// The Grab zone that the Path leaves on its edge when the Docklet is tucked.
    func grabZoneRect(geometry: DockletGeometry) -> CGRect {
        switch self {
        case .drawbridge:
            DockletDrawbridge(geometry: geometry).grabZoneRect
        }
    }
}

/// The screen edge a Tucked Docklet rests past.
enum DockletEdge {
    /// The trailing edge of the screen.
    case right

    /// The unit direction from this edge into the screen.
    var inward: CGVector {
        switch self {
        case .right:
            CGVector(dx: -1, dy: 0)
        }
    }
}
