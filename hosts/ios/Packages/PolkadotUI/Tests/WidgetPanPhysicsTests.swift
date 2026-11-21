import Testing
@testable import PolkadotUI

/// Where every turn lands: the product only hears the step a release rests on.
struct WidgetPanPhysicsTests {
    /// One card width (360 pt) to the left turns one step, at most one step per swipe.
    private let pan = CustomMessageWidgetNode.Pan(
        id: "turn",
        axis: .horizontal,
        distance: -1,
        value: 0,
        maxStepsPerGesture: 1,
        period: 2,
        animation: nil,
        actionId: "turned",
        label: nil
    )
    private let width = 360.0

    @Test("A leftward drag of a quarter card width moves a quarter step forward")
    func dragFollowsTheFinger() {
        #expect(WidgetPanPhysics.dragged(from: 0, origin: 0, travel: -90, extent: width, pan: pan) == 0.25)
        #expect(WidgetPanPhysics.dragged(from: 0, origin: 0, travel: 90, extent: width, pan: pan) == -0.25)
    }

    @Test("A drag never moves further than one step from where it began")
    func dragIsClampedToOneStep() {
        #expect(WidgetPanPhysics.dragged(from: 0, origin: 0, travel: -900, extent: width, pan: pan) == 1)
        #expect(WidgetPanPhysics.dragged(from: 0, origin: 0, travel: 900, extent: width, pan: pan) == -1)
    }

    /// Taken mid-settle, the value continues from where it was instead of jumping to where it was going.
    @Test("A drag taken mid-settle continues from the value it took")
    func dragContinuesFromItsStart() {
        #expect(WidgetPanPhysics.dragged(from: 0.6, origin: 1, travel: 0, extent: width, pan: pan) == 0.6)
    }

    @Test("A slow release rests on the nearest step, and halfway rounds up", arguments: [
        (0.3, 0), (0.5, 1), (0.7, 1), (-0.5, 0), (-0.6, -1)
    ])
    func slowReleaseRestsOnNearest(value: Double, rest: Int) {
        #expect(WidgetPanPhysics.restTarget(value: value, velocity: 0, from: 0, pan: pan) == rest)
    }

    @Test("A flick projects ahead along its velocity")
    func flickProjects() {
        #expect(WidgetPanPhysics.restTarget(value: 0.2, velocity: 2, from: 0, pan: pan) == 1)
        #expect(WidgetPanPhysics.restTarget(value: 0.2, velocity: -2, from: 0, pan: pan) == 0)
    }

    @Test("A hard flick still rests within one step of where the drag began")
    func hardFlickIsCapped() {
        #expect(WidgetPanPhysics.restTarget(value: 0.9, velocity: 100, from: 0, pan: pan) == 1)
    }

    @Test("Velocity in points converts to steps along the pan's direction")
    func velocityConverts() {
        #expect(WidgetPanPhysics.velocity(-720, extent: width, pan: pan) == 2)
    }

    @Test("A settle ends exactly on its target")
    func settleEndsOnTarget() {
        let settle = WidgetPanSettle(start: 0.4, target: 1, velocity: 0, curve: .spring(WidgetPanPhysics.releaseSpring))

        #expect(settle.value(at: settle.duration) == 1)
        #expect(settle.value(at: 0) == 0.4)
    }
}
