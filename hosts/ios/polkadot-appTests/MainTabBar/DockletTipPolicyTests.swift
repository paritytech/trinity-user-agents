import CoreGraphics
import Testing
@testable import polkadot_app

@Suite("DockletTipPolicy")
struct DockletTipPolicyTests {
    @Test("The first user tuck shows the tucked tip and marks only that tip as seen")
    func firstUserTuckShowsTuckedTip() {
        let decision = DockletTipPolicy.decision(for: .user, seen: [], isAnotherTipShowing: false)
        #expect(decision == DockletTipDecision(tip: .tucked, tipsSeen: [.tucked]))
    }

    @Test("A user tuck after the tucked tip was seen shows nothing")
    func secondUserTuckShowsNothing() {
        let decision = DockletTipPolicy.decision(for: .user, seen: [.tucked], isAnotherTipShowing: false)
        #expect(decision == nil)
    }

    @Test("The first auto-hide shows the auto-hidden tip and marks both tips as seen")
    func firstAutoHideShowsAutoHiddenTip() {
        let decision = DockletTipPolicy.decision(for: .autoHide, seen: [], isAnotherTipShowing: false)
        #expect(decision == DockletTipDecision(tip: .autoHidden, tipsSeen: [.tucked, .autoHidden]))
    }

    @Test("An auto-hide after the tucked tip was seen still shows the auto-hidden tip, once")
    func autoHideAfterTuckedTipShowsAutoHiddenTipOnce() {
        let first = DockletTipPolicy.decision(for: .autoHide, seen: [.tucked], isAnotherTipShowing: false)
        #expect(first?.tip == .autoHidden)
        let second = DockletTipPolicy.decision(
            for: .autoHide,
            seen: [.tucked, .autoHidden],
            isAnotherTipShowing: false
        )
        #expect(second == nil)
    }

    @Test("No Docklet tip shows while another tab bar tip is on screen", arguments: [
        DockletTuckKind.user,
        .autoHide,
    ])
    func anotherTipBlocksDockletTip(_ tuck: DockletTuckKind) {
        #expect(DockletTipPolicy.decision(for: tuck, seen: [], isAnotherTipShowing: true) == nil)
    }

    @Test("A drag released beyond 56 pt restores the bar", arguments: [57, -57, 200] as [CGFloat])
    func flingBeyondDistanceRestores(_ translationX: CGFloat) {
        #expect(DockletTipPolicy.restores(afterDragging: translationX))
    }

    @Test("A drag released at or within 56 pt snaps back", arguments: [0, 20, 56, -56] as [CGFloat])
    func shortDragSnapsBack(_ translationX: CGFloat) {
        #expect(!DockletTipPolicy.restores(afterDragging: translationX))
    }
}
