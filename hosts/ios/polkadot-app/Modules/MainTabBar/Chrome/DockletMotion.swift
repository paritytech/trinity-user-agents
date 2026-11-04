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

extension DockletRest {
    /// The haptic a settle to this rest lands with: heavy on a user tuck, medium on a user restore, else none.
    func landingHaptic(wasTucked: Bool, isUserDriven: Bool) -> DockletHaptic? {
        let landsTucked = self == .tucked
        guard isUserDriven, landsTucked != wasTucked else {
            return nil
        }
        return landsTucked ? .heavy : .medium
    }
}

/// The Docklet's hide progress. A drag moves it, and a spring settles it. Time is passed in by the caller, so the
/// type reads no clock and imports no UIKit.
struct DockletMotion {
    /// Progress at or past which a settle lands on tucked.
    private static let tuckThreshold: CGFloat = 0.5
    /// How far ahead, in seconds, a release's velocity is projected when it picks the rest.
    private static let projectionSeconds: CGFloat = 0.1
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
    private var spring: DockletSpring?
    /// The held pose's extra turn and offset over the plain pose at release. It fades as the settle runs.
    private var releaseExtra: DockletPose = .zero
    private var unwindFactor: CGFloat = 0
    /// Whether the content inside the glass fades with the hide progress. A quiet hide sets it, and so does a Reduce
    /// Motion tuck once it lands. A drag and a non-quiet settle clear it.
    private var isQuietHide = false
    /// Where the content inside the glass is headed. Only a drag, settle or snap sets it, so a tick never changes it.
    private(set) var contentFadeTarget: DockletContentFadeTarget = .shown
    /// The Reduce Motion settle that is running, in place of a spring.
    private var fade: DockletReducedFade?

    var isSettling: Bool {
        spring != nil || fade != nil
    }

    /// Whether a Reduce Motion fade is running. The Edge glow follows the content while it runs.
    var isFading: Bool {
        fade != nil
    }

    /// Opacity of the content inside the glass. A Reduce Motion fade sets it; otherwise a quiet hide, or a tuck that
    /// has landed under Reduce Motion, fades it.
    var contentOpacity: CGFloat {
        if let fade {
            return fade.contentOpacity
        }
        return isQuietHide ? DockletContentFade.opacity(progress: progress) : 1
    }

    /// Jumps to `rest` with no motion, for changes the caller animates itself.
    mutating func snap(to rest: DockletRest) {
        drag = nil
        spring = nil
        fade = nil
        releaseExtra = .zero
        unwindFactor = 0
        progress = rest.progress
        destination = rest
        if rest != .tucked {
            isQuietHide = false
        }
        contentFadeTarget = contentOpacity == 0 ? .hidden : .shown
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
        drag.samples.append(.init(time: time, progress: progress, fingerX: location.x))
        drag.peak = max(drag.peak, abs(progress - drag.startProgress))
        self.drag = drag
        return selectionHaptic()
    }

    /// Releases the drag and settles toward the rest that the release position and velocity point at. Under Reduce
    /// Motion the release fades instead, holding the pose it was released with. A restore to shown lands at once.
    mutating func end(time: TimeInterval, geometry: DockletGeometry, reduceMotion: Bool = false) {
        guard let drag else {
            return
        }
        let heldPose = pose(geometry: geometry)
        self.drag = nil
        let velocity = drag.samples.velocity(travel: drag.travel)
        let rest = restAfterRelease(of: drag, velocity: velocity)
        let wasTucked = drag.isAiming || drag.startProgress >= Self.tuckThreshold
        let haptic = rest.landingHaptic(wasTucked: wasTucked, isUserDriven: true)
        if reduceMotion {
            startFade(to: rest, now: time, landingHaptic: haptic, isQuiet: false)
        } else {
            startSpring(to: rest, velocity: velocity, now: time, landingHaptic: haptic)
        }
        guard fade?.rest != .shown else {
            return
        }
        releaseExtra = heldPose.minus(geometry.plainPose(progress: progress))
        unwindFactor = 1
    }

