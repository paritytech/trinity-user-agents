import CoreGraphics
import Foundation

/// The two places the Docklet rests. Its hide progress is 0 when shown and 1 when tucked.
enum DockletRest: Equatable {
    case shown
    case tucked

    var progress: CGFloat {
        switch self {
        case .shown:
            0
        case .tucked:
            1
        }
    }
}

/// A haptic the Docklet asks for: a user-started settle changing its rest, or a selection tick while it is held.
enum DockletHaptic: Equatable {
    case heavy
    case medium
    case selection
}

/// The Docklet's hide progress. A drag moves it, and a spring settles it. Time is passed in by the caller, so the
/// type reads no clock and imports no UIKit.
struct DockletMotion {
    /// Progress at or past which a settle lands on tucked.
    private static let tuckThreshold: CGFloat = 0.5
    /// How far ahead, in seconds, a release's velocity is projected when it picks the rest.
    private static let projectionSeconds: CGFloat = 0.1
    /// Longest a settle may run, in seconds, even if the spring has not yet come to rest.
    private static let hardStopSeconds: TimeInterval = 1.4
    /// Shortest a settle may run, in seconds, before it is allowed to land.
    private static let minimumSettleSeconds: TimeInterval = 0.08
    /// Distance from the rest under which a settle counts as landed.
    private static let landingTolerance: CGFloat = 0.0015
    /// Span, in seconds, of the samples whose slope gives a release's velocity.
    private static let velocityWindowSeconds: TimeInterval = 0.1
    /// Slowest, in points per second, that a trailing sample may move and still count toward the velocity.
    private static let trailingSpeedPoints: CGFloat = 150
    /// Furthest a drag must stray from its start before turning back can reverse its rest.
    private static let turnBackPeak: CGFloat = 0.05
    /// Share of that peak a drag may fall back by before its rest reverses to the start rest.
    private static let turnBackShare: CGFloat = 0.3
    /// Leftward finger speed, in points a second, that sends a grabbed bar back to shown at any drag length.
    private static let leftwardFlickSpeed: CGFloat = 350
    /// Release speed, in progress per second, above which a settle is under-damped.
    private static let underDampedSpeed: CGFloat = 4
    /// Projected progress that sounds a selection tick as it rises past, and as it falls below, the marks.
    private static let selectionRising: CGFloat = 0.54
    private static let selectionFalling: CGFloat = 0.46
    /// Largest band that overdrag past 0 or 1 can add, and how quickly the band stiffens.
    private static let rubberBandMaximum: CGFloat = 0.25
    private static let rubberBandRate: CGFloat = 2.2

    private(set) var progress: CGFloat = 0
    private(set) var destination: DockletRest = .shown
    private var drag: Drag?
    private var spring: Spring?
    /// The held pose's extra turn and offset over the plain pose at release. It fades as the settle runs.
    private var releaseExtra: DockletPose = .zero
    private var unwindFactor: CGFloat = 0

    var isSettling: Bool {
        spring != nil
    }

    /// Jumps to `rest` with no motion, for changes the caller animates itself.
    mutating func snap(to rest: DockletRest) {
        drag = nil
        spring = nil
        releaseExtra = .zero
        unwindFactor = 0
        progress = rest.progress
        destination = rest
    }

    /// Starts a drag at `touch`. A touch on a moving bar catches it where it is.
    mutating func begin(touch: CGPoint, time: TimeInterval, geometry: DockletGeometry) {
        let travel = geometry.hideTravel(touchX: touch.x)
        let hold = DockletHold(
            touch: touch,
            progress: progress,
            travel: travel,
            screenPose: pose(geometry: geometry),
            geometry: geometry
        )
        startDrag(touch: touch, time: time, travel: travel, grip: .hold(hold))
    }

    /// Starts a drag at a touch in the Grab zone, where the bar turns to point at the finger. A bar still settling
    /// toward tucked is caught where it is.
    mutating func beginAim(touch: CGPoint, time: TimeInterval, geometry: DockletGeometry) {
        let aim = DockletAim(touch: touch, progress: progress, screenPose: pose(geometry: geometry), geometry: geometry)
        startDrag(touch: touch, time: time, travel: geometry.hideTravel(touchX: touch.x), grip: .aim(aim))
    }

    /// Follows the finger. Returns a selection tick when the projected rest crosses a mark.
    mutating func move(location: CGPoint, translationX: CGFloat, time: TimeInterval) -> DockletHaptic? {
        guard var drag else {
            return nil
        }
        switch drag.grip {
        case var .hold(hold):
            progress = Self.rubberBanded(drag.startProgress + translationX / drag.travel)
            hold.track(finger: location, progress: progress)
            drag.grip = .hold(hold)
        case var .aim(aim):
            aim.track(finger: location)
            progress = aim.progress
            drag.grip = .aim(aim)
        }
        drag.samples.append(Sample(time: time, progress: progress, fingerX: location.x))
        drag.peak = max(drag.peak, abs(progress - drag.startProgress))
        self.drag = drag
        return selectionHaptic()
    }

