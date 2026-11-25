import CoreGraphics
@testable import PolkadotUI
import SwiftUI
import Testing

private typealias Mask = CustomMessageWidgetNode.Mask

/// A stamped brush, checked against the core's own cases (`MaskBrush::stamps`, `cleared_share`) in points.
struct MaskBrushTests {
    private func brush(
        _ tip: Mask.Brush.Tip = .ellipse,
        aspect: CGFloat = 0.25,
        degrees: CGFloat = 0,
        spacing: CGFloat = 1
    )
        -> Mask.Brush {
        Mask.Brush(tip: tip, aspect: aspect, angle: degrees * .pi / 180, spacing: spacing)
    }

    @Test("Stamps fall every spacing along the line, carried across corners, with one more on a bare last point")
    func stampsAlongTheLine() {
        let straight = [CGPoint.zero, CGPoint(x: 10, y: 0)]
        #expect(brush().stamps(width: 4, along: straight).map(\.x) == [0, 4, 8, 10])
        #expect(brush().stamps(width: 5, along: straight).map(\.x) == [0, 5, 10])
        let corner = [CGPoint.zero, CGPoint(x: 3, y: 0), CGPoint(x: 3, y: 3)]
        #expect(brush().stamps(width: 4, along: corner) == [.zero, CGPoint(x: 3, y: 1), CGPoint(x: 3, y: 3)])
        #expect(brush().stamps(width: 4, along: [CGPoint(x: 0.07, y: 0.09)]) == [CGPoint(x: 0.07, y: 0.09)])
        #expect(brush().stamps(width: 4, along: []).isEmpty)
        #expect(brush(spacing: 0).stamps(width: 10, along: straight).count == 21, "a spacing of 0 is a twentieth")
    }

    @Test("A stamped line has at most the core's most stamps, never closer than half a point, from end to end")
    func stampsAreBounded() throws {
        let thin = brush(spacing: 0.05)
        let long = thin.stamps(width: 0, along: [.zero, CGPoint(x: 100_000, y: 0)])
        #expect(long.count == Mask.Brush.mostStamps)
        let widest = [CGPoint(x: CGFloat(Int32.min) / 100, y: 0), CGPoint(x: CGFloat(Int32.max) / 100, y: 0)]
        let stamps = thin.stamps(width: 0, along: widest)
        #expect(stamps.count == Mask.Brush.mostStamps)
        #expect(stamps.first == widest[0])
        let end = try #require(stamps.last)
        #expect(abs(end.x - widest[1].x) < 0.01 && end.y == 0, "the last stamp is on the last point")
        #expect(thin.stamps(width: 0, along: [.zero, CGPoint(x: 2, y: 0)]).count == 5, "every half a point")
        let zigzag = (0 ..< 10_000).map { CGPoint(x: $0 % 2 == 0 ? 0 : 1_000, y: CGFloat($0) / 100) }
        #expect(thin.stamps(width: 0, along: zigzag).count <= Mask.Brush.mostStamps)
    }

    @Test("The aspect and the spacing are clamped as the core clamps them")
    func clamps() {
        #expect(brush(aspect: 0).aspect == 0.001)
        #expect(brush(aspect: 2).aspect == 1)
        #expect(brush(spacing: 9).spacing == 4)
    }

    @Test("A stamp covers the samples its turned tip covers")
    func coverage() {
        let one = { (tip: Mask.Brush.Tip, degrees: CGFloat, width: CGFloat, centre: CGPoint, size: CGSize) in
            let shape = Mask.Shape.stamped(width: width, brush: brush(tip, degrees: degrees), points: [centre])
            let count = MaskCoverage.count([shape], in: size)
            return [count.covered, count.total]
        }
        let row = CGSize(width: 10, height: 2)
        #expect(one(.ellipse, 0, 4, CGPoint(x: 5, y: 1), row) == [3, 5], "lying flat it covers three samples")
        #expect(one(.ellipse, 90, 4, CGPoint(x: 5, y: 1), row) == [1, 5], "upright only the one under it")
        let rows = CGSize(width: 10, height: 4)
        #expect(one(.ellipse, 0, 8, CGPoint(x: 5, y: 2), rows) == [2, 10], "an ellipse reaches both rows at its middle")
        #expect(one(.capsule, 0, 8, CGPoint(x: 5, y: 2), rows) == [6, 10], "a capsule along its straight sides")
    }

    @Test("A followed pan with a brush adds its strokes as stamped lines")
    func followedBrush() {
        let nib = brush(.capsule, degrees: -30, spacing: 0.25)
        let mask = Mask(key: "veil", follow: .init(pan: "pan", width: 40, brush: nib))
        let picture = MaskPicture(
            mask,
            strokes: [[CGPoint(x: 1, y: 2), CGPoint(x: 3, y: 4)]],
            place: CGPoint(x: 10, y: 0)
        )
        #expect(picture.shapes == [.stamped(
            width: 40,
            brush: nib,
            points: [CGPoint(x: 11, y: 2), CGPoint(x: 13, y: 4)]
        )])
    }

    @Test("Stamps are filled turned by the tip's angle, the other way in a right-to-left node")
    func drawing() {
        let size = CGSize(width: 100, height: 100)
        let shape = Mask.Shape.stamped(
            width: 20,
            brush: brush(aspect: 0.1, degrees: 45),
            points: [CGPoint(x: 30, y: 50)]
        )
        let leftToRight = MaskDrawing.fills([shape], in: size, mirrored: false)
        #expect(leftToRight.contains(CGPoint(x: 36, y: 56)), "along the tip, down and to the right")
        #expect(!leftToRight.contains(CGPoint(x: 36, y: 44)))
        let rightToLeft = MaskDrawing.fills([shape], in: size, mirrored: true)
        #expect(rightToLeft.contains(CGPoint(x: 64, y: 56)), "counted from the right edge, turned the other way")
        #expect(!rightToLeft.contains(CGPoint(x: 76, y: 56)))
        #expect(MaskDrawing.lines([shape], in: size, mirrored: false).isEmpty, "a stamped line is not a round line")
    }
}
