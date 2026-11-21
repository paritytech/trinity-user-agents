import Testing
@testable import PolkadotUI

/// The rules that reconcile a product's trees with what is on screen. They are where two hosts would diverge.
@MainActor
struct WidgetMotionTests {
    private let frames = ManualMotionFrames()
    private let width = 360.0

    private func pan(value: Int) -> CustomMessageWidgetNode.Pan {
        CustomMessageWidgetNode.Pan(
            id: "turn",
            axis: .horizontal,
            distance: -1,
            value: value,
            maxStepsPerGesture: 1,
            period: 2,
            animation: .ease(duration: 0.5),
            actionId: "turned",
            label: "Turn over"
        )
    }

    private func motion(reduceMotion: Bool = false) -> (WidgetMotion, EventLog) {
        let motion = WidgetMotion(frames: frames, reduceMotion: { reduceMotion })
        let log = EventLog()
        motion.onEvent = { actionId, event in log.events.append("\(actionId):\(event)") }
        return (motion, log)
    }

    private func value(_ motion: WidgetMotion) -> Double? {
        motion.reading("turn")?.value
    }

    @Test("A repeated declared value changes nothing, so a tree sent before a turn settled cannot undo it")
    func repeatedDeclaredValueIsIgnored() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 0)])
        motion.beginDrag("turn")
        motion.drag("turn", travel: -270, extent: width)
        motion.release("turn", pointsPerSecond: 0, extent: width)
        frames.runToEnd()

        motion.apply(pans: [pan(value: 0)])

        #expect(value(motion) == 1)
        #expect(log.events == ["turned:began", "turned:settled(1)"])
    }

    @Test("A changed declared value animates there and reports where it rested")
    func changedDeclaredValueAnimates() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 0)])

        motion.apply(pans: [pan(value: 1)])
        frames.advance(by: 0.25)
        let midway = value(motion) ?? 0
        frames.runToEnd()

        #expect(midway > 0 && midway < 1)
        #expect(value(motion) == 1)
        #expect(log.events == ["turned:settled(1)"])
    }

    @Test("With Reduce Motion a declared change jumps, and reports the same")
    func reduceMotionJumps() {
        let (motion, log) = motion(reduceMotion: true)
        motion.apply(pans: [pan(value: 0)])

        motion.apply(pans: [pan(value: 1)])

        #expect(value(motion) == 1)
        #expect(log.events == ["turned:settled(1)"])
    }

    @Test("A declared change while a finger holds the pan waits, and the finger's rest is what the product hears")
    func fingerOutranksADeclaredChange() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 0)])
        motion.beginDrag("turn")
        motion.drag("turn", travel: -60, extent: width)

        motion.apply(pans: [pan(value: 1)])
        #expect(motion.isUserMoving)
        motion.release("turn", pointsPerSecond: 0, extent: width)
        frames.runToEnd()

        #expect(value(motion) == 0)
        #expect(!motion.isUserMoving)
        #expect(log.events == ["turned:began", "turned:settled(0)"])
    }

    /// The product redraws with the side it heard, which the card already rests on.
    @Test("A tree confirming where the card rests moves and reports nothing")
    func confirmationIsSilent() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 0)])
        motion.beginDrag("turn")
        motion.drag("turn", travel: -270, extent: width)
        motion.release("turn", pointsPerSecond: 0, extent: width)
        frames.runToEnd()

        motion.apply(pans: [pan(value: 1)])

        #expect(value(motion) == 1)
        #expect(log.events == ["turned:began", "turned:settled(1)"])
    }

    @Test("A press the system takes returns the value to where the drag began")
    func cancelledDragReturns() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 0)])
        motion.beginDrag("turn")
        motion.drag("turn", travel: -200, extent: width)

        motion.cancelDrag("turn")
        frames.runToEnd()

        #expect(value(motion) == 0)
        #expect(log.events == ["turned:began", "turned:cancelled"])
    }

    @Test("The accessibility action steps forward and reports the rest")
    func accessibilityStep() {
        let (motion, log) = motion()
        motion.apply(pans: [pan(value: 1)])

        motion.step("turn")
        frames.runToEnd()

        #expect(value(motion) == 2)
        #expect(log.events == ["turned:settled(2)"])
    }

    @Test("A tree that leaves a pan out drops it, and one that brings it back starts from its declared value")
    func droppedPanStartsOver() {
        let (motion, _) = motion()
        motion.apply(pans: [pan(value: 0)])
        motion.apply(pans: [pan(value: 1)])
        frames.runToEnd()

        motion.apply(pans: [])
        #expect(motion.reading("turn") == nil)
        motion.apply(pans: [pan(value: 0)])

        #expect(value(motion) == 0)
    }
}

private final class EventLog {
    var events: [String] = []
}

/// Steps settles only when a test says so.
@MainActor
private final class ManualMotionFrames: WidgetMotionFrames {
    private var step: ((Double) -> Bool)?

    func start(_ step: @escaping (Double) -> Bool) {
        self.step = step
    }

    func advance(by seconds: Double) {
        guard let step, step(seconds) else {
            step = nil
            return
        }
    }

    func runToEnd() {
        for _ in 0 ..< 600 where step != nil {
            advance(by: 1.0 / 60)
        }
    }
}
