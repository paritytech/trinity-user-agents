import CoreGraphics
import Foundation
import SwiftUI
import Testing
@testable import PolkadotUI

/// The cells a worker clears for a straight stroke: every cell whose centre lies within `reach` of the segment, as the
/// Daily Dollar card's worker clears them.
private func cellsUnder(
    _ a: CGPoint, _ b: CGPoint, reach: CGFloat, columns: Int, rows: Int, size: CGSize
) -> CoverCells {
    var cells = CoverCells()
    let width = size.width / CGFloat(columns)
    let height = size.height / CGFloat(rows)
    let dx = b.x - a.x
    let dy = b.y - a.y
    let length2 = dx * dx + dy * dy
    for row in 0 ..< rows {
        for column in 0 ..< columns {
            let centre = CGPoint(x: (CGFloat(column) + 0.5) * width, y: (CGFloat(row) + 0.5) * height)
            let t = length2 == 0 ? 0 : min(max(((centre.x - a.x) * dx + (centre.y - a.y) * dy) / length2, 0), 1)
            let px = a.x + t * dx - centre.x
            let py = a.y + t * dy - centre.y
            if px * px + py * py <= reach * reach { cells.insert(row * columns + column) }
        }
    }
    return cells
}

struct CoverBrushTests {
    private let size = CGSize(width: 290, height: 380)
    private let columns = 36
    private let rows = 47
    private let brush = CustomMessageWidgetNode.CoverBrush(pan: "pan", width: 52)

    private func brushed(_ strokes: [[CGPoint]], origin: CGPoint = .zero) -> Path {
        CoverGridShape.brushStrokes(strokes, origin: origin).strokedPath(CoverGridShape.brushStyle(brush))
    }

    @Test("A stroke is a line the brush's width with round ends, as the existing card draws it")
    func roundEndedLine() {
        let path = brushed([[CGPoint(x: 60, y: 100), CGPoint(x: 230, y: 100)]])
        #expect(path.contains(CGPoint(x: 145, y: 125)), "half the width from the line is cut")
        #expect(!path.contains(CGPoint(x: 145, y: 127)), "past half the width the cover stays")
        #expect(path.contains(CGPoint(x: 35, y: 100)), "the end is a half circle the brush's width")
        #expect(path.contains(CGPoint(x: 60 - 18, y: 100 - 18)), "inside the round end")
        #expect(!path.contains(CGPoint(x: 60 - 20, y: 100 - 20)), "a square end's corner keeps its cover")
    }

    @Test("Two strokes 54 points apart leave a strip of cover between them")
    func stripBetweenStrokes() {
        let path = brushed([
            [CGPoint(x: 35, y: 55), CGPoint(x: 255, y: 55)],
            [CGPoint(x: 255, y: 109), CGPoint(x: 35, y: 109)],
        ])
        #expect(path.contains(CGPoint(x: 145, y: 80.5)) && path.contains(CGPoint(x: 145, y: 83.5)))
        #expect(!path.contains(CGPoint(x: 145, y: 82)), "the 2 point strip between them stays")
    }

    @Test("A touch with no move is a dot the brush's width")
    func dot() {
        let path = brushed([[CGPoint(x: 100, y: 100)]])
        #expect(path.contains(CGPoint(x: 100, y: 125)) && path.contains(CGPoint(x: 75.2, y: 100)))
        #expect(!path.contains(CGPoint(x: 100, y: 127)))
    }

    @Test("Strokes are moved from the pan's frame into the cover's")
    func movedIntoTheCover() {
        let path = brushed([[CGPoint(x: 60, y: 100), CGPoint(x: 80, y: 100)]], origin: CGPoint(x: 10, y: 20))
        #expect(path.contains(CGPoint(x: 60, y: 80 + 25)))
        #expect(!path.contains(CGPoint(x: 60, y: 100 + 25)))
    }

    @Test("The gate holds every point of a stroke whose worker cleared the cells within half the brush")
    func gateCoversTheStroke() {
        let a = CGPoint(x: 40, y: 70)
        let b = CGPoint(x: 250, y: 160)
        let cells = cellsUnder(a, b, reach: 26, columns: columns, rows: rows, size: size)
        let gate = CoverGridShape.brushGate(cells, columns: columns, rows: rows, in: size)
        let stroke = brushed([[a, b]])
        var inside = 0
        for x in stride(from: 0.5, to: size.width, by: 1.5) {
            for y in stride(from: 0.5, to: size.height, by: 1.5) {
                let point = CGPoint(x: x, y: y)
                guard stroke.contains(point) else { continue }
                inside += 1
                #expect(gate.contains(point), "stroke point \(point) outside the gate")
            }
        }
        #expect(inside > 1_000)
    }

    @Test("The gate cuts nothing more than a cell's diagonal from a cleared cell")
    func gateStaysNearTheCells() {
        var cells = CoverCells()
        cells.insert(10 * columns + 10)
        let gate = CoverGridShape.brushGate(cells, columns: columns, rows: rows, in: size)
        let width = size.width / CGFloat(columns)
        let height = size.height / CGFloat(rows)
        let centre = CGPoint(x: 10.5 * width, y: 10.5 * height)
        let diagonal = (width * width + height * height).squareRoot()
        #expect(gate.contains(CGPoint(x: centre.x + diagonal - 0.5, y: centre.y)))
        #expect(!gate.contains(CGPoint(x: centre.x + diagonal + 0.5, y: centre.y)))
    }

    @Test("A gate follows a right-to-left layout's mirrored columns")
    func gateMirrors() {
        var cells = CoverCells()
        cells.insert(0)
        let gate = CoverGridShape.brushGate(cells, columns: columns, rows: rows, in: size, mirrored: true)
        #expect(gate.contains(CGPoint(x: size.width - 4, y: 4)))
        #expect(!gate.contains(CGPoint(x: 4, y: 4)))
    }
}

@MainActor
struct PanTrailTests {
    @Test("Each stroke starts where the finger touched down and follows it")
    func strokes() {
        let trail = PanTrail()
        trail.begin(at: CGPoint(x: 1, y: 1))
        trail.extend(to: CGPoint(x: 5, y: 1))
        trail.extend(to: CGPoint(x: 5, y: 1))
        trail.begin(at: CGPoint(x: 9, y: 9))
        #expect(trail.strokes == [[CGPoint(x: 1, y: 1), CGPoint(x: 5, y: 1)], [CGPoint(x: 9, y: 9)]])
    }

    @Test("A place with no stroke begun starts one")
    func extendStarts() {
        let trail = PanTrail()
        trail.extend(to: CGPoint(x: 3, y: 4))
        #expect(trail.strokes == [[CGPoint(x: 3, y: 4)]])
    }

    @Test("A tree drops the strokes with the cells cleared since")
    func treeDropsStrokes() {
        let updates = GestureOffsets()
        updates.trail(for: "pan").begin(at: .zero)
        updates.clear(key: "foil", cells: [1])
        updates.treeArrived()
        #expect(updates.trail(for: "pan").isEmpty)
        #expect(updates.cover(for: "foil").cleared == nil)
        #expect(updates.trail(for: "pan") === updates.trail(for: "pan"), "one trail a key")
    }
}
