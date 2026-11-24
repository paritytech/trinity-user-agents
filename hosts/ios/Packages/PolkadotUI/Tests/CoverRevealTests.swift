import Foundation
import Testing
@testable import PolkadotUI

struct CoverRevealTests {
    private let reveal = CustomMessageWidgetNode.CoverReveal(pan: "scratch", atPermille: 550, action: "scratch.reveal")
    /// The Daily Dollar card's grid: 35 x 37 cells, so 55% is 712.25 cells and 713 reach it.
    private let cells = 35 * 37

    /// What the watch sends for each look in turn, worked out before the expectation since `update` is mutating.
    private func sends(_ watch: inout CoverRevealWatch, _ looks: [CoverRevealWatch.Input]) -> [Bool] {
        looks.map { watch.update($0, cells: cells, reveal: reveal) }
    }

    @Test("The share is reached at its cell and not one cell before, as the core counts it")
    func reachedAtTheShare() {
        #expect(!reveal.reached(cleared: 712, cells: cells))
        #expect(reveal.reached(cleared: 713, cells: cells))
        let half = CustomMessageWidgetNode.CoverReveal(pan: "p", atPermille: 500, action: "r")
        #expect(!half.reached(cleared: 1, cells: 4) && half.reached(cleared: 2, cells: 4))
        let none = CustomMessageWidgetNode.CoverReveal(pan: "p", atPermille: 0, action: "r")
        let more = CustomMessageWidgetNode.CoverReveal(pan: "p", atPermille: 1_001, action: "r")
        #expect(none.reached(cleared: 0, cells: 4), "zero is reached on any lift")
        #expect(!more.reached(cleared: 4, cells: 4), "more than the whole is never reached")
    }

    @Test("A lift past the share sends the reveal once")
    func liftPastTheShare() {
        var watch = CoverRevealWatch()
        let sent = sends(&watch, [
            .init(down: true, lifts: 0, cleared: 800),
            .init(down: false, lifts: 1, cleared: 800),
            .init(down: false, lifts: 1, cleared: 900)
        ])
        #expect(sent == [false, true, false])
    }

    @Test("A lift one cell short sends nothing")
    func liftShortOfTheShare() {
        var watch = CoverRevealWatch()
        let sent = sends(&watch, [.init(down: false, lifts: 1, cleared: 712)])
        #expect(sent == [false])
    }

    @Test("Cells the worker clears in answer to the end, after the lift, count")
    func cellsAfterTheLift() {
        var watch = CoverRevealWatch()
        let sent = sends(&watch, [
            .init(down: false, lifts: 1, cleared: 700),
            .init(down: false, lifts: 1, cleared: 713)
        ])
        #expect(sent == [false, true])
    }

    @Test("Nothing is sent while the finger is down, and a new touch starts over")
    func nothingWhileDown() {
        var watch = CoverRevealWatch()
        let sent = sends(&watch, [
            .init(down: false, lifts: 1, cleared: 100),
            .init(down: true, lifts: 1, cleared: 800),
            .init(down: false, lifts: 2, cleared: 800)
        ])
        #expect(sent == [false, false, true])
    }

    @Test("A lift and a new touch between two looks send nothing until that touch lifts")
    func liftAndTouchTogether() {
        var watch = CoverRevealWatch()
        let sent = sends(&watch, [
            .init(down: true, lifts: 1, cleared: 800),
            .init(down: false, lifts: 2, cleared: 800)
        ])
        #expect(sent == [false, true])
    }

    @Test("Lifts before the cover was drawn send nothing")
    func liftsBeforeTheCover() {
        var watch = CoverRevealWatch(lifts: 3)
        let sent = sends(&watch, [
            .init(down: false, lifts: 3, cleared: 900),
            .init(down: false, lifts: 4, cleared: 900)
        ])
        #expect(sent == [false, true])
    }

    @Test("Cells outside the grid are not counted")
    func countsInsideTheGrid() {
        var cleared = CoverCells()
        cleared.insert(3)
        cleared.insert(cells)
        #expect(cleared.count(below: cells) == 1)
    }

    @Test("A touch counts its lifts and ignores a repeated state")
    @MainActor
    func touchCountsLifts() {
        let touch = PanTouch()
        touch.set(down: false)
        #expect(touch.lifts == 0 && !touch.isDown)
        touch.set(down: true)
        touch.set(down: true)
        touch.set(down: false)
        #expect(touch.lifts == 1 && !touch.isDown)
    }
}
