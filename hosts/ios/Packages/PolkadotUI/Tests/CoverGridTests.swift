import CoreGraphics
import Foundation
import SwiftUI
import Testing
@testable import PolkadotUI

struct CoverCellsTests {
    @Test("A tree's bitset reads row by row, lowest bit first")
    func readsTheTreeBits() {
        let cells = CoverCells(bits: Data([0b0000_0101, 0b0000_1000]))
        #expect((0 ..< 16).filter(cells.contains) == [0, 2, 11])
        #expect(cells.count == 3)
    }

    @Test("A cell is counted once, and one outside any grid is refused")
    func insertsOnce() {
        var cells = CoverCells()
        let first = cells.insert(700)
        let again = cells.insert(700)
        let negative = cells.insert(-1)
        let outside = cells.insert(CoverCells.maxSide * CoverCells.maxSide)
        #expect(first)
        #expect(!again && !negative && !outside)
        #expect(cells.count == 1)
    }

    @Test("A union keeps the cells of both")
    func union() {
        let tree = CoverCells(bits: Data([0b11]))
        var since = CoverCells()
        since.insert(1)
        since.insert(90)
        let both = tree.union(since)
        #expect(both.count == 3)
        #expect([0, 1, 90].allSatisfy(both.contains))
    }

    @Test("Columns and rows are kept inside what the renderer allows")
    func clampsTheGrid() {
        let props = CustomMessageWidgetNode.CoverGridProps(key: "foil", columns: 0, rows: 900)
        #expect(props.columns == 1)
        #expect(props.rows == CoverCells.maxSide)
    }
}

struct CoverGridShapeTests {
    @Test("Each cleared cell is a hole centred on it, reaching its corners")
    func holeReachesTheCorners() {
        var cells = CoverCells()
        cells.insert(5) // row 1, column 1 of a 4 x 2 grid of 10 x 10 cells
        let path = CoverGridShape.holes(cells, columns: 4, rows: 2, in: CGSize(width: 40, height: 20))
        let bounds = path.boundingRect
        let radius = 50.0.squareRoot()
        #expect(abs(bounds.midX - 15) < 0.001 && abs(bounds.midY - 15) < 0.001)
        #expect(abs(bounds.width - 2 * radius) < 0.001)
        #expect(path.contains(CGPoint(x: 10.2, y: 10.2)), "a corner of the cell is inside its hole")
        #expect(!path.contains(CGPoint(x: 2, y: 2)), "an uncleared cell keeps its cover")
    }

    @Test("Two neighbouring holes leave no cover between them")
    func neighboursMerge() {
        var cells = CoverCells()
        cells.insert(0)
        cells.insert(1)
        let path = CoverGridShape.holes(cells, columns: 2, rows: 1, in: CGSize(width: 20, height: 10))
        for across in stride(from: 0.5, through: 19.5, by: 1) {
            #expect(path.contains(CGPoint(x: across, y: 5)))
        }
    }

    @Test("Right to left puts the first column at the right edge")
    func mirrors() {
        var cells = CoverCells()
        cells.insert(0)
        let path = CoverGridShape.holes(
            cells, columns: 4, rows: 1, in: CGSize(width: 40, height: 10), mirrored: true
        )
        #expect(abs(path.boundingRect.midX - 35) < 0.001)
    }

    @Test("Cells past the grid draw nothing")
    func ignoresCellsOutsideTheGrid() {
        var cells = CoverCells()
        cells.insert(8)
        #expect(CoverGridShape.holes(cells, columns: 4, rows: 2, in: CGSize(width: 40, height: 20)).isEmpty)
    }
}

@MainActor
struct CoverGridUpdateTests {
    @Test("Cleared cells reach their cover and answer the pan under way")
    func clearAnswersThePan() {
        let updates = GestureOffsets()
        let pan = updates.channel(for: "scratch")
        var answers = 0
        pan.onAnswer = { answers += 1 }
        updates.clear(key: "foil", cells: [3, 4, 4])
        #expect(updates.cover(for: "foil").cleared?.count == 2)
        #expect(updates.cover(for: "other").cleared == nil)
        #expect(pan.offset == nil, "clearing cells moves no gesture")
        #expect(answers == 1)
    }

    @Test("An answer that clears nothing new still answers")
    func emptyAnswer() {
        let updates = GestureOffsets()
        let pan = updates.channel(for: "scratch")
        var answers = 0
        pan.onAnswer = { answers += 1 }
        updates.clear(key: "foil", cells: [])
        #expect(answers == 1)
        #expect(updates.cover(for: "foil").cleared?.isEmpty == true)
    }

    @Test("A tree that arrives after cleared cells wins")
    func treeWins() {
        let updates = GestureOffsets()
        updates.clear(key: "foil", cells: [1, 2])
        updates.treeArrived()
        #expect(updates.cover(for: "foil").cleared == nil)
        updates.clear(key: "foil", cells: [9])
        #expect(updates.cover(for: "foil").cleared?.contains(9) == true)
        #expect(updates.cover(for: "foil").cleared?.contains(1) == false)
    }
}

struct LocationPanTests {
    @Test("A finger's place is measured from the top-start corner and kept inside the surface")
    func clampsToTheSurface() {
        let size = CGSize(width: 280, height: 300)
        #expect(GestureSurfaceView.inside(CGPoint(x: 40, y: 50), size, mirrored: false) == CGPoint(x: 40, y: 50))
        #expect(GestureSurfaceView.inside(CGPoint(x: -9, y: 400), size, mirrored: false) == CGPoint(x: 0, y: 300))
        #expect(GestureSurfaceView.inside(CGPoint(x: 40, y: 50), size, mirrored: true) == CGPoint(x: 240, y: 50))
    }
}