    /// Settles from the current progress to `rest`, with no velocity. Under Reduce Motion it fades instead.
    mutating func settle(to rest: DockletRest, now: TimeInterval, isUserDriven: Bool, reduceMotion: Bool = false) {
        drag = nil
        let wasTucked = progress >= Self.tuckThreshold
        let haptic = rest.landingHaptic(wasTucked: wasTucked, isUserDriven: isUserDriven)
        if reduceMotion {
            startFade(to: rest, now: now, landingHaptic: haptic, isQuiet: false)
        } else {
            startSpring(to: rest, velocity: 0, now: now, landingHaptic: haptic)
        }
    }

    /// Settles to `rest` on the quiet spring, with no haptic. The first frame only starts the clock, so the bar holds
    /// still for that frame. Under Reduce Motion it fades instead.
    mutating func quietSettle(to rest: DockletRest, now: TimeInterval, reduceMotion: Bool = false) {
        drag = nil
        if reduceMotion {
            startFade(to: rest, now: now, landingHaptic: nil, isQuiet: true)
        } else {
            startSpring(to: rest, velocity: 0, now: now, landingHaptic: nil, isQuiet: true)
        }
    }

    /// Advances the settle to `now`. Returns the haptic when a user-started settle lands on a new rest.
    mutating func tick(now: TimeInterval) -> DockletHaptic? {
        if fade != nil {
            return tickFade(now: now)
        }
        return tickSpring(now: now)
    }

    /// The Docklet's pose now: the plain pose, or the held pose while a finger holds the bar, plus any unwinding.
    func pose(geometry: DockletGeometry) -> DockletPose {
        let base = drag?.grip.pose(geometry: geometry) ?? geometry.plainPose(progress: progress)
        return base.plus(releaseExtra, scaledBy: unwindFactor)
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
        var samples: DockletDragSamples
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
}

/// The opacity of the content inside the glass during a quiet hide. The glass itself never fades.
enum DockletContentFade {
    /// Scale of the fade's curve, which reaches zero just before progress 1.
    private static let scale: CGFloat = 1.12
    /// Exponent of the fade's curve.
    private static let exponent: CGFloat = 1.15

    /// Opacity at `progress`: 1 at shown, and zero from about 0.906 on.
    static func opacity(progress: CGFloat) -> CGFloat {
        max(0, 1 - scale * pow(max(0, progress), exponent))
    }

    /// Length of the quiet hide's content fade. The quiet spring, critically damped with a 0.42 s period, reaches
    /// the fade's zero at progress 0.906 after about 0.265 s.
    static let quietFadeDuration: TimeInterval = 0.265
    /// Control points of the timing curve that runs the quiet fade in SwiftUI over `quietFadeDuration`. The curve
    /// matches the spring's opacity within about 0.6 %.
    static let quietFadeFirstControl = (x: 0.25, y: 0.0)
    static let quietFadeSecondControl = (x: 0.4, y: 0.8)
    /// Frames the quiet fade waits after its trigger. The quiet spring's first frame only starts its clock, so the
    /// bar moves one frame after the trigger.
    static let quietFadeLeadFrames = 1
}

/// Where the content inside the glass is headed, and how it gets there. A transition sets it once, when it starts.
enum DockletContentFadeTarget: Equatable {
    /// Full opacity at once.
    case shown
    /// No opacity at once.
    case hidden
    /// To zero on the quiet hide's timing curve.
    case quietFadeOut
    /// To zero, linearly, over `DockletReducedFade.duration`.
    case reducedFadeOut
    /// To full opacity, linearly, over `DockletReducedFade.duration`.
    case reducedFadeIn

    /// The opacity the transition lands on.
    var opacity: CGFloat {
        switch self {
        case .shown,
             .reducedFadeIn:
            1
        case .hidden,
             .quietFadeOut,
             .reducedFadeOut:
            0
        }
    }
}

private extension DockletMotion {
    static func rubberBanded(_ raw: CGFloat) -> CGFloat {
        let slope = rubberBandMaximum * rubberBandRate
        if raw > 1 {
            return 1 + (raw - 1).rubberBanded(limit: rubberBandMaximum, slope: slope)
        }
        if raw < 0 {
            return -(-raw).rubberBanded(limit: rubberBandMaximum, slope: slope)
        }
        return raw
    }

