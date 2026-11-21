import Testing
@testable import PolkadotUI

/// A turning card swaps its faces by these values, so an error here draws the wrong face at the wrong angle.
struct WidgetKeyframesTests {
    private typealias Keyframe = CustomMessageWidgetNode.Keyframe

    private func drive(_ keyframes: [Keyframe]) -> CustomMessageWidgetNode.Drive {
        CustomMessageWidgetNode.Drive(source: "turn", property: .rotationY, keyframes: keyframes)
    }

    private let rotation = [Keyframe(at: 0, value: 0, curve: .linear), Keyframe(at: 2, value: 360, curve: .linear)]

    /// The front's opacity: shown until the quarter turn, hidden through the back, shown again from the third quarter.
    private let frontOpacity = [
        Keyframe(at: 0, value: 1, curve: .linear),
        Keyframe(at: 0.5, value: 0, curve: .step),
        Keyframe(at: 1.5, value: 1, curve: .step)
    ]

    @Test("A linear keyframe interpolates from the one before it", arguments: [
        (0.0, 0.0), (0.25, 45.0), (1.0, 180.0), (1.75, 315.0)
    ])
    func linearInterpolates(value: Double, degrees: Double) {
        #expect(drive(rotation).value(at: value, period: nil) == degrees)
    }

    @Test("A step holds the previous value until its own, then jumps", arguments: [
        (0.0, 1.0), (0.49, 1.0), (0.5, 0.0), (1.49, 0.0), (1.5, 1.0), (1.9, 1.0)
    ])
    func stepHoldsThenJumps(value: Double, opacity: Double) {
        #expect(drive(frontOpacity).value(at: value, period: nil) == opacity)
    }

    @Test("Outside the keyframes the property holds the nearest end")
    func holdsOutsideTheRange() {
        let middle = [Keyframe(at: 1, value: 10, curve: .linear), Keyframe(at: 2, value: 20, curve: .linear)]

        #expect(drive(middle).value(at: 0, period: nil) == 10)
        #expect(drive(middle).value(at: 5, period: nil) == 20)
    }

    /// A card turned twice rests on 2, and one turned back from the front on -1: both must read as a face.
    @Test("A wrapping pan reads modulo its period, from below zero too", arguments: [
        (2.0, 1.0), (2.25, 1.0), (-0.5, 1.0), (-1.0, 0.0), (3.0, 0.0)
    ])
    func wrapsModuloThePeriod(value: Double, opacity: Double) {
        #expect(drive(frontOpacity).value(at: value, period: 2) == opacity)
    }

    @Test("A drive without keyframes leaves the node as it is")
    func emptyDriveIsNeutral() {
        let scale = CustomMessageWidgetNode.Drive(source: "turn", property: .scale, keyframes: [])

        #expect(scale.value(at: 0.7, period: nil) == 1)
    }
}
