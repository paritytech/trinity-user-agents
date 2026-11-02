import CoreGraphics
import Testing
@testable import polkadot_app

@Suite("DockletMotion")
struct DockletMotionTests {
    private let geometry = DockletGeometry(
        dockFrame: CGRect(x: 10, y: 785, width: 382, height: 62),
        bounds: CGRect(x: 0, y: 0, width: 402, height: 874)
    )

    /// Drags from `touchX` by `translationX`, then releases with no velocity at time zero.
    private func release(from touchX: CGFloat, translationX: CGFloat) -> DockletMotion {
        var motion = DockletMotion()
        motion.begin(touchX: touchX, geometry: geometry)
        motion.move(translationX: translationX)
        motion.end(velocityX: 0, now: 0)
        return motion
    }

    private func isNear(_ point: CGPoint, _ expected: CGPoint) -> Bool {
        abs(point.x - expected.x) < 1e-6 && abs(point.y - expected.y) < 1e-6
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
        #expect(point.applying(geometry.transform(progress: 0, anchor: .zero)) == point)
    }

    @Test("Tucked turns the bar a quarter about the pivot with the leading end up, then slides it out")
    func tuckedPoseIsQuarterTurnAndSlide() {
        let transform = geometry.transform(progress: 1, anchor: .zero)
        let pivot = geometry.pivot
        let leadingEnd = CGPoint(x: pivot.x - 100, y: pivot.y)
        #expect(isNear(pivot.applying(transform), CGPoint(x: pivot.x + geometry.slideOut, y: pivot.y)))
        #expect(isNear(
            leadingEnd.applying(transform),
            CGPoint(x: pivot.x + geometry.slideOut, y: pivot.y - 100)
        ))
    }

    @Test("Halfway slides an eighth of the slide out")
    func halfwaySlidesAnEighth() {
        let pivot = geometry.pivot
        let transform = geometry.transform(progress: 0.5, anchor: .zero)
        #expect(isNear(pivot.applying(transform), CGPoint(x: pivot.x + geometry.slideOut / 8, y: pivot.y)))
    }

    @Test("The Edge glow and Grab zone sit on the right edge at the tucked bar's top")
    func glowAndGrabZoneFollowTuckedBar() {
        #expect(geometry.edgeGlowRect == CGRect(x: 378, y: 420, width: 24, height: 200))
        #expect(geometry.grabZoneRect == CGRect(x: 355, y: 420, width: 47, height: 200))
    }
}
