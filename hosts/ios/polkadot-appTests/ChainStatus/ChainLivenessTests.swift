import Foundation
import Testing
import SubstrateSdk
@testable import polkadot_app

struct ChainLivenessTests {
    @Test("Nil liveness with zero samples")
    func nilLivenessWithZeroSamples() {
        let liveness = ChainLiveness(blockPeriod: .seconds(2))

        #expect(liveness.liveness(at: Date()) == nil)
    }

    @Test("Nil liveness with one sample")
    func nilLivenessWithOneSample() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()

        liveness.record(height: 0, at: date)

        #expect(liveness.liveness(at: date) == nil)
    }

    @Test("Full window of regular blocks reaches liveness 1")
    func fullWindowLiveness1() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        for index in 0 ... 15 {
            liveness.record(height: BlockNumber(index), at: startDate.addingTimeInterval(Double(index) * 2))
        }

        #expect(liveness.liveness(at: startDate.addingTimeInterval(30)) == 1.0)
    }

    @Test("Stalled chain has liveness 0")
    func stalledChainLiveness0() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        liveness.record(height: 0, at: startDate)
        liveness.record(height: 0, at: startDate.addingTimeInterval(30))

        #expect(liveness.liveness(at: startDate.addingTimeInterval(30)) == 0.0)
    }

    @Test("History shorter than window returns nil")
    func shorterThanWindowReturnsNil() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        liveness.record(height: 0, at: startDate)
        liveness.record(height: 1, at: startDate.addingTimeInterval(5))

        #expect(liveness.liveness(at: startDate.addingTimeInterval(5)) == nil)
    }

    @Test("Window length 30s for fast chains")
    func window30sForFastChains() {
        let liveness = ChainLiveness(blockPeriod: .seconds(2))

        #expect(liveness.slotCount == 15)
    }

    @Test("Window length 60s for slow chains")
    func window60sForSlowChains() {
        let liveness = ChainLiveness(blockPeriod: .seconds(6))

        #expect(liveness.slotCount == 10)
    }

    @Test("Clear resets to nil")
    func clearResetsToNil() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        for index in 0 ... 15 {
            liveness.record(height: BlockNumber(index), at: startDate.addingTimeInterval(Double(index) * 2))
        }

        liveness.clear()

        #expect(liveness.liveness(at: startDate.addingTimeInterval(30)) == nil)
    }

    @Test("Reorg moving head backward counts from the anchor")
    func reorgBackwardGuarded() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        liveness.record(height: 10, at: startDate)
        liveness.record(height: 9, at: startDate.addingTimeInterval(2))
        liveness.record(height: 11, at: startDate.addingTimeInterval(4))

        // Anchor is height 10 at the window start, head is 11: one block in 15 slots.
        #expect(liveness.liveness(at: startDate.addingTimeInterval(30)) == 1.0 / 15.0)
    }

    @Test("Head below the anchor height reads liveness 0 without trapping")
    func headBelowAnchorHeight() {
        // BlockNumber is unsigned, so an unguarded subtraction here traps and kills the run.
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let startDate = Date()

        liveness.record(height: 10, at: startDate)
        liveness.record(height: 5, at: startDate.addingTimeInterval(2))

        #expect(liveness.liveness(at: startDate.addingTimeInterval(30)) == 0.0)
    }

    @Test("Anchor with healthy span reaches liveness 1 immediately")
    func healthyAnchorLiveness1() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 30, chainTimeSpanSeconds: 30)

        liveness.apply(anchor, at: date)

        #expect(liveness.liveness(at: date) == 1.0)
    }

    @Test("Anchor with stalled chain span 90s gives 5/15 liveness")
    func stalledAnchorLiveness5_15() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 30, chainTimeSpanSeconds: 90)

        liveness.apply(anchor, at: date)

        #expect(liveness.liveness(at: date) == 5.0 / 15.0)
    }

    @Test("Anchor with very large span 600s gives 0 liveness")
    func largeSpanLiveness0() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 30, chainTimeSpanSeconds: 600)

        liveness.apply(anchor, at: date)

        #expect(liveness.liveness(at: date) == 0.0)
    }

    @Test("Anchor with small span less than window clamps to 1.0")
    func smallSpanClampsTo1() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 30, chainTimeSpanSeconds: 10)

        liveness.apply(anchor, at: date)

        #expect(liveness.liveness(at: date) == 1.0)
    }

    @Test("Anchor with effectiveSlots exceeding headHeight does not trap")
    func effectiveSlotsExceedsHeight() {
        // headHeight 3 with span 30 wants 15 effective slots, so the subtraction would
        // underflow BlockNumber. The chain can only prove the 3 blocks it has.
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 3, chainTimeSpanSeconds: 30)

        liveness.apply(anchor, at: date)

        #expect(liveness.liveness(at: date) == 3.0 / 15.0)
    }

    @Test("Real arrival after anchor continues window calculation")
    func realArrivalAfterAnchor() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        let anchor = ChainLivenessAnchor(headHeight: 10, chainTimeSpanSeconds: 30)

        liveness.apply(anchor, at: date)
        // Add a real block after the anchor
        liveness.record(height: 12, at: date.addingTimeInterval(4))

        // Window at date+4 is (date-26, date+4); anchor sample at date-30 is before window start
        // so it's the window anchor. Height delta = 12 - 0 = 12, liveness = min(1.0, 12/15) = 0.8
        let result = liveness.liveness(at: date.addingTimeInterval(4))
        #expect(result == 0.8)
    }

    @Test("Anchor samples are used for window calculation after real arrivals")
    func anchorSamplesUsedAfterReals() {
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()
        // Anchor with perfect health: 15 blocks over 30s, all within window
        let anchor = ChainLivenessAnchor(headHeight: 15, chainTimeSpanSeconds: 30)

        liveness.apply(anchor, at: date)

        // Query immediately after anchor: should show liveness 1.0
        #expect(liveness.liveness(at: date) == 1.0)

        // Add a real block slightly ahead
        liveness.record(height: 16, at: date.addingTimeInterval(2))

        // Still within window, still shows full liveness
        #expect(liveness.liveness(at: date.addingTimeInterval(2)) == 1.0)
    }

    @Test("Non-positive span is invalid and must not be applied")
    func nonPositiveSpanIsInvalid() {
        // A non-positive span is impossible on one lineage, so it means a reorg or a
        // load-balanced endpoint answered the two reads from different backends. Applying it
        // must leave liveness un-anchored, never report a healthy chain.
        var liveness = ChainLiveness(blockPeriod: .seconds(2))
        let date = Date()

        liveness.apply(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 0), at: date)
        #expect(liveness.liveness(at: date) == nil, "zero span must not anchor")

        liveness.apply(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: -5), at: date)
        #expect(liveness.liveness(at: date) == nil, "negative span must not anchor")
    }
}
