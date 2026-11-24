import CoreGraphics
@testable import PolkadotUI
import Testing

/// The same cases as the core's `cleared_share` tests, in points: a 10 by 6 point node is 5 by 3 samples.
struct MaskCoverageTests {
    private typealias Shape = CustomMessageWidgetNode.Mask.Shape
    private let node = CGSize(width: 10, height: 6)

    @Test("Samples are 2 point squares from the top-start corner, a part square counting as whole")
    func samples() {
        #expect(MaskCoverage.count([], in: node) == (0, 15))
        #expect(MaskCoverage.count([.all], in: node) == (15, 15))
        #expect(MaskCoverage.count([], in: CGSize(width: 10.01, height: 6)) == (0, 18))
        #expect(MaskCoverage.count([], in: CGSize(width: 0, height: 6)) == (0, 0))
    }

    @Test("A shape covers the samples on its edge and not one past it")
    func edges() {
        let circle = { (radius: CGFloat) in [Shape.circle(center: CGPoint(x: 1, y: 1), radius: radius)] }
        #expect(MaskCoverage.count(circle(2), in: node) == (3, 15))
        #expect(MaskCoverage.count(circle(1.99), in: node) == (1, 15))
        let row = { (width: CGFloat) in
            [Shape.stroke(width: width, points: [CGPoint(x: 1, y: 3), CGPoint(x: 9, y: 3)])]
        }
        #expect(MaskCoverage.count(row(4), in: node) == (15, 15))
        #expect(MaskCoverage.count(row(3.98), in: node) == (5, 15))
        let dot = [Shape.stroke(width: 4, points: [CGPoint(x: 5, y: 3)])]
        #expect(MaskCoverage.count(dot, in: node) == (5, 15))
        let outside = [Shape.circle(center: CGPoint(x: -1, y: -1), radius: 3)]
        #expect(MaskCoverage.count(outside, in: node) == (1, 15))
        #expect(MaskCoverage.count(circle(2) + circle(2), in: node) == (3, 15))
    }

    /// A followed pan's strokes, as the trail keeps them and the lift counts them: on a 290 by 380 point node at
    /// (56, 157) on screen, two crossing strokes and a press, then rows across it, 52 points wide. The core's
    /// `cleared_share` gives 534 thousandths after the fourth row and 583 after the fifth, so 550 is reached on the
    /// fifth.
    @Test("Followed strokes reach a share on the same row as the core's rule")
    @MainActor
    func followedStrokesReachTheShareOnTheCoresRow() {
        let frame = CGRect(x: 56, y: 157, width: 290, height: 380)
        let mask = CustomMessageWidgetNode.Mask(key: "k", follow: .init(pan: "p", width: 52))
        let share = CustomMessageWidgetNode.Mask.ClearedShare(atPermille: 550, action: "a")
        let trail = PanTrail()
        trail.origin = frame.origin
        let local = { (x: CGFloat, y: CGFloat) in CGPoint(x: x - frame.minX, y: y - frame.minY) }
        func stroke(_ points: [CGPoint]) {
            trail.begin(at: points[0])
            for point in points.dropFirst() {
                trail.extend(to: point)
            }
        }
        stroke([local(90, 220), local(290, 340)])
        stroke([local(300, 290), local(80, 360)])
        stroke([local(250, 470)])
        var permille: [Int] = []
        var reached: [Bool] = []
        for y: CGFloat in [190, 230, 270, 310, 350, 390] {
            stroke([local(70, y), local(330, y)])
            let source = MaskSource.of(mask: mask, trail: trail, frame: frame, mirrored: false)
            let count = MaskCoverage.count(source.picture(added: []).shapes, in: frame.size)
            permille.append(count.covered * 1_000 / count.total)
            reached.append(share.reached(covered: count.covered, samples: count.total))
        }
        #expect(permille == [374, 445, 496, 534, 583, 685])
        #expect(reached == [false, false, false, false, true, true])
    }

