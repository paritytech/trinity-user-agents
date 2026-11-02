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

/// Where the Docklet's parts sit, derived from its resting frame and the chrome's bounds. The bar pivots about a
/// point just past its trailing edge, and a tucked bar leaves its Edge glow and Grab zone on the right edge.
struct DockletGeometry: Equatable {
    /// Distance the pivot sits past the bar's trailing end.
    private static let pivotGap: CGFloat = 14
    /// Distance the bar slides past the screen edge beyond its full tuck.
    private static let extraSlide: CGFloat = 60
    /// Width of the Edge glow strip on the trailing edge.
    private static let edgeGlowWidth: CGFloat = 24
    /// Width of the Grab zone on the trailing edge, which is wider than the glow so it is easier to catch.
    private static let grabZoneWidth: CGFloat = 47
    /// Height of the Edge glow and the Grab zone.
    private static let edgeZoneLength: CGFloat = 200
    /// Shortest drag that takes the bar from shown to tucked.
    private static let minimumHideTravel: CGFloat = 160
    /// Share of the bar's width that a full hide drag takes, before the minimum applies.
    private static let hideTravelShare: CGFloat = 0.75
    /// Multiple of the room between a drag's start and the trailing edge that a hide drag may take.
    private static let edgeTravelMultiple: CGFloat = 1.2
    /// Shortest drag a drag that starts near the trailing edge may take to hide the bar.
    private static let minimumEdgeTravel: CGFloat = 60
    /// Rotation of the bar about the pivot when fully tucked, with the leading end going up.
    static let quarterTurn: CGFloat = .pi / 2
    /// Exponent of the cubic ease that drives the slide out.
    private static let slideExponent: CGFloat = 3

    /// The bar's resting frame.
    let dockFrame: CGRect
    /// The chrome's full-bleed bounds.
    let bounds: CGRect

    var pivot: CGPoint {
        CGPoint(x: dockFrame.maxX + Self.pivotGap, y: dockFrame.midY)
    }

    /// How far the bar travels past the trailing edge when it is fully tucked.
    var slideOut: CGFloat {
        bounds.width - dockFrame.maxX + Self.extraSlide
    }

    var edgeGlowRect: CGRect {
        CGRect(x: bounds.width - Self.edgeGlowWidth, y: glowTop, width: Self.edgeGlowWidth, height: Self.edgeZoneLength)
    }

    var grabZoneRect: CGRect {
        CGRect(x: bounds.width - Self.grabZoneWidth, y: glowTop, width: Self.grabZoneWidth, height: Self.edgeZoneLength)
    }

    /// The drag distance that takes the bar from shown to tucked, for a drag that starts at `touchX`. A drag that
    /// starts near the trailing edge needs less travel.
    func hideTravel(touchX: CGFloat) -> CGFloat {
        min(
            max(Self.minimumHideTravel, Self.hideTravelShare * dockFrame.width),
            max(Self.minimumEdgeTravel, Self.edgeTravelMultiple * (bounds.width - touchX))
        )
    }

    /// The pose at `progress` without a finger: a quarter turn about the pivot, then the slide out.
    func plainPose(progress: CGFloat) -> DockletPose {
        DockletPose(
            angle: Self.quarterTurn * progress,
            offset: CGVector(dx: slideOut * pow(max(0, progress), Self.slideExponent), dy: 0)
        )
    }

    /// The transform that maps the bar's resting frame to the screen for `pose`.
    func absolute(_ pose: DockletPose) -> CGAffineTransform {
        let hinge = pivot
        return CGAffineTransform(translationX: -hinge.x, y: -hinge.y)
            .concatenating(CGAffineTransform(rotationAngle: pose.angle))
            .concatenating(CGAffineTransform(translationX: hinge.x + pose.offset.dx, y: hinge.y + pose.offset.dy))
    }

