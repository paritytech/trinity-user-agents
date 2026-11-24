import CoreGraphics
import Foundation
@testable import PolkadotUI
import Testing

private typealias Node = CustomMessageWidgetNode

/// Enter animations and tracks: the core's easing and timeline rules, followed values, and how a node's motion is
/// kept across trees.
struct NodeMotionTests {
    private static let ease = Node.Curve(x1: 0.25, y1: 0.1, x2: 0.25, y2: 1)

    /// The core's test timeline: up to 1000 at 100 ms, back to 600 at 300 ms, linear.
    private static func riseAndSettle(runs: Int, alternate: Bool = false) -> Node.Keyframes {
        Node.Keyframes(
            frames: [.init(at: 0, value: 0), .init(at: 0.1, value: 1_000), .init(at: 0.3, value: 600)],
            runs: runs,
            alternate: alternate
        )
    }

    private static func rounded(_ value: Double?) -> Int? {
        value.map { Int($0.rounded()) }
    }

    @Test("A curve eases as CSS cubic-bezier does, as the core's Curve::ease")
    func curve() {
        #expect(abs(Node.Curve.linear.ease(0.25) - 0.25) < 1e-3)
        #expect(abs(Self.ease.ease(0) - 0) < 1e-3)
        #expect(abs(Self.ease.ease(1) - 1) < 1e-3)
        #expect(abs(Self.ease.ease(0.5) - 0.8024) < 1e-3)
        #expect(abs(Self.ease.ease(2) - 1) < 1e-3, "time is clamped")
        #expect(Node.Curve(x1: 0.34, y1: 1.56, x2: 0.64, y2: 1).ease(0.6) > 1, "progress may overshoot")
    }

    @Test("Keyframes play their runs and hold where the last run ended, as the core's Keyframes::value_at")
    func keyframeRuns() {
        let once = Self.riseAndSettle(runs: 2)
        let values = [0.05, 0.1, 0.2, 0.35, 0.6, 60].map { Self.rounded(once.value(at: $0)) }
        #expect(values == [500, 1_000, 800, 500, 600, 600])
        let back = Self.riseAndSettle(runs: 2, alternate: true)
        #expect([0.3, 0.35, 0.6].map { Self.rounded(back.value(at: $0)) } == [600, 700, 0])
        #expect(Self.rounded(Self.riseAndSettle(runs: 0).value(at: 3.05)) == 500, "for ever repeats")
    }

    @Test("Keyframes out of order or at one time jump, and no keyframes give no value")
    func keyframeOrder() {
        let frames = { (times: [(TimeInterval, Double)]) in
            Node.Keyframes(frames: times.map { Node.Keyframe(at: $0.0, value: $0.1) }, runs: 1)
        }
        let late = frames([(0, 0), (0.2, 1_000), (0.1, 500)])
        #expect([0.1, 0.2, 0.9].map { Self.rounded(late.value(at: $0)) } == [500, 500, 500])
        #expect(frames([(0, 7), (0, 8), (0, 9)]).value(at: 0) == 9)
        #expect(Node.Keyframes(frames: []).value(at: 0.005) == nil)
        let many = (0 ..< 40).map { Node.Keyframe(at: Double($0) / 10, value: Double($0)) }
        #expect(Node.Keyframes(frames: many).frames.count == 32, "at most 32 are drawn")
    }

    @Test("Under Reduce Motion a timeline holds its end, or its first keyframe when it plays for ever")
    func restingValue() {
        #expect(Self.rounded(Self.riseAndSettle(runs: 2).restingValue) == 600)
        #expect(Self.rounded(Self.riseAndSettle(runs: 2, alternate: true).restingValue) == 0)
        #expect(Self.riseAndSettle(runs: 0).restingValue == 0)
        #expect(Self.riseAndSettle(runs: 2).isOver(at: 0.6))
        #expect(!Self.riseAndSettle(runs: 2).isOver(at: 0.59))
        #expect(!Self.riseAndSettle(runs: 0).isOver(at: 600))
    }

    @Test("A spring ends within a thousandth of its change, moving less than that in 1/60 s")
    func springEnds() {
        let spring = SpringMotion(omega: 18, damping: 0.8)
        #expect(!spring.isDone(start: -1, speed: 0, change: 1, time: 0.1))
        #expect(spring.isDone(start: -1, speed: 0, change: 1, time: 2))
        #expect(abs(spring.displacement(start: -1, speed: 0, time: 0) + 1) < 1e-9, "it starts at rest")
    }

