import CoreGraphics
@testable import PolkadotUI
import SwiftUI
import Testing

private typealias Mask = CustomMessageWidgetNode.Mask

struct MaskDrawingTests {
    private let size = CGSize(width: 100, height: 50)

    @Test("Circles, the whole node and one-point strokes are filled; longer strokes are lines")
    func fillsAndLines() {
        let shapes: [Mask.Shape] = [
            .circle(center: CGPoint(x: 10, y: 10), radius: 5),
            .stroke(width: 8, points: [CGPoint(x: 50, y: 25)]),
            .stroke(width: 8, points: [CGPoint(x: 0, y: 0), CGPoint(x: 20, y: 0)]),
            .stroke(width: 4, points: [CGPoint(x: 0, y: 40), CGPoint(x: 90, y: 40)]),
            .stroke(width: 8, points: [CGPoint(x: 60, y: 5), CGPoint(x: 70, y: 5)])
        ]
        let fills = MaskDrawing.fills(shapes, in: size, mirrored: false)
        #expect(fills.contains(CGPoint(x: 10, y: 10)))
        #expect(fills.contains(CGPoint(x: 53, y: 25)), "a one-point stroke is a dot half its width round")
        #expect(!fills.contains(CGPoint(x: 10, y: 0)), "a line is not filled")
        let lines = MaskDrawing.lines(shapes, in: size, mirrored: false)
        #expect(lines.map(\.width) == [8, 4], "one path per width, in the order they first appear")
        #expect(lines[0].path.boundingRect == CGRect(x: 0, y: 0, width: 70, height: 5))
        #expect(MaskDrawing.fills([.all], in: size, mirrored: false).boundingRect == CGRect(origin: .zero, size: size))
    }

    @Test("Right to left, shapes count from the right edge")
    func mirrored() {
        let fills = MaskDrawing.fills([.circle(center: CGPoint(x: 10, y: 10), radius: 2)], in: size, mirrored: true)
        #expect(fills.contains(CGPoint(x: 90, y: 10)))
        #expect(!fills.contains(CGPoint(x: 10, y: 10)))
    }

    @Test("Empty and zero-sized shapes draw nothing")
    func nothing() {
        let shapes: [Mask.Shape] = [
            .circle(center: .zero, radius: 0), .stroke(width: 0, points: [.zero, CGPoint(x: 5, y: 5)]),
            .stroke(width: 5, points: [])
        ]
        #expect(MaskDrawing.fills(shapes, in: size, mirrored: false).isEmpty)
        #expect(MaskDrawing.lines(shapes, in: size, mirrored: false).isEmpty)
    }

    @Test("The fade follows the CSS ease curve from 0 to 1")
    func ease() {
        let fade = MaskFade(from: MaskPicture(mode: .hide, shapes: []), start: 10, duration: 0.35)
        #expect(fade.progress(at: 9) == 0)
        #expect(fade.progress(at: 10.35) == 1)
        #expect(abs(fade.progress(at: 10.175) - 0.8024) < 0.01, "ease is about 80% of the way at half time")
        #expect(!fade.isDone(at: 10.3) && fade.isDone(at: 10.36))
    }
}

struct MaskPictureTests {
    @Test("The tree's shapes, then those added, then the followed strokes moved into the node's frame")
    func order() {
        let mask = Mask(
            key: "veil", shapes: [.all], follow: .init(pan: "pan", width: 52)
        )
        let picture = MaskPicture(
            mask,
            added: [.circle(center: .zero, radius: 1)],
            strokes: [[CGPoint(x: 1, y: 2), CGPoint(x: 3, y: 4)], []],
            place: CGPoint(x: 10, y: 20)
        )
        #expect(picture.shapes == [
            .all, .circle(center: .zero, radius: 1),
            .stroke(width: 52, points: [CGPoint(x: 11, y: 22), CGPoint(x: 13, y: 24)])
        ])
    }

    @Test("A mask that follows no pan draws no strokes; right to left, strokes count from the right edge")
    func followOnly() {
        let strokes = [[CGPoint(x: 1, y: 2)]]
        #expect(MaskPicture(Mask(key: "k"), strokes: strokes).shapes.isEmpty)
        let mirrored = MaskPicture(
            Mask(key: "k", follow: .init(pan: "p", width: 4)), strokes: strokes, mirroredWidth: 100
        )
        #expect(mirrored.shapes == [.stroke(width: 4, points: [CGPoint(x: 99, y: 2)])])
    }
}