    /// Stops any settle and takes the bar under the finger from its current progress.
    private mutating func startDrag(touch: CGPoint, time: TimeInterval, travel: CGFloat, grip: Grip) {
        spring = nil
        fade = nil
        releaseExtra = .zero
        unwindFactor = 0
        isQuietHide = false
        contentFadeTarget = .shown
        drag = Drag(
            startProgress: progress,
            travel: travel,
            grip: grip,
            samples: DockletDragSamples(first: .init(time: time, progress: progress, fingerX: touch.x)),
            peak: 0,
            isAboveSelection: progress >= Self.tuckThreshold
        )
    }

    mutating func selectionHaptic() -> DockletHaptic? {
        guard let drag else {
            return nil
        }
        let projected = progress + drag.samples.velocity(travel: drag.travel) * Self.projectionSeconds
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
        if drag.isAiming, drag.samples.leftwardSpeed > Self.leftwardFlickSpeed {
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
        landingHaptic: DockletHaptic?,
        isQuiet: Bool = false
    ) {
        destination = rest
        releaseExtra = .zero
        unwindFactor = 0
        isQuietHide = isQuiet
        contentFadeTarget = isQuiet ? .quietFadeOut : .shown
        spring = DockletSpring(
            start: now,
            rest: rest,
            initialOffset: progress - rest.progress,
            initialVelocity: velocity,
            dampingRatio: abs(velocity) > Self.underDampedSpeed ? DockletSpring.underDampedRatio : 1,
            landingHaptic: landingHaptic,
            angularFrequency: isQuiet ? DockletSpring.quietAngularFrequency : DockletSpring.angularFrequency,
            isAwaitingFirstFrame: isQuiet
        )
    }

    /// Moves the spring to `now`. The bar lands on its rest once the spring has settled or run out of time.
    mutating func tickSpring(now: TimeInterval) -> DockletHaptic? {
        guard var spring else {
            return nil
        }
        if spring.isAwaitingFirstFrame {
            spring.isAwaitingFirstFrame = false
            spring.start = now
            self.spring = spring
            return nil
        }
        let elapsed = now - spring.start
        let offset = spring.offset(after: elapsed)
        unwindFactor = spring.unwindShare(after: elapsed)
        guard spring.hasLanded(after: elapsed) else {
            progress = spring.rest.progress + offset
            return nil
        }
        progress = spring.rest.progress
        self.spring = nil
        releaseExtra = .zero
        unwindFactor = 0
        return spring.landingHaptic
    }

    /// Starts a Reduce Motion settle. A restore puts the bar at shown at once, so the glass never animates.
    mutating func startFade(to rest: DockletRest, now: TimeInterval, landingHaptic: DockletHaptic?, isQuiet: Bool) {
        spring = nil
        destination = rest
        fade = DockletReducedFade(start: now, rest: rest, landingHaptic: landingHaptic, isQuiet: isQuiet)
        contentFadeTarget = rest == .tucked ? .reducedFadeOut : .reducedFadeIn
        guard rest == .shown else {
            return
        }
        progress = rest.progress
        releaseExtra = .zero
        unwindFactor = 0
    }

    /// Holds the pose while the fade runs. At its landing the bar lands on its rest and the landing haptic is returned.
    mutating func tickFade(now: TimeInterval) -> DockletHaptic? {
        guard var fade else {
            return nil
        }
        guard !fade.hasLanded(at: now) else {
            progress = fade.rest.progress
            isQuietHide = fade.rest == .tucked
            releaseExtra = .zero
            unwindFactor = 0
            self.fade = nil
            return fade.landingHaptic
        }
        fade.advance(to: now)
        self.fade = fade
        return nil
    }
}
