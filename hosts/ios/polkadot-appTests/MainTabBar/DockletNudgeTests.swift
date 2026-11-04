import CoreGraphics
import Foundation
import Testing
@testable import polkadot_app

@Suite("DockletNudge")
struct DockletNudgeTests {
    private func isNear(_ value: CGFloat, _ expected: CGFloat) -> Bool {
        abs(value - expected) < 1e-6
    }

    @Test("The first tuck arms the nudge and later tucks do not")
    func nudgeArmsOnce() {
        var nudge = DockletNudge()
        let firstTuck = nudge.arm()
        let laterTuck = nudge.arm()
        #expect(firstTuck)
        #expect(!laterTuck)
    }

    @Test("A nudge that played does not replay on a later tuck")
    func playedNudgeStaysSpent() {
        var nudge = DockletNudge()
        _ = nudge.arm()
        nudge.start(at: 0)
        nudge.stop()
        let laterTuck = nudge.arm()
        #expect(!laterTuck)
    }

    @Test("A drag cancels a playing nudge, and it does not replay")
    func dragCancelsNudge() {
        var nudge = DockletNudge()
        _ = nudge.arm()
        nudge.start(at: 10)
        nudge.stop()
        let laterTuck = nudge.arm()
        #expect(!nudge.isPlaying)
        #expect(isNear(nudge.inset(at: 10.2), 0))
        #expect(!laterTuck)
    }

    @Test("A started nudge is over 400 ms later, not before")
    func nudgeEndsAfterDuration() {
        var nudge = DockletNudge()
        _ = nudge.arm()
        nudge.start(at: 10)
        #expect(!nudge.isOver(at: 10.3))
        #expect(nudge.isOver(at: 10.4))
    }

    @Test("A playing nudge insets the bar by the curve at the given time")
    func playingNudgeInsetsBar() {
        var nudge = DockletNudge()
        _ = nudge.arm()
        nudge.start(at: 5)
        #expect(isNear(nudge.inset(at: 5.2), 8))
    }
}