    /// Releases the drag and settles toward the rest that the release position and velocity point at.
    mutating func end(time: TimeInterval, geometry: DockletGeometry) {
        guard let drag else {
            return
        }
        let heldPose = pose(geometry: geometry)
        self.drag = nil
        let velocity = Self.velocity(of: drag.samples, travel: drag.travel)
        startSpring(
            to: restAfterRelease(of: drag, velocity: velocity),
            velocity: velocity,
            now: time,
            wasTucked: drag.isAiming || drag.startProgress >= Self.tuckThreshold,
            isUserDriven: true
        )
        releaseExtra = heldPose.minus(geometry.plainPose(progress: progress))
        unwindFactor = 1
    }

    /// Settles from the current progress to `rest`, with no velocity.
    mutating func settle(to rest: DockletRest, now: TimeInterval, isUserDriven: Bool) {
        drag = nil
        startSpring(
            to: rest,
            velocity: 0,
            now: now,
            wasTucked: progress >= Self.tuckThreshold,
            isUserDriven: isUserDriven
        )
    }

    /// Advances the settle to `now`. Returns the haptic when a user-started settle lands on a new rest.
    mutating func tick(now: TimeInterval) -> DockletHaptic? {
        guard let spring else {
            return nil
        }
        let elapsed = now - spring.start
        let offset = spring.offset(after: elapsed)
        unwindFactor = Self.unwindShare(after: elapsed)
        let hasLanded = elapsed > Self.hardStopSeconds
            || (elapsed > Self.minimumSettleSeconds
                && abs(offset) < Self.landingTolerance
                && unwindFactor < Self.landingTolerance)
        guard hasLanded else {
            progress = spring.rest.progress + offset
            return nil
        }
        progress = spring.rest.progress
        self.spring = nil
        releaseExtra = .zero
        unwindFactor = 0
        return spring.landingHaptic
    }

    /// The Docklet's pose now: the plain pose, or the held pose while a finger holds the bar, plus any unwinding.
    func pose(geometry: DockletGeometry) -> DockletPose {
        let base = drag?.grip.pose(geometry: geometry) ?? geometry.plainPose(progress: progress)
        return base.plus(releaseExtra, scaledBy: unwindFactor)
    }

    private struct Sample {
        let time: TimeInterval
        let progress: CGFloat
        let fingerX: CGFloat
    }

    private enum Grip {
        case hold(DockletHold)
        case aim(DockletAim)

        func pose(geometry: DockletGeometry) -> DockletPose {
            switch self {
            case let .hold(hold):
                hold.pose(geometry: geometry)
            case let .aim(aim):
                aim.pose()
            }
        }
    }

    private struct Drag {
        let startProgress: CGFloat
        /// The distance a full hide takes from where the drag began.
        let travel: CGFloat
        var grip: Grip
        var samples: [Sample]
        /// The furthest the drag has strayed from its start progress.
        var peak: CGFloat
        /// Whether the projected rest is past the selection mark.
        var isAboveSelection: Bool

        var isAiming: Bool {
            guard case .aim = grip else {
                return false
            }
            return true
        }
    }

    private struct Spring {
        /// Gives a critically damped settle that is close to rest after about 0.32 s.
        static let angularFrequency: CGFloat = 2 * .pi / 0.32
        /// Damping ratio of a fast settle, which overshoots its rest once.
        static let underDampedRatio: CGFloat = 0.78

        let start: TimeInterval
        let rest: DockletRest
        let initialOffset: CGFloat
        let initialVelocity: CGFloat
        let dampingRatio: CGFloat
        let landingHaptic: DockletHaptic?

        /// Distance from the rest after `elapsed` seconds.
        func offset(after elapsed: TimeInterval) -> CGFloat {
            let time = CGFloat(elapsed)
            let omega = Self.angularFrequency
            guard dampingRatio < 1 else {
                let linear = initialOffset + (initialVelocity + omega * initialOffset) * time
                return linear * exp(-omega * time)
            }
            let dampedOmega = omega * (1 - dampingRatio * dampingRatio).squareRoot()
            let sineCoefficient = (initialVelocity + dampingRatio * omega * initialOffset) / dampedOmega
            let oscillation = initialOffset * cos(dampedOmega * time) + sineCoefficient * sin(dampedOmega * time)
            return exp(-dampingRatio * omega * time) * oscillation
        }
    }
}

