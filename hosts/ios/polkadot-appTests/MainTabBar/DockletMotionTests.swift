import CoreGraphics
import Foundation
import Testing
@testable import polkadot_app

@Suite("DockletMotion")
struct DockletMotionTests {
    private let geometry = DockletGeometry(
        dockFrame: CGRect(x: 10, y: 785, width: 382, height: 62),
        bounds: CGRect(x: 0, y: 0, width: 402, height: 874)
    )

    private var drawbridge: DockletDrawbridge {
        DockletDrawbridge(geometry: geometry)
    }

    /// Drags from `touchX` by `translationX`, then releases with no velocity at time zero.
    private func release(from touchX: CGFloat, translationX: CGFloat, reduceMotion: Bool = false) -> DockletMotion {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: touchX, y: 816), time: 0, geometry: geometry)
        _ = motion.move(
            location: CGPoint(x: touchX + translationX, y: 816),
            translation: CGVector(dx: translationX, dy: 0),
            time: 0
        )
        motion.end(time: 0, geometry: geometry, reduceMotion: reduceMotion)
        return motion
    }

    /// Drags 140 points from the first slot in 0.1 s, the last half of it in 0.05 s.
    private func flick() -> DockletMotion {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = motion.move(location: CGPoint(x: 110, y: 816), translation: CGVector(dx: 70, dy: 0), time: 0.05)
        _ = motion.move(location: CGPoint(x: 180, y: 816), translation: CGVector(dx: 140, dy: 0), time: 0.1)
        return motion
    }

    /// The highest progress a settle that started at 0.1 s reaches over the next 1.4 s.
    private func peakProgress(of settling: DockletMotion) -> CGFloat {
        var motion = settling
        var peak: CGFloat = 0
        for step in 1 ... 140 {
            _ = motion.tick(now: 0.1 + Double(step) * 0.01)
            peak = max(peak, motion.progress)
        }
        return peak
    }

    private func isNear(_ point: CGPoint, _ expected: CGPoint) -> Bool {
        abs(point.x - expected.x) < 1e-6 && abs(point.y - expected.y) < 1e-6
    }

    private func isNear(_ transform: CGAffineTransform, _ expected: CGAffineTransform) -> Bool {
        abs(transform.a - expected.a) < 1e-6
            && abs(transform.b - expected.b) < 1e-6
            && abs(transform.c - expected.c) < 1e-6
            && abs(transform.d - expected.d) < 1e-6
            && abs(transform.tx - expected.tx) < 1e-6
            && abs(transform.ty - expected.ty) < 1e-6
    }

    /// A vector of length 100 pointing `degrees` below the horizon, with a positive y being down.
    private func vector(degreesDown degrees: CGFloat) -> CGVector {
        let radians = degrees * CGFloat.pi / 180
        return CGVector(dx: 100 * cos(radians), dy: 100 * sin(radians))
    }

    @Test("A release past half tucks the bar")
    func releasePastHalfTucks() {
        let motion = release(from: 200, translationX: 130)
        #expect(motion.destination == .tucked)
    }

    @Test("A release before half settles shown")
    func releaseBeforeHalfSettlesShown() {
        let motion = release(from: 200, translationX: 100)
        #expect(motion.destination == .shown)
    }

    @Test("The same swipe tucks from the first slot and from the last")
    func sameSwipeTucksFromEitherEnd() {
        #expect(release(from: 40, translationX: 200).destination == .tucked)
        #expect(release(from: 340, translationX: 200).destination == .tucked)
    }

    @Test("A settle lands exactly on its rest, not before 0.08 s and by 1.4 s")
    func settleLandsOnRest() {
        var motion = release(from: 40, translationX: 200)
        _ = motion.tick(now: 0.05)
        #expect(motion.isSettling)
        _ = motion.tick(now: 1.4)
        #expect(!motion.isSettling)
        #expect(motion.progress == 1)
    }

    @Test("A user tuck lands with a heavy haptic")
    func userTuckEmitsHeavy() {
        var motion = release(from: 40, translationX: 200)
        #expect(motion.tick(now: 1.4) == .heavy)
    }

    @Test("A restore from tucked lands with a medium haptic")
    func restoreEmitsMedium() {
        var motion = DockletMotion()
        motion.snap(to: .tucked)
        motion.settle(to: .shown, now: 0, isUserDriven: true)
        #expect(motion.tick(now: 1.4) == .medium)
    }

    @Test("A policy settle emits no haptic")
    func policySettleEmitsNothing() {
        var motion = DockletMotion()
        motion.settle(to: .tucked, now: 0, isUserDriven: false)
        #expect(motion.tick(now: 1.4) == nil)
    }

    @Test("Shown is the identity pose")
    func shownPoseIsIdentity() {
        let point = CGPoint(x: 306, y: 816)
        #expect(point.applying(drawbridge.transform(progress: 0, anchor: .zero)) == point)
    }

    @Test("Tucked turns the bar a quarter about the pivot with the leading end up, then slides it out")
    func tuckedPoseIsQuarterTurnAndSlide() {
        let transform = drawbridge.transform(progress: 1, anchor: .zero)
        let pivot = drawbridge.pivot
        let leadingEnd = CGPoint(x: pivot.x - 100, y: pivot.y)
        #expect(isNear(pivot.applying(transform), CGPoint(x: pivot.x + drawbridge.slideOut, y: pivot.y)))
        #expect(isNear(
            leadingEnd.applying(transform),
            CGPoint(x: pivot.x + drawbridge.slideOut, y: pivot.y - 100)
        ))
    }

    @Test("Halfway slides an eighth of the slide out")
    func halfwaySlidesAnEighth() {
        let pivot = drawbridge.pivot
        let transform = drawbridge.transform(progress: 0.5, anchor: .zero)
        #expect(isNear(pivot.applying(transform), CGPoint(x: pivot.x + drawbridge.slideOut / 8, y: pivot.y)))
    }

    @Test("The Edge glow and Grab zone sit on the right edge at the tucked bar's top")
    func glowAndGrabZoneFollowTuckedBar() {
        #expect(drawbridge.edgeGlowRect == CGRect(x: 378, y: 420, width: 24, height: 200))
        #expect(drawbridge.grabZoneRect == CGRect(x: 355, y: 420, width: 47, height: 200))
    }

    @Test("A drag past either limit is rubber banded, so it saturates")
    func rubberBandSaturates() {
        var overTucked = DockletMotion()
        overTucked.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = overTucked.move(location: CGPoint(x: 10_040, y: 816), translation: CGVector(dx: 10_000, dy: 0), time: 0.01)
        #expect(overTucked.progress < 1.25)

        var overShown = DockletMotion()
        overShown.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = overShown.move(location: CGPoint(x: -9_960, y: 816), translation: CGVector(dx: -10_000, dy: 0), time: 0.01)
        #expect(overShown.progress > -0.25)
    }

    @Test("A swipe left from shown settles back to shown")
    func swipeLeftSettlesShown() {
        let motion = release(from: 200, translationX: -100)
        #expect(motion.destination == .shown)
    }

    @Test("A short fast flick tucks, and a trailing pause keeps the flick's velocity")
    func flickTucksThroughPause() {
        var released = flick()
        released.end(time: 0.1, geometry: geometry)
        #expect(released.destination == .tucked)

        var pausedFlick = flick()
        _ = pausedFlick.move(location: CGPoint(x: 180, y: 816), translation: CGVector(dx: 140, dy: 0), time: 0.2)
        _ = pausedFlick.move(location: CGPoint(x: 180, y: 816), translation: CGVector(dx: 140, dy: 0), time: 0.3)
        pausedFlick.end(time: 0.3, geometry: geometry)
        #expect(pausedFlick.destination == .tucked)
    }

    @Test("Out to 0.8 and back to 0.3 returns to shown")
    func turnBackReturnsToStart() {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = motion.move(location: CGPoint(x: 269, y: 816), translation: CGVector(dx: 229, dy: 0), time: 0.1)
        _ = motion.move(location: CGPoint(x: 126, y: 816), translation: CGVector(dx: 86, dy: 0), time: 0.2)
        motion.end(time: 0.2, geometry: geometry)
        #expect(motion.destination == .shown)
    }

    @Test("Out to 0.8, back to 0.55 and a pause settles shown, though 0.55 alone would tuck")
    func turnBackBelowHalfSettlesShown() {
        let travel = drawbridge.hideTravel(touchX: 40)
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = motion.move(
            location: CGPoint(x: 40 + 0.8 * travel, y: 816),
            translation: CGVector(dx: 0.8 * travel, dy: 0),
            time: 0.1
        )
        _ = motion.move(
            location: CGPoint(x: 40 + 0.55 * travel, y: 816),
            translation: CGVector(dx: 0.55 * travel, dy: 0),
            time: 0.3
        )
        _ = motion.move(
            location: CGPoint(x: 40 + 0.55 * travel, y: 816),
            translation: CGVector(dx: 0.55 * travel, dy: 0),
            time: 0.5
        )
        motion.end(time: 0.5, geometry: geometry)
        #expect(motion.destination == .shown)
    }

    @Test("A fast settle overshoots its rest, and a slow one does not")
    func fastSettleOvershootsSlowDoesNot() {
        var fast = flick()
        fast.end(time: 0.1, geometry: geometry)
        #expect(peakProgress(of: fast) > 1)

        var slow = DockletMotion()
        slow.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = slow.move(location: CGPoint(x: 126, y: 816), translation: CGVector(dx: 86, dy: 0), time: 0.1)
        slow.end(time: 0.1, geometry: geometry)
        #expect(peakProgress(of: slow) <= 1)
    }

    @Test("A held bar keeps the grabbed point under the finger at progress zero")
    func heldBarKeepsGrabbedPointUnderFinger() {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 200, y: 816), time: 0, geometry: geometry)
        _ = motion.move(location: CGPoint(x: 230, y: 800), translation: CGVector(dx: 0, dy: 0), time: 0.01)
        let grabbed = CGPoint(x: 200, y: 816).applying(motion.transform(geometry: geometry, anchor: .zero))
        #expect(abs(grabbed.x - 230) < 0.5)
        #expect(abs(grabbed.y - 800) < 0.5)
    }

    @Test("A bar caught mid-settle follows a finger dragged back to where its progress reaches shown")
    func caughtBarFollowsFingerDraggedBack() {
        var motion = release(from: 40, translationX: 0.6 * drawbridge.hideTravel(touchX: 40))
        _ = motion.tick(now: 0.01)
        #expect(abs(motion.progress - 0.6) < 0.05)
        let touch = CGPoint(x: 306, y: 816).applying(motion.transform(geometry: geometry, anchor: .zero))
        motion.begin(touch: touch, time: 0.01, geometry: geometry)
        let travel = drawbridge.hideTravel(touchX: touch.x)
        let finger = CGPoint(x: touch.x - 0.6 * travel, y: touch.y)
        _ = motion.move(location: finger, translation: CGVector(dx: -0.6 * travel, dy: 0), time: 0.02)
        let grabbed = CGPoint(x: 306, y: 816).applying(motion.transform(geometry: geometry, anchor: .zero))
        #expect(abs(grabbed.x - finger.x) < 0.5)
        #expect(abs(grabbed.y - finger.y) < 0.5)
    }

    @Test("A held bar never sinks below the screen while the finger stays above it")
    func heldBarStaysAboveFloor() {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 200, y: 800), time: 0, geometry: geometry)
        _ = motion.move(location: CGPoint(x: 350, y: 830), translation: CGVector(dx: 150, dy: 0), time: 0.01)
        let mapping = motion.transform(geometry: geometry, anchor: .zero)
        #expect(geometry.lowestY(mapping: mapping) <= geometry.bounds.maxY + 1e-6)
    }

    @Test("A bar caught mid-settle keeps the pose it was on screen with")
    func catchMidSettleKeepsPose() {
        var motion = release(from: 40, translationX: 200)
        _ = motion.tick(now: 0.05)
        let before = motion.transform(geometry: geometry, anchor: .zero)
        let touch = CGPoint(x: 306, y: 816).applying(before)
        motion.begin(touch: touch, time: 0.05, geometry: geometry)
        #expect(isNear(motion.transform(geometry: geometry, anchor: .zero), before))
    }

    @Test("A bar at rest begins a drag 40 degrees down")
    func restingBarTakesFortyDegreesDown() {
        #expect(DockletDirectionLock.isSideways(translation: vector(degreesDown: 40), isSettling: false))
    }

    @Test("A bar at rest refuses a drag 50 degrees down")
    func restingBarRefusesFiftyDegreesDown() {
        #expect(!DockletDirectionLock.isSideways(translation: vector(degreesDown: 50), isSettling: false))
    }

    @Test("A bar caught mid-settle takes a drag 50 degrees down")
    func caughtBarTakesFiftyDegreesDown() {
        #expect(DockletDirectionLock.isSideways(translation: vector(degreesDown: 50), isSettling: true))
    }

    @Test("A held bar ticks once rising past 0.54 and once falling below 0.46")
    func selectionTicksOnCrossings() {
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = motion.move(location: CGPoint(x: 183, y: 816), translation: CGVector(dx: 143.25, dy: 0), time: 1.0)
        let rising = motion.move(location: CGPoint(x: 212, y: 816), translation: CGVector(dx: 171.9, dy: 0), time: 1.05)
        let staying = motion.move(location: CGPoint(x: 215, y: 816), translation: CGVector(dx: 175, dy: 0), time: 1.1)
        let falling = motion.move(location: CGPoint(x: 126, y: 816), translation: CGVector(dx: 86, dy: 0), time: 1.15)
        #expect(rising == .selection)
        #expect(staying == nil)
        #expect(falling == .selection)
    }

    /// A tucked bar grabbed at `touch`, with no movement yet.
    private func grabbedTucked(at touch: CGPoint) -> DockletMotion {
        var motion = DockletMotion()
        motion.snap(to: .tucked)
        motion.beginAim(touch: touch, time: 0, geometry: geometry)
        return motion
    }

    @Test("A grab dragged low and far in from the edge aims the bar at shown")
    func aimLowAndFarInIsShown() {
        var motion = grabbedTucked(at: CGPoint(x: 370, y: 600))
        _ = motion.move(location: CGPoint(x: 60, y: 800), translation: CGVector(dx: -310, dy: 0), time: 0.01)
        #expect(motion.progress < 0.1)
    }

    @Test("A grab dragged high at the edge aims the bar at tucked")
    func aimHighAtEdgeIsTucked() {
        var motion = grabbedTucked(at: CGPoint(x: 370, y: 600))
        _ = motion.move(location: CGPoint(x: 400, y: 430), translation: CGVector(dx: 30, dy: 0), time: 0.01)
        #expect(motion.progress > 0.9)
    }

    @Test("The first move after a grab keeps the progress the bar was grabbed with")
    func firstMoveKeepsGrabbedProgress() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 500))
        _ = motion.move(location: CGPoint(x: 390, y: 500), translation: CGVector(dx: 0, dy: 0), time: 0.01)
        #expect(abs(motion.progress - 1) < 1e-9)
    }

    @Test("A grab dragged partly in and then down releases to shown")
    func grabDraggedDownReleasesShown() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 500))
        _ = motion.move(location: CGPoint(x: 260, y: 830), translation: CGVector(dx: -130, dy: 0), time: 0.05)
        motion.end(time: 0.05, geometry: geometry)
        #expect(motion.destination == .shown)
    }

    @Test("A grab flicked left faster than 350 points a second releases to shown, though it would tuck")
    func leftwardFlickReleasesShown() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 430))
        _ = motion.move(location: CGPoint(x: 350, y: 430), translation: CGVector(dx: -40, dy: 0), time: 0.05)
        _ = motion.move(location: CGPoint(x: 300, y: 430), translation: CGVector(dx: -90, dy: 0), time: 0.1)
        motion.end(time: 0.1, geometry: geometry)
        #expect(motion.destination == .shown)
    }

    @Test("A grab pulled 20 points and released slowly tucks again")
    func smallPullReleasesTucked() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 500))
        _ = motion.move(location: CGPoint(x: 370, y: 500), translation: CGVector(dx: -20, dy: 0), time: 0.5)
        motion.end(time: 0.5, geometry: geometry)
        #expect(motion.destination == .tucked)
    }

    @Test("A grab caught while settling to tucked keeps the pose it was on screen with")
    func grabCaughtFlyingToTuckedKeepsPose() {
        var motion = DockletMotion()
        motion.settle(to: .tucked, now: 0, isUserDriven: false)
        _ = motion.tick(now: 0.125)
        #expect(abs(motion.progress - 0.7) < 0.01)
        let before = motion.transform(geometry: geometry, anchor: .zero)
        motion.beginAim(touch: CGPoint(x: 380, y: 500), time: 0.125, geometry: geometry)
        #expect(isNear(motion.transform(geometry: geometry, anchor: .zero), before))
    }

    @Test("A grab released to shown lands with a medium haptic")
    func grabRestoreEmitsMedium() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 500))
        _ = motion.move(location: CGPoint(x: 200, y: 830), translation: CGVector(dx: -190, dy: 0), time: 0.05)
        motion.end(time: 0.05, geometry: geometry)
        #expect(motion.tick(now: 2) == .medium)
    }

    @Test("A grab released back to tucked lands silently, since the bar never left")
    func grabReturnToTuckedEmitsNothing() {
        var motion = grabbedTucked(at: CGPoint(x: 390, y: 500))
        _ = motion.move(location: CGPoint(x: 390, y: 500), translation: CGVector(dx: 0, dy: 0), time: 0.5)
        motion.end(time: 0.5, geometry: geometry)
        #expect(motion.destination == .tucked)
        #expect(motion.tick(now: 2) == nil)
    }

    @Test("A quiet hide holds still on its first frame, then lands tucked by about 0.6 s with no haptic")
    func quietHideLandsSilently() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        #expect(motion.tick(now: 1.0 / 60) == nil)
        #expect(motion.progress == 0)
        #expect(motion.tick(now: 0.5) == nil)
        #expect(motion.isSettling)
        #expect(motion.tick(now: 0.7) == nil)
        #expect(!motion.isSettling)
        #expect(motion.progress == 1)
    }

    @Test("A quiet hide fades the content, and a swipe tuck keeps it")
    func onlyQuietHideFadesContent() {
        var quiet = DockletMotion()
        quiet.quietSettle(to: .tucked, now: 0)
        _ = quiet.tick(now: 1.0 / 60)
        _ = quiet.tick(now: 0.7)
        #expect(quiet.contentOpacity == 0)

        var swipe = release(from: 40, translationX: 200)
        _ = swipe.tick(now: 1.4)
        #expect(swipe.progress == 1)
        #expect(swipe.contentOpacity == 1)
    }

    @Test("Settling back to shown restores the content at once")
    func shownRestoresContent() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        _ = motion.tick(now: 1.0 / 60)
        _ = motion.tick(now: 0.7)
        motion.settle(to: .shown, now: 0.7, isUserDriven: false)
        #expect(motion.contentOpacity == 1)
    }

    @Test("Content fades from full at shown to none just before tucked, and never below none")
    func contentFadeCurve() {
        #expect(DockletContentFade.opacity(progress: 0) == 1)
        #expect(abs(DockletContentFade.opacity(progress: 0.5) - 0.495) < 1e-3)
        #expect(DockletContentFade.opacity(progress: 0.906) < 1e-3)
        #expect(DockletContentFade.opacity(progress: 0.91) == 0)
        #expect(DockletContentFade.opacity(progress: 1) == 0)
    }

    @Test("A Reduce Motion tuck holds its pose, fades the content out over 0.22 s, then lands tucked")
    func reducedTuckFadesThenLands() {
        var motion = release(from: 40, translationX: 200, reduceMotion: true)
        let heldTransform = motion.transform(geometry: geometry, anchor: .zero)
        let heldProgress = motion.progress
        #expect(motion.tick(now: 0.11) == nil)
        #expect(abs(motion.contentOpacity - 0.5) < 1e-6)
        #expect(motion.progress == heldProgress)
        #expect(isNear(motion.transform(geometry: geometry, anchor: .zero), heldTransform))
        #expect(motion.tick(now: 0.22) == .heavy)
        #expect(!motion.isSettling)
        #expect(motion.progress == 1)
        #expect(motion.contentOpacity == 0)
    }

    @Test("A Reduce Motion restore places the bar at shown at once and fades the content in over 0.22 s")
    func reducedRestoreFadesContentIn() {
        var motion = DockletMotion()
        motion.snap(to: .tucked)
        motion.settle(to: .shown, now: 0, isUserDriven: true, reduceMotion: true)
        #expect(motion.progress == 0)
        #expect(motion.contentOpacity == 0)
        #expect(motion.tick(now: 0.11) == nil)
        #expect(abs(motion.contentOpacity - 0.5) < 1e-6)
        #expect(motion.progress == 0)
        #expect(motion.tick(now: 0.22) == .medium)
        #expect(motion.contentOpacity == 1)
        #expect(!motion.isSettling)
    }

    @Test("A Reduce Motion drag moves the bar with the finger, and only the release starts the fade")
    func reducedDragFollowsFingerUntilRelease() {
        let travel = drawbridge.hideTravel(touchX: 40)
        var motion = DockletMotion()
        motion.begin(touch: CGPoint(x: 40, y: 816), time: 0, geometry: geometry)
        _ = motion.move(
            location: CGPoint(x: 40 + 0.5 * travel, y: 816),
            translation: CGVector(dx: 0.5 * travel, dy: 0),
            time: 0.1
        )
        #expect(abs(motion.progress - 0.5) < 1e-9)
        #expect(!motion.isSettling)
        motion.end(time: 0.1, geometry: geometry, reduceMotion: true)
        #expect(motion.isFading)
        #expect(abs(motion.progress - 0.5) < 1e-9)
    }

    @Test("A Reduce Motion quiet hide fades the content out over 0.22 s and lands tucked with no haptic")
    func reducedQuietHideFadesThenLands() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0, reduceMotion: true)
        #expect(motion.tick(now: 0.11) == nil)
        #expect(abs(motion.contentOpacity - 0.5) < 1e-6)
        #expect(motion.progress == 0)
        #expect(motion.tick(now: 0.22) == nil)
        #expect(!motion.isSettling)
        #expect(motion.progress == 1)
        #expect(motion.contentOpacity == 0)
    }

    /// The transitions whose content fade target must hold through every later tick.
    enum FadeScenario: CaseIterable {
        case quietHide
        case reducedQuietHide
        case swipeTuck
        case reducedSwipeTuck
        case restore
    }

    private func scenarioMotion(_ scenario: FadeScenario) -> DockletMotion {
        switch scenario {
        case .quietHide:
            var motion = DockletMotion()
            motion.quietSettle(to: .tucked, now: 0)
            return motion
        case .reducedQuietHide:
            var motion = DockletMotion()
            motion.quietSettle(to: .tucked, now: 0, reduceMotion: true)
            return motion
        case .swipeTuck:
            return release(from: 40, translationX: 200)
        case .reducedSwipeTuck:
            return release(from: 40, translationX: 200, reduceMotion: true)
        case .restore:
            var motion = DockletMotion()
            motion.snap(to: .tucked)
            motion.settle(to: .shown, now: 0, isUserDriven: true)
            return motion
        }
    }

    @Test("A quiet hide fades the content out from its start and keeps that target until it lands at zero")
    func quietHideTargetsFadeOut() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        #expect(motion.contentFadeTarget == .quietFadeOut)
        _ = motion.tick(now: 1.0 / 60)
        _ = motion.tick(now: 0.5)
        #expect(motion.contentFadeTarget == .quietFadeOut)
        _ = motion.tick(now: 0.7)
        #expect(!motion.isSettling)
        #expect(motion.contentOpacity == motion.contentFadeTarget.opacity)
    }

    @Test("A grab during a quiet fade targets shown, so the content does not keep fading under the finger")
    func grabDuringQuietFadeTargetsShown() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        _ = motion.tick(now: 1.0 / 60)
        _ = motion.tick(now: 0.15)
        motion.beginAim(touch: CGPoint(x: 390, y: 500), time: 0.15, geometry: geometry)
        #expect(motion.contentFadeTarget == .shown)
    }

    @Test("Restoring a quiet-hidden bar targets shown, or a linear fade in under Reduce Motion")
    func restoreTargetsShownOrFadeIn() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        _ = motion.tick(now: 1.0 / 60)
        _ = motion.tick(now: 0.7)
        var restored = motion
        restored.settle(to: .shown, now: 0.7, isUserDriven: true)
        #expect(restored.contentFadeTarget == .shown)

        motion.settle(to: .shown, now: 0.7, isUserDriven: true, reduceMotion: true)
        #expect(motion.contentFadeTarget == .reducedFadeIn)
        _ = motion.tick(now: 1.0)
        #expect(motion.contentOpacity == 1)
    }

    @Test("Under Reduce Motion a quiet hide and a swipe tuck both fade the content out and rest with it at zero")
    func reducedTucksTargetFadeOut() {
        var quiet = DockletMotion()
        quiet.quietSettle(to: .tucked, now: 0, reduceMotion: true)
        #expect(quiet.contentFadeTarget == .reducedFadeOut)
        _ = quiet.tick(now: 0.22)
        #expect(quiet.contentOpacity == 0)

        var swipe = release(from: 40, translationX: 200, reduceMotion: true)
        #expect(swipe.contentFadeTarget == .reducedFadeOut)
        _ = swipe.tick(now: 0.22)
        #expect(swipe.contentOpacity == 0)
    }

    @Test("A swipe tuck keeps the content shown while it lands, since only a tuck under Reduce Motion fades it")
    func swipeTuckTargetsShown() {
        var motion = release(from: 40, translationX: 200)
        #expect(motion.contentFadeTarget == .shown)
        _ = motion.tick(now: 0.7)
        #expect(motion.contentFadeTarget == .shown)
        _ = motion.tick(now: 1.4)
        #expect(motion.contentFadeTarget == .shown)
        #expect(motion.contentOpacity == 1)
    }

    @Test("Snapping a quiet hide to tucked hides the content at once, and to shown restores it at once")
    func snapTargetsFollowRest() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        _ = motion.tick(now: 1.0 / 60)
        _ = motion.tick(now: 0.15)

        var tucked = motion
        tucked.snap(to: .tucked)
        #expect(tucked.contentFadeTarget == .hidden)
        #expect(tucked.contentOpacity == 0)

        motion.snap(to: .shown)
        #expect(motion.contentFadeTarget == .shown)
        #expect(motion.contentOpacity == 1)
    }

    @Test("A tick never changes the content fade target", arguments: FadeScenario.allCases)
    func tickNeverChangesFadeTarget(scenario: FadeScenario) {
        var motion = scenarioMotion(scenario)
        let target = motion.contentFadeTarget
        for step in 1 ... 90 {
            _ = motion.tick(now: Double(step) / 60)
            #expect(motion.contentFadeTarget == target)
        }
    }

    @Test("The quiet fade reaches zero about its duration after the bar starts moving")
    func quietFadeTimingMatchesCurve() {
        var motion = DockletMotion()
        motion.quietSettle(to: .tucked, now: 0)
        _ = motion.tick(now: 1.0 / 60)
        var zeroTime: TimeInterval?
        for step in 1 ... 360 {
            let now = 1.0 / 60 + Double(step) / 240
            _ = motion.tick(now: now)
            if motion.contentOpacity == 0 {
                zeroTime = now
                break
            }
        }
        let elapsed = (zeroTime ?? .infinity) - 1.0 / 60
        #expect(abs(elapsed - DockletContentFade.quietFadeDuration) <= 1.0 / 120)
    }
}