    @Test("A share is reached exactly at its line")
    func reached() {
        let share = { (permille: Int) in CustomMessageWidgetNode.Mask.ClearedShare(atPermille: permille, action: "a") }
        #expect(share(550).reached(covered: 713, samples: 1_295))
        #expect(!share(550).reached(covered: 712, samples: 1_295))
        #expect(share(500).reached(covered: 2, samples: 4))
        #expect(!share(500).reached(covered: 1, samples: 4))
        #expect(share(0).reached(covered: 0, samples: 4))
        #expect(!share(1_001).reached(covered: 4, samples: 4))
    }
}

struct MaskShareWatchTests {
    private typealias Input = MaskShareWatch.Input

    @Test("A followed pan checks on the lift, then on each addition until it touches down again, once per lift")
    func followedPanChecksAfterTheLift() {
        var sent = false
        var watch = MaskShareWatch()
        var asked = 0
        let no = { asked += 1; return false }
        let yes = { asked += 1; return true }
        sent = watch.update(Input(down: true), follows: true, reached: yes)
        #expect(!sent, "never while the finger is down")
        sent = watch.update(Input(down: true, additions: 1), follows: true, reached: yes)
        #expect(!sent)
        #expect(asked == 0)
        sent = watch.update(Input(lifts: 1, additions: 1), follows: true, reached: no)
        #expect(!sent, "checked on the lift")
        #expect(asked == 1)
        sent = watch.update(Input(lifts: 1, additions: 2), follows: true, reached: yes)
        #expect(sent, "an answer to the end counts")
        sent = watch.update(Input(lifts: 1, additions: 3), follows: true, reached: yes)
        #expect(!sent, "once per lift")
        sent = watch.update(Input(lifts: 1, trees: 1, additions: 3), follows: true, reached: yes)
        #expect(!sent)
        sent = watch.update(Input(down: true, lifts: 1, additions: 3), follows: true, reached: yes)
        #expect(!sent)
        sent = watch.update(Input(lifts: 2, additions: 3), follows: true, reached: yes)
        #expect(sent, "a new lift checks again")
    }

    @Test("A touch-down before the share is reached disarms the lift before it")
    func touchDownDisarms() {
        var sent = false
        var watch = MaskShareWatch()
        sent = watch.update(Input(lifts: 1), follows: true, reached: { false })
        #expect(!sent)
        sent = watch.update(Input(down: true, lifts: 1), follows: true, reached: { true })
        #expect(!sent)
        sent = watch.update(Input(lifts: 1, additions: 1), follows: true, reached: { true })
        #expect(!sent, "not after a new touch")
    }

    @Test("A tree alone never checks")
    func treeAloneNeverChecks() {
        var sent = false
        var watch = MaskShareWatch(Input(lifts: 1))
        sent = watch.update(Input(lifts: 1, trees: 1), follows: true, reached: { true })
        #expect(!sent)
        sent = watch.update(Input(trees: 1), follows: false, reached: { true })
        #expect(!sent)
    }

    @Test("With no pan to follow, each addition checks, once per tree")
    func unfollowedChecksOnAdditions() {
        var sent = false
        var watch = MaskShareWatch()
        sent = watch.update(Input(additions: 1), follows: false, reached: { false })
        #expect(!sent)
        sent = watch.update(Input(additions: 2), follows: false, reached: { true })
        #expect(sent)
        sent = watch.update(Input(additions: 3), follows: false, reached: { true })
        #expect(!sent, "once per tree")
        sent = watch.update(Input(trees: 1, additions: 4), follows: false, reached: { true })
        #expect(sent, "a new tree, again")
    }

    @Test("What happened before the mask was drawn sends nothing")
    func pastLiftsSendNothing() {
        var sent = false
        var watch = MaskShareWatch(Input(lifts: 3, additions: 5))
        sent = watch.update(Input(lifts: 3, trees: 1, additions: 5), follows: true, reached: { true })
        #expect(!sent)
    }
}