private extension DockletMotion {
    static func rubberBanded(_ raw: CGFloat) -> CGFloat {
        if raw > 1 {
            return 1 + rubber(raw - 1)
        }
        if raw < 0 {
            return -rubber(-raw)
        }
        return raw
    }

    static func rubber(_ overdrag: CGFloat) -> CGFloat {
        rubberBandMaximum * (1 - 1 / (1 + rubberBandRate * overdrag))
    }

    /// Share of the held pose's extra that remains `elapsed` seconds into a settle.
    static func unwindShare(after elapsed: TimeInterval) -> CGFloat {
        let time = CGFloat(elapsed)
        let omega = Spring.angularFrequency
        return (1 + omega * time) * exp(-omega * time)
    }

    /// Stops any settle and takes the bar under the finger from its current progress.
    private mutating func startDrag(touch: CGPoint, time: TimeInterval, travel: CGFloat, grip: Grip) {
        spring = nil
        releaseExtra = .zero
        unwindFactor = 0
        drag = Drag(
            startProgress: progress,
            travel: travel,
            grip: grip,
            samples: [Sample(time: time, progress: progress, fingerX: touch.x)],
            peak: 0,
            isAboveSelection: progress >= Self.tuckThreshold
        )
    }

    /// Velocity in progress per second, from the slope over the last window of the moving samples. Trailing samples
    /// that barely moved are dropped first, so a pause before release keeps the velocity of the flick.
    private static func velocity(of samples: [Sample], travel: CGFloat) -> CGFloat {
        let slowestSpeed = trailingSpeedPoints / travel
        var moving = samples
        while moving.count >= 2, isSlow(moving[moving.count - 2], moving[moving.count - 1], slowestSpeed) {
            moving.removeLast()
        }
        guard let span = span(of: moving) else {
            return 0
        }
        return (span.last.progress - span.first.progress) / CGFloat(span.last.time - span.first.time)
    }

    /// The first and last samples of the window that ends at the last sample, or nil when it holds fewer than two.
    private static func span(of samples: [Sample]) -> (first: Sample, last: Sample)? {
        guard let last = samples.last else {
            return nil
        }
        let window = samples.filter { last.time - $0.time <= velocityWindowSeconds }
        guard let first = window.first, window.count >= 2, last.time > first.time else {
            return nil
        }
        return (first, last)
    }

    /// Leftward speed of the finger, in points a second, over the same window the velocity uses.
    private static func leftwardSpeed(of samples: [Sample]) -> CGFloat {
        guard let span = span(of: samples) else {
            return 0
        }
        return (span.first.fingerX - span.last.fingerX) / CGFloat(span.last.time - span.first.time)
    }

    private static func isSlow(_ earlier: Sample, _ later: Sample, _ speed: CGFloat) -> Bool {
        abs(later.progress - earlier.progress) < speed * CGFloat(later.time - earlier.time)
    }

    mutating func selectionHaptic() -> DockletHaptic? {
        guard let drag else {
            return nil
        }
        let projected = progress + Self.velocity(of: drag.samples, travel: drag.travel) * Self.projectionSeconds
        if !drag.isAboveSelection, projected > Self.selectionRising {
            self.drag?.isAboveSelection = true
            return .selection
        }
        if drag.isAboveSelection, projected < Self.selectionFalling {
            self.drag?.isAboveSelection = false
            return .selection
        }
        return nil
    }

    private func restAfterRelease(of drag: Drag, velocity: CGFloat) -> DockletRest {
        if drag.isAiming, Self.leftwardSpeed(of: drag.samples) > Self.leftwardFlickSpeed {
            return .shown
        }
        let target = isTurnedBack(drag) ? drag.startProgress : progress + velocity * Self.projectionSeconds
        return target >= Self.tuckThreshold ? .tucked : .shown
    }

    private func isTurnedBack(_ drag: Drag) -> Bool {
        let distance = abs(progress - drag.startProgress)
        return drag.peak > Self.turnBackPeak && drag.peak - distance > Self.turnBackShare * drag.peak
    }

    mutating func startSpring(
        to rest: DockletRest,
        velocity: CGFloat,
        now: TimeInterval,
        wasTucked: Bool,
        isUserDriven: Bool
    ) {
        let landsTucked = rest == .tucked
        var landingHaptic: DockletHaptic?
        if isUserDriven, landsTucked != wasTucked {
            landingHaptic = landsTucked ? .heavy : .medium
        }
        destination = rest
        releaseExtra = .zero
        unwindFactor = 0
        spring = Spring(
            start: now,
            rest: rest,
            initialOffset: progress - rest.progress,
            initialVelocity: velocity,
            dampingRatio: abs(velocity) > Self.underDampedSpeed ? Spring.underDampedRatio : 1,
            landingHaptic: landingHaptic
        )
    }
}
