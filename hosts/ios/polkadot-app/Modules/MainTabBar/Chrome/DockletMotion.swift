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

/// A haptic the Docklet asks for when a user-started settle changes its rest.
enum DockletHaptic: Equatable {
    case heavy
    case medium
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
    private static let quarterTurn: CGFloat = .pi / 2
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

    /// The transform for a view whose anchor point is `anchor`, posed at `progress`. Shown is the identity. Tucked
    /// is a quarter turn about the pivot with the leading end going up, then the slide out.
    func transform(progress: CGFloat, anchor: CGPoint) -> CGAffineTransform {
        let hinge = pivot
        let slide = slideOut * pow(max(0, progress), Self.slideExponent)
        let absolute = CGAffineTransform(translationX: -hinge.x, y: -hinge.y)
            .concatenating(CGAffineTransform(rotationAngle: Self.quarterTurn * progress))
            .concatenating(CGAffineTransform(translationX: hinge.x + slide, y: hinge.y))
        let mappedAnchor = anchor.applying(absolute)
        return CGAffineTransform(
            a: absolute.a,
            b: absolute.b,
            c: absolute.c,
            d: absolute.d,
            tx: mappedAnchor.x - anchor.x,
            ty: mappedAnchor.y - anchor.y
        )
    }

    private var glowTop: CGFloat {
        pivot.y - (pivot.x - dockFrame.minX)
    }
}

/// The Docklet's hide progress. A drag moves it, and a critically damped spring settles it. Time is passed in by the
/// caller, so the type reads no clock and imports no UIKit.
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

    private(set) var progress: CGFloat = 0
    private(set) var destination: DockletRest = .shown
    private var drag: Drag?
    private var spring: Spring?

    var isSettling: Bool {
        spring != nil
    }

    /// Jumps to `rest` with no motion, for changes the caller animates itself.
    mutating func snap(to rest: DockletRest) {
        drag = nil
        spring = nil
        progress = rest.progress
        destination = rest
    }

    mutating func begin(touchX: CGFloat, geometry: DockletGeometry) {
        spring = nil
        drag = Drag(startProgress: progress, travel: geometry.hideTravel(touchX: touchX))
    }

    mutating func move(translationX: CGFloat) {
        guard let drag else {
            return
        }
        progress = min(max(drag.startProgress + translationX / drag.travel, 0), 1)
    }

    /// Releases the drag and settles toward the rest the release position and velocity point at.
    mutating func end(velocityX: CGFloat, now: TimeInterval) {
        guard let drag else {
            return
        }
        self.drag = nil
        let velocity = velocityX / drag.travel
        let rest: DockletRest = progress + velocity * Self.projectionSeconds >= Self.tuckThreshold ? .tucked : .shown
        startSpring(
            to: rest,
            velocity: velocity,
            now: now,
            wasTucked: drag.startProgress >= Self.tuckThreshold,
            isUserDriven: true
        )
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
        let hasLanded = elapsed > Self.hardStopSeconds
            || (elapsed > Self.minimumSettleSeconds && abs(offset) < Self.landingTolerance)
        guard hasLanded else {
            progress = min(max(spring.rest.progress + offset, 0), 1)
            return nil
        }
        progress = spring.rest.progress
        self.spring = nil
        return spring.landingHaptic
    }

    private mutating func startSpring(
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
        spring = Spring(
            start: now,
            rest: rest,
            initialOffset: progress - rest.progress,
            initialVelocity: velocity,
            landingHaptic: landingHaptic
        )
    }

    private struct Drag {
        let startProgress: CGFloat
        /// The distance a full hide takes from where the drag began.
        let travel: CGFloat
    }

    private struct Spring {
        /// Gives a critically damped settle that is close to rest after about 0.32 s.
        static let angularFrequency: CGFloat = 2 * .pi / 0.32

        let start: TimeInterval
        let rest: DockletRest
        let initialOffset: CGFloat
        let initialVelocity: CGFloat
        let landingHaptic: DockletHaptic?

        /// Distance from the rest after `elapsed` seconds: x(t) = (x0 + (v0 + ω·x0)·t)·e^(−ω·t).
        func offset(after elapsed: TimeInterval) -> CGFloat {
            let time = CGFloat(elapsed)
            let omega = Self.angularFrequency
            let linear = initialOffset + (initialVelocity + omega * initialOffset) * time
            return linear * exp(-omega * time)
        }
    }
}
