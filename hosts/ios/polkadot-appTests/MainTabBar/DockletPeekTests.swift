import CoreGraphics
import Foundation
import Testing
@testable import polkadot_app

@Suite("DockletPeek")
struct DockletPeekTests {
    private func isNear(_ value: CGFloat, _ expected: CGFloat) -> Bool {
        abs(value - expected) < 1e-6
    }

    @Test("The first tuck arms the peek and later tucks do not")
    func peekArmsOnce() {
        var peek = DockletPeek()
        let firstTuck = peek.arm()
        let laterTuck = peek.arm()
        #expect(firstTuck)
        #expect(!laterTuck)
    }

    @Test("A peek that played does not replay on a later tuck")
    func playedPeekStaysSpent() {
        var peek = DockletPeek()
        _ = peek.arm()
        peek.start(at: 0)
        peek.stop()
        let laterTuck = peek.arm()
        #expect(!laterTuck)
    }

    @Test("A drag cancels a playing peek, and it does not replay")
    func dragCancelsPeek() {
        var peek = DockletPeek()
        _ = peek.arm()
        peek.start(at: 10)
        peek.stop()
        let laterTuck = peek.arm()
        #expect(!peek.isPlaying)
        #expect(isNear(peek.progressDip(at: 10.2), 0))
        #expect(!laterTuck)
    }

    @Test("A started peek is over 400 ms later, not before")
    func peekEndsAfterDuration() {
        var peek = DockletPeek()
        _ = peek.arm()
        peek.start(at: 10)
        #expect(!peek.isOver(at: 10.3))
        #expect(peek.isOver(at: 10.4))
    }

    @Test("A playing peek takes the progress back by the curve at the given time")
    func playingPeekDipsProgress() {
        var peek = DockletPeek()
        _ = peek.arm()
        peek.start(at: 5)
        #expect(isNear(peek.progressDip(at: 5.2), DockletPeek.depth))
    }
}
