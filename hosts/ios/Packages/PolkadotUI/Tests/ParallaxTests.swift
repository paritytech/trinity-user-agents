import CoreGraphics
import Foundation
@testable import PolkadotUI
import Testing

private typealias Parallax = CustomMessageWidgetNode.Parallax

/// Parallax: the core's offset rule, the rest attitude, tilt read from gravity, and the pan's drawn offset.
struct ParallaxTests {
    /// The core's test layer, in points: 24 at the clamp of 20 degrees, settling over 3 s, behind at -0.3.
    private static let back = Parallax(depth: -0.3, tilt: .init(travel: 24, maxTilt: 20, settle: 3), pan: "card")

    private static func close(_ size: CGSize, _ width: CGFloat, _ height: CGFloat) -> Bool {
        abs(size.width - width) < 1e-9 && abs(size.height - height) < 1e-9
    }

    @Test("Parallax scales the tilt and the pan by its depth, as the core's Parallax::offset")
    func offset() {
        let front = Parallax(depth: 1, tilt: Self.back.tilt, pan: "card")
        #expect(Self.close(front.offset(tilt: CGVector(dx: 10, dy: -5), panOffset: .zero), 12, -6))
        #expect(
            Self.close(front.offset(tilt: CGVector(dx: 45, dy: -90), panOffset: .zero), 24, -24),
            "held at the clamp"
        )
        #expect(Self.close(front.offset(tilt: CGVector(dx: 10, dy: 0), panOffset: CGSize(width: 3, height: 1)), 15, 1))
        #expect(Self.close(
            Self.back.offset(tilt: CGVector(dx: 10, dy: 0), panOffset: CGSize(width: 3, height: 1)),
            -4.5,
            -0.3
        ))
        let panOnly = Parallax(depth: 1, pan: "card")
        #expect(Self.close(
            panOnly.offset(tilt: CGVector(dx: 10, dy: 10), panOffset: CGSize(width: 3, height: 1)),
            3,
            1
        ))
        let flat = Parallax(depth: 1, tilt: .init(travel: 24, maxTilt: 0, settle: 0), pan: "card")
        #expect(
            Self.close(flat.offset(tilt: CGVector(dx: 10, dy: 10), panOffset: .zero), 0, 0),
            "zero tilt never moves"
        )
        let tiltOnly = Parallax(depth: 1, tilt: Self.back.tilt)
        #expect(
            Self.close(tiltOnly.offset(tilt: .zero, panOffset: CGSize(width: 3, height: 1)), 0, 0),
            "no pan, no follow"
        )
    }

    @Test("Tilt is measured from the attitude at appearance, which follows the device's on its time constant")
    func rest() {
        var rest = TiltRest()
        #expect(
            rest.tilt(current: CGVector(dx: 30, dy: 10), elapsed: 0, settle: 3) == .zero,
            "the first reading is rest"
        )
        let tilted = rest.tilt(current: CGVector(dx: 40, dy: 10), elapsed: 0, settle: 3)
        #expect(abs(tilted.dx - 10) < 1e-9 && tilted.dy == 0)
        let later = rest.tilt(current: CGVector(dx: 40, dy: 10), elapsed: 3, settle: 3)
        #expect(abs(later.dx - 10 * exp(-1)) < 1e-9, "one time constant closes all but 1/e")
        var kept = TiltRest()
        _ = kept.tilt(current: .zero, elapsed: 0, settle: 0)
        #expect(kept.tilt(current: CGVector(dx: 5, dy: 0), elapsed: 100, settle: 0).dx == 5, "zero keeps the attitude")
    }

    @Test("Tilt is read from gravity: lowering the right edge or the top edge reads positive")
    func gravity() {
        let flat = TiltRest.degrees(gravity: (0, 0, -1))
        #expect(abs(flat.dx) < 1e-9 && abs(flat.dy) < 1e-9)
        let rightDown = TiltRest.degrees(gravity: (sin(.pi / 6), 0, -cos(.pi / 6)))
        #expect(abs(rightDown.dx - 30) < 1e-9 && abs(rightDown.dy) < 1e-9)
        let topDown = TiltRest.degrees(gravity: (0, sin(.pi / 9), -cos(.pi / 9)))
        #expect(abs(topDown.dx) < 1e-9 && abs(topDown.dy - 20) < 1e-9)
        let upright = TiltRest.degrees(gravity: (0, -1, 0))
        #expect(abs(upright.dy + 90) < 1e-9, "held upright, the top edge is a quarter turn up from flat")
    }

    @Test("The pan's part is where the keyed gesture's children are drawn: the host's drag and the worker's offset")
    @MainActor
    func panOffset() {
        let offsets = GestureOffsets()
        offsets.place(for: "card").dragOffset = CGPoint(x: 30, y: 4)
        offsets.channel(for: "card").receive(CGPoint(x: -10, y: 0))
        #expect(NodeParallaxModifier.panOffset(key: "card", offsets: offsets) == CGSize(width: 20, height: 4))
        #expect(NodeParallaxModifier.panOffset(key: "other", offsets: offsets) == .zero, "a key naming no gesture")
        #expect(NodeParallaxModifier.panOffset(key: nil, offsets: offsets) == .zero)
    }
}