@MainActor
struct MaskChannelTests {
    private let veil = Mask(key: "veil", follow: .init(pan: "pan", width: 52), fade: 0.35)

    @Test("Shapes add up until a tree, and an empty extension changes nothing")
    func extend() {
        let channel = MaskChannel()
        channel.extend([.all])
        channel.extend([])
        channel.extend([.circle(center: .zero, radius: 1)])
        #expect(channel.added == [.all, .circle(center: .zero, radius: 1)])
        channel.treeArrived(next: veil)
        #expect(channel.added.isEmpty)
    }

    @Test("A tree that changes a fading mask fades from what was drawn, strokes included")
    func fadesFromWhatWasDrawn() {
        let channel = MaskChannel()
        let trail = PanTrail()
        trail.begin(at: CGPoint(x: 5, y: 5))
        trail.extend(to: CGPoint(x: 9, y: 5))
        channel.source = MaskSource(mask: veil, trail: trail, place: .zero)
        channel.extend([.circle(center: .zero, radius: 3)])
        channel.treeArrived(next: Mask(key: "veil", shapes: [.all], fade: 0.35), now: 100)
        #expect(channel.fade?.from.shapes == [
            .circle(center: .zero, radius: 3), .stroke(width: 52, points: [CGPoint(x: 5, y: 5), CGPoint(x: 9, y: 5)])
        ])
        #expect(channel.fade?.start == 100)
        channel.endFade(at: 100.2)
        #expect(channel.fade != nil)
        channel.endFade(at: 100.36)
        #expect(channel.fade == nil)
    }

    @Test("No fade without a duration, nor when the tree draws what was drawn; a running fade goes on")
    func noFade() {
        let channel = MaskChannel()
        channel.source = MaskSource(mask: veil, trail: nil, place: .zero)
        channel.treeArrived(next: Mask(key: "veil", shapes: [.all]), now: 1)
        #expect(channel.fade == nil)

        channel.treeArrived(next: Mask(key: "veil", shapes: [.all], fade: 0.35), now: 2)
        let running = channel.fade
        #expect(running != nil)
        channel.source = MaskSource(mask: Mask(key: "veil", shapes: [.all], fade: 0.35), trail: nil, place: .zero)
        channel.treeArrived(next: Mask(key: "veil", shapes: [.all], fade: 0.35), now: 2.1)
        #expect(channel.fade == running, "the same mask again leaves the fade running")
        channel.treeArrived(next: nil, now: 2.2)
        #expect(channel.fade == nil, "a tree without the mask drops its fade")
    }

    @Test("A mask never drawn has nothing to fade from")
    func firstTree() {
        let channel = MaskChannel()
        channel.treeArrived(next: Mask(key: "veil", shapes: [.all], fade: 0.35))
        #expect(channel.fade == nil)
    }

    @Test("A tree's keyed masks are found at any depth, the first of a key winning")
    func keyedMasks() {
        let inner = CustomMessageWidgetNode(content: .spacer, modifiers: .init(mask: Mask(key: "a", shapes: [.all])))
        let other = CustomMessageWidgetNode(content: .spacer, modifiers: .init(mask: Mask(key: "a")))
        let unkeyed = CustomMessageWidgetNode(content: .spacer, modifiers: .init(mask: Mask()))
        let tree = CustomMessageWidgetNode(
            content: .column(.init(), children: [
                CustomMessageWidgetNode(content: .box(.init(), children: [inner]), modifiers: .empty), other, unkeyed
            ]),
            modifiers: .init(mask: Mask(key: "b"))
        )
        #expect(tree.keyedMasks == ["a": Mask(key: "a", shapes: [.all]), "b": Mask(key: "b")])
    }
}

@MainActor
struct PanTrailTests {
    @Test("Strokes begin at touch-down, skip repeated places, and a tree drops them")
    func strokes() {
        let trail = PanTrail()
        trail.extend(to: CGPoint(x: 1, y: 1))
        trail.extend(to: CGPoint(x: 1, y: 1))
        trail.begin(at: CGPoint(x: 5, y: 5))
        trail.extend(to: CGPoint(x: 6, y: 5))
        #expect(trail.strokes == [[CGPoint(x: 1, y: 1)], [CGPoint(x: 5, y: 5), CGPoint(x: 6, y: 5)]])
        trail.reset()
        #expect(trail.isEmpty)
    }
}