    /// The lowest y of the bar's resting corners once `pose` is applied.
    func lowestY(_ pose: DockletPose) -> CGFloat {
        let mapping = absolute(pose)
        return [dockFrame.minX, dockFrame.maxX].reduce(-CGFloat.infinity) { lowest, cornerX in
            [dockFrame.minY, dockFrame.maxY].reduce(lowest) {
                max($0, CGPoint(x: cornerX, y: $1).applying(mapping).y)
            }
        }
    }

    /// The transform for a view whose anchor point is `anchor`, posed as `pose`.
    func transform(_ pose: DockletPose, anchor: CGPoint) -> CGAffineTransform {
        let mapping = absolute(pose)
        let mappedAnchor = anchor.applying(mapping)
        return CGAffineTransform(
            a: mapping.a,
            b: mapping.b,
            c: mapping.c,
            d: mapping.d,
            tx: mappedAnchor.x - anchor.x,
            ty: mappedAnchor.y - anchor.y
        )
    }

    /// The transform for a view whose anchor point is `anchor`, posed at `progress`. Shown is the identity. Tucked
    /// is a quarter turn about the pivot with the leading end going up, then the slide out.
    func transform(progress: CGFloat, anchor: CGPoint) -> CGAffineTransform {
        transform(plainPose(progress: progress), anchor: anchor)
    }

    var glowTop: CGFloat {
        pivot.y - (pivot.x - dockFrame.minX)
    }
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
        let screenPose = pose(geometry: geometry)
        spring = nil
        releaseExtra = .zero
        unwindFactor = 0
        let travel = geometry.hideTravel(touchX: touch.x)
        drag = Drag(
            startProgress: progress,
            travel: travel,
            hold: DockletHold(
                touch: touch,
                progress: progress,
                travel: travel,
                screenPose: screenPose,
                geometry: geometry
            ),
            samples: [Sample(time: time, progress: progress)],
            peak: 0,
            isAboveSelection: progress >= Self.tuckThreshold
        )
    }

    /// Follows the finger. Returns a selection tick when the projected rest crosses a mark.
    mutating func move(location: CGPoint, translationX: CGFloat, time: TimeInterval) -> DockletHaptic? {
        guard var drag else {
            return nil
        }
        progress = Self.rubberBanded(drag.startProgress + translationX / drag.travel)
        drag.samples.append(Sample(time: time, progress: progress))
        drag.peak = max(drag.peak, abs(progress - drag.startProgress))
        drag.hold.track(finger: location, progress: progress)
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
            wasTucked: drag.startProgress >= Self.tuckThreshold,
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
        let base = drag?.hold.pose(geometry: geometry) ?? geometry.plainPose(progress: progress)
        return base.plus(releaseExtra, scaledBy: unwindFactor)
    }

    private struct Sample {
        let time: TimeInterval
        let progress: CGFloat
    }

    private struct Drag {
        let startProgress: CGFloat
        /// The distance a full hide takes from where the drag began.
        let travel: CGFloat
        var hold: DockletHold
        var samples: [Sample]
        /// The furthest the drag has strayed from its start progress.
        var peak: CGFloat
        /// Whether the projected rest is past the selection mark.
        var isAboveSelection: Bool
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

    /// Velocity in progress per second, from the slope over the last window of the moving samples. Trailing samples
    /// that barely moved are dropped first, so a pause before release keeps the velocity of the flick.
    private static func velocity(of samples: [Sample], travel: CGFloat) -> CGFloat {
        let slowestSpeed = trailingSpeedPoints / travel
        var moving = samples
        while moving.count >= 2, isSlow(moving[moving.count - 2], moving[moving.count - 1], slowestSpeed) {
            moving.removeLast()
        }
        guard moving.count >= 2, let last = moving.last else {
            return 0
        }
        let window = moving.filter { last.time - $0.time <= velocityWindowSeconds }
        guard let first = window.first, window.count >= 2, last.time > first.time else {
            return 0
        }
        return (last.progress - first.progress) / CGFloat(last.time - first.time)
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