    @Test("A followed value first appears at its target, and a new target is reached from where it was drawn")
    func followRetargets() {
        let spring = Node.Motion.spring(omega: 18, damping: 0.8)
        var run = FollowRun.resting(at: 10)
        #expect(run.value(at: 5) == 10 && run.isDone(at: 5))
        run = run.retargeted(110, motion: spring, at: 1)
        #expect(abs(run.value(at: 1) - 10) < 1e-9)
        let spring18 = SpringMotion(omega: 18, damping: 0.8)
        let measured = (spring18.displacement(start: -1, speed: 3, time: 0.2001)
            - spring18.displacement(start: -1, speed: 3, time: 0.1999)) / 0.0002
        #expect(abs(spring18.speed(start: -1, speed: 3, time: 0.2) - measured) < 1e-4, "the spring's own speed")
        #expect(SpringMotion(omega: 10, damping: 1).speed(start: -1, speed: 2, time: 0) == 2)
        #expect(abs(SpringMotion(omega: 10, damping: 1.5).speed(start: -1, speed: 2, time: 0) - 2) < 1e-9)
        let moving = run.speed(at: 1.05)
        let turned = run.retargeted(0, motion: spring, at: 1.05)
        #expect(abs(turned.value(at: 1.05) - run.value(at: 1.05)) < 1e-6, "no jump")
        #expect(abs(turned.speed(at: 1.05) - moving) < abs(moving) * 0.01, "a spring keeps its speed")
        #expect(turned.isDone(at: 4) && abs(turned.value(at: 4)) < 0.1)
        let timed = FollowRun.resting(at: 0).retargeted(100, motion: .timed(duration: 0.2, curve: .linear), at: 0)
        #expect(abs(timed.value(at: 0.1) - 50) < 1e-9 && timed.value(at: 1) == 100)
    }

    @Test("Enter holds its pose for its delay, then moves to how the tree draws the node")
    func enter() {
        let enter = Node.Enter(
            from: Node.Pose(opacity: 0, scale: 0.9, offset: CGSize(width: 0, height: 12), rotation: -5),
            motion: .timed(duration: 0.2, curve: .linear),
            delay: 0.05
        )
        var state = NodeMotionState()
        state.appear(tracks: [], at: 10)
        let held = state.pose(enter: enter, at: 10.04, reduceMotion: false)
        #expect(held == MotionPose(opacity: 0, scale: 0.9, offset: CGSize(width: 0, height: 12), rotation: -5))
        let half = state.pose(enter: enter, at: 10.15, reduceMotion: false)
        #expect(abs(half.opacity - 0.5) < 1e-9 && abs(half.offset.height - 6) < 1e-9)
        #expect(state.pose(enter: enter, at: 10.25, reduceMotion: false) == .identity)
        #expect(state.isSettled(enter: enter, at: 10.25) && !state.isSettled(enter: enter, at: 10.2))
        #expect(state.pose(enter: enter, at: 10, reduceMotion: true) == .identity, "Reduce Motion draws the tree")
        #expect(NodeMotionState().pose(enter: enter, at: 0, reduceMotion: false).opacity == 0, "before it appears")
    }

    @Test("Tracks compose with the enter: opacity and scale multiply, offsets and turn add")
    func compose() {
        let enter = Node.Enter(from: Node.Pose(opacity: 0.5, scale: 2), motion: .timed(duration: 1, curve: .linear))
        var state = NodeMotionState()
        state.appear(tracks: [
            Node.Track(property: .opacity, play: .follow(target: 0.5, motion: .timed(duration: 1, curve: .linear))),
            Node.Track(property: .scale, play: .follow(target: 3, motion: .timed(duration: 1, curve: .linear))),
            Node.Track(property: .offsetX, play: .follow(target: 4, motion: .timed(duration: 1, curve: .linear))),
        ], at: 0)
        let pose = state.pose(enter: enter, at: 0, reduceMotion: false)
        #expect(abs(pose.opacity - 0.25) < 1e-9 && abs(pose.scale - 6) < 1e-9 && pose.offset.width == 4)
    }

    @Test("A tree that keeps a timeline keeps its clock; a changed one starts over; a dropped one lets go")
    func treesKeepClocks() {
        let timeline = Node.Track(property: .rotation, play: .keyframes(Self.riseAndSettle(runs: 0)))
        var state = NodeMotionState()
        state.appear(tracks: [timeline], at: 0)
        state.update([timeline], at: 0.05)
        #expect(Self.rounded(state.pose(enter: nil, at: 0.1, reduceMotion: false).rotation) == 1_000)
        let changed = Node.Track(property: .rotation, play: .keyframes(Self.riseAndSettle(runs: 1)))
        state.update([changed], at: 0.1)
        #expect(Self.rounded(state.pose(enter: nil, at: 0.15, reduceMotion: false).rotation) == 500)
        state.update([], at: 0.2)
        #expect(state.pose(enter: nil, at: 0.25, reduceMotion: false) == .identity)
    }

    @Test("Under Reduce Motion a followed value changes at once")
    func reducedFollow() {
        var state = NodeMotionState()
        let track = { (target: Double) in
            Node.Track(property: .offsetY, play: .follow(target: target, motion: .timed(duration: 1, curve: .linear)))
        }
        state.appear(tracks: [track(0)], at: 0)
        state.update([track(40)], at: 1)
        #expect(state.pose(enter: nil, at: 1, reduceMotion: true).offset.height == 40)
        #expect(abs(state.pose(enter: nil, at: 1, reduceMotion: false).offset.height) < 1e-6)
    }
}
