import CoreGraphics
import Foundation
@testable import PolkadotUI
import Testing

private typealias Mask = CustomMessageWidgetNode.Mask

/// Mask progress: a share checked mid-stroke, a rescue for a cover that stopped growing, and haptic steps.
struct MaskProgressTests {
    private typealias Input = MaskShareWatch.Input

    @Test("With whileDown, a followed share is checked as the finger moves, at most once per lift")
    func checksWhileDown() {
        var watch = MaskShareWatch()
        let sends = [
            Input(down: true),
            Input(down: true, moves: 2),
            Input(down: true, moves: 3),
            Input(down: false, lifts: 1, moves: 3),
            Input(down: false, lifts: 1, additions: 1, moves: 3),
            Input(down: true, lifts: 1, additions: 1, moves: 3),
            Input(down: true, lifts: 1, additions: 1, moves: 4),
        ].map { watch.update($0, follows: true, whileDown: true) { true } }
        // Touching down is not a move; the first move that reaches the share sends, once for the touch; the lift's
        // check was spent by it, and so was an addition before the next touch; a move on a new touch starts over.
        #expect(sends == [false, true, false, false, false, false, true])
    }

    @Test("Without whileDown, nothing is sent while the finger is down")
    func waitsForTheLift() {
        var watch = MaskShareWatch()
        let sends = [Input(down: true, moves: 5), Input(down: false, lifts: 1, moves: 5)]
            .map { watch.update($0, follows: true) { true } }
        #expect(sends == [false, true])
    }

    @Test("Steps between two covers follow the core's rule")
    func steps() {
        let every = { (permille: Int) in Mask.Haptics(everyPermille: permille, step: .tick) }
        #expect(every(100).stepsPassed(from: 95, to: 105) == 1)
        #expect(every(100).stepsPassed(from: 100, to: 199) == 0)
        #expect(every(100).stepsPassed(from: 99, to: 100) == 1)
        #expect(every(100).stepsPassed(from: 0, to: 1_000) == 10)
        #expect(every(100).stepsPassed(from: 500, to: 400) == 0)
        #expect(every(0).stepsPassed(from: 0, to: 1_000) == 0)
    }

    @Test("Haptic steps count from the tree's own cover, once for a measure however many steps it passed")
    func hapticSteps() {
        let haptics = Mask.Haptics(everyPermille: 100, step: .tick, reach: .success)
        var cover = MaskCoverWatch(covered: 150, total: 1_000, at: 0)
        let steps = [150, 199, 420, 430].enumerated().map { time, covered in
            cover.measured(covered: covered, total: 1_000, at: TimeInterval(time + 1), haptics: haptics)
        }
        // The tree's own cover plays nothing; passing two steps at once plays one.
        #expect(steps == [false, false, true, false])
        cover.tree(covered: 800, total: 1_000, at: 5)
        let afterTree = cover.measured(covered: 800, total: 1_000, at: 6, haptics: haptics)
        #expect(!afterTree, "a new tree plays nothing")
    }

    @Test("The rescue is due once the cover has stood still for its time while at least its share")
    func rescue() {
        let share = Mask.ClearedShare(
            atPermille: 600, action: "done", rescue: Mask.Rescue(after: 1.5, fromPermille: 450)
        )
        var cover = MaskCoverWatch(covered: 0, total: 1_000, at: 10)
        #expect(cover.rescueDue(share: share) == nil, "below the rescue's share")
        _ = cover.measured(covered: 460, total: 1_000, at: 11, haptics: nil)
        #expect(cover.rescueDue(share: share) == 12.5, "the clock restarts when the cover grows")
        _ = cover.measured(covered: 460, total: 1_000, at: 12, haptics: nil)
        #expect(cover.rescueDue(share: share) == 12.5, "the same cover does not restart it")
        cover.sent()
        #expect(cover.rescueDue(share: share) == nil, "at most once per tree")
        cover.tree(covered: 500, total: 1_000, at: 20)
        #expect(cover.rescueDue(share: share) == 21.5, "a new tree starts the clock over")
        let unreachable = Mask.ClearedShare(
            atPermille: 400, action: "done", rescue: Mask.Rescue(after: 1, fromPermille: 450)
        )
        #expect(cover.rescueDue(share: unreachable) == nil, "a rescue share above the share's own never sends")
    }

    @Test("A trail counts its places, and a tree drops them")
    @MainActor
    func trailPoints() {
        let trail = PanTrail()
        trail.begin(at: .zero)
        trail.extend(to: CGPoint(x: 1, y: 0))
        trail.extend(to: CGPoint(x: 1, y: 0))
        #expect(trail.points == 2, "a repeated place is not a new one")
        trail.reset()
        #expect(trail.points == 0)
    }
}
