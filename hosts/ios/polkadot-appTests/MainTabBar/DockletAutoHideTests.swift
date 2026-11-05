import Foundation
import Testing
@testable import polkadot_app

@Suite("DockletAutoHide")
struct DockletAutoHideTests {
    private let idle = DockletAutoHide.idleSeconds

    @Test("An outside touch makes the hide due at once")
    func outsideTouchHidesAtOnce() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: false, at: 1)
        #expect(autoHide.deadline == 1)
        #expect(autoHide.isDue(at: 1))
    }

    @Test("A second outside touch leaves the pending hide where it was")
    func secondOutsideTouchKeepsDeadline() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: false, at: 1)
        autoHide.touch(isOnDocklet: false, at: 1.5)
        #expect(autoHide.deadline == 1)
    }

    @Test("A touch on the Docklet cancels a pending outside-touch hide and restarts the idle countdown")
    func touchOnDockletCancelsPendingHide() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: false, at: 1)
        autoHide.touch(isOnDocklet: true, at: 1.2)
        #expect(autoHide.deadline == 1.2 + idle)
    }

    @Test("Three seconds of idle make the hide due")
    func idleHidesAfterThreeSeconds() {
        let autoHide = DockletAutoHide(startedAt: 0)
        #expect(!autoHide.isDue(at: 2.9))
        #expect(autoHide.isDue(at: idle))
    }

    @Test("A touch at 2.9 s pushes the hide to 5.9 s")
    func touchRestartsIdle() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: true, at: 2.9)
        #expect(autoHide.deadline == 2.9 + idle)
    }

    @Test("A key press restarts the idle countdown like a touch does")
    func keyPressRestartsIdle() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.keyPress(at: 2.9)
        #expect(autoHide.deadline == 2.9 + idle)
    }

    @Test("Each blocker holds the hide back", arguments: [
        DockletHideBlocker.panelOpen,
        .keyboardUp,
        .motionRunning,
        .notShown,
        .tipShown,
        .voiceOver,
    ])
    func blockerSuppressesHide(_ blocker: DockletHideBlocker) {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.setBlocked(blocker, true, at: 0)
        #expect(autoHide.deadline == nil)
        #expect(!autoHide.isDue(at: 100))
    }

    @Test("Switching VoiceOver off restarts the idle countdown from the time it was switched off")
    func clearingVoiceOverRestartsCountdown() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.setBlocked(.voiceOver, true, at: 0)
        autoHide.setBlocked(.voiceOver, false, at: 20)
        #expect(autoHide.deadline == 20 + idle)
        #expect(!autoHide.isDue(at: 22.9))
        #expect(autoHide.isDue(at: 23))
    }

    @Test("Clearing the last blocker restarts the idle countdown from the clear time")
    func clearingLastBlockerRestartsCountdown() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: true, at: 1)
        autoHide.setBlocked(.panelOpen, true, at: 1)
        autoHide.setBlocked(.panelOpen, false, at: 12)
        #expect(autoHide.deadline == 15)
        #expect(!autoHide.isDue(at: 12))
        #expect(autoHide.isDue(at: 15))
    }

    @Test("The countdown waits until every blocker has cleared, then restarts at the last clear")
    func countdownWaitsForEveryBlocker() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.setBlocked(.keyboardUp, true, at: 0)
        autoHide.setBlocked(.motionRunning, true, at: 0)
        autoHide.setBlocked(.keyboardUp, false, at: 2)
        #expect(autoHide.deadline == nil)
        autoHide.setBlocked(.motionRunning, false, at: 4)
        #expect(autoHide.deadline == 4 + idle)
    }

    @Test("Clearing a blocker that was never held leaves the idle countdown alone")
    func clearingUnheldBlockerKeepsCountdown() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.setBlocked(.notShown, false, at: 2)
        #expect(autoHide.deadline == idle)
    }

    @Test("An outside touch made while a panel is open does not hide the Docklet once the panel closes")
    func outsideTouchWhileBlockedDoesNotHide() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.setBlocked(.panelOpen, true, at: 0)
        autoHide.touch(isOnDocklet: false, at: 1)
        #expect(autoHide.deadline == nil)
        autoHide.setBlocked(.panelOpen, false, at: 2)
        #expect(autoHide.deadline == 2 + idle)
    }

    @Test("Holding a blocker drops a pending outside-touch hide")
    func blockerDropsPendingOutsideHide() {
        var autoHide = DockletAutoHide(startedAt: 0)
        autoHide.touch(isOnDocklet: false, at: 1)
        autoHide.setBlocked(.keyboardUp, true, at: 2)
        autoHide.setBlocked(.keyboardUp, false, at: 3)
        #expect(autoHide.deadline == 3 + idle)
    }
}
