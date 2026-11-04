import CoreGraphics
import Foundation
@testable import polkadot_app
import Testing

/// The Purse's card collapsing into the header, its sticky bar, and the
/// Total panel's split bar.
struct PurseMotionTests {
    @Test
    func theCardCollapsesOverOneHundredFiftyPointsOfScroll() {
        #expect(PurseMotion.Collapse.progress(scrollOffset: -40) == 0)
        #expect(PurseMotion.Collapse.progress(scrollOffset: 0) == 0)
        #expect(PurseMotion.Collapse.progress(scrollOffset: 75) == 0.5)
        #expect(PurseMotion.Collapse.progress(scrollOffset: 150) == 1)
        #expect(PurseMotion.Collapse.progress(scrollOffset: 400) == 1)
    }

    @Test
    func withoutScrollGeometryTheOffsetIsReadFromTheCard() {
        let rest = PurseMotion.Collapse.restTop

        #expect(PurseMotion.Collapse.scrollOffset(cardTop: rest + 40) == 0)
        #expect(PurseMotion.Collapse.scrollOffset(cardTop: rest - 75) == 75)
    }

    @Test
    func theEasingIsASmoothstep() {
        #expect(PurseMotion.smoothstep(0) == 0)
        #expect(PurseMotion.smoothstep(0.5) == 0.5)
        #expect(PurseMotion.smoothstep(1) == 1)
        #expect(PurseMotion.smoothstep(0.25) == 0.156_25)
    }

    /// A 354 by 231 card resting at (24, 120), docking at a bar centred on (201, 84).
    private let rest = CGRect(x: 24, y: 120, width: 354, height: 231)
    private let dock = CGPoint(x: 201, y: 84)

    @Test
    func atRestTheCardIsWhereThePageHasIt() {
        #expect(PurseMotion.Collapse.floatingFrame(rest: rest, scrollOffset: 0, dock: dock) == rest)
    }

    @Test
    func dockedTheCardIsTheThumbnailCentredInTheHeader() {
        for offset: CGFloat in [150, 200, 900] {
            let frame = PurseMotion.Collapse.floatingFrame(rest: rest, scrollOffset: offset, dock: dock)

            #expect(abs(frame.width - 60) < 0.000_1)
            #expect(abs(frame.height - 231 * 60 / 354) < 0.000_1)
            #expect(abs(frame.midX - dock.x) < 0.000_1)
            #expect(abs(frame.midY - dock.y) < 0.000_1)
        }
    }

    @Test
    func halfwayTheCardHasRiddenUpAndShrunkHalfway() {
        let frame = PurseMotion.Collapse.floatingFrame(rest: rest, scrollOffset: 75, dock: dock)
        let riding = CGPoint(x: rest.midX, y: rest.midY - 75)

        #expect(abs(frame.width - (354 + 60) / 2) < 0.000_1)
        #expect(abs(frame.midX - (riding.x + dock.x) / 2) < 0.000_1)
        #expect(abs(frame.midY - (riding.y + dock.y) / 2) < 0.000_1)
    }

    @Test
    func theBalanceAndTitleFadeFasterThanTheCardShrinks() {
        #expect(PurseMotion.Collapse.balanceOpacity(0) == 1)
        #expect(abs(PurseMotion.Collapse.balanceOpacity(0.5) - 0.1) < 0.000_001)
        #expect(PurseMotion.Collapse.balanceOpacity(1) == 0)
        #expect(abs(PurseMotion.Collapse.titleOpacity(0.2) - 0.5) < 0.000_001)
        #expect(PurseMotion.Collapse.titleOpacity(0.4) == 0)
    }

    @Test
    func theHeaderThumbnailTakesOverFromTheCard() {
        #expect(PurseMotion.Collapse.thumbnailOpacity(0.6) == 0)
        #expect(PurseMotion.Collapse.thumbnailOpacity(1) == 1)
        #expect(PurseMotion.Collapse.cardOpacity(0) == 1)
        #expect(PurseMotion.Collapse.cardOpacity(1) == 0)
        #expect(abs(PurseMotion.Collapse.cardOpacity(0.8) + PurseMotion.Collapse.thumbnailOpacity(0.8) - 1) < 0.000_001)
    }

    @Test
    func theStickyBarShowsOnceTheCardHasGone() {
        let card = PurseScroll.Card(bottom: 20, balance: 424.13)

        #expect(PurseScroll.bar(card: PurseScroll.Card(bottom: 120, balance: 1)) == nil)
        #expect(PurseScroll.bar(card: card) == PurseScroll.Bar(balance: 424.13))
    }

    @Test
    func theClearingShareNeverDisappearsWhileAnythingClears() {
        #expect(CoinageSplitBar.share(clearing: 0, total: 100) == 0)
        #expect(CoinageSplitBar.share(clearing: 1, total: 100) == 0.06)
        #expect(CoinageSplitBar.share(clearing: 50, total: 100) == 0.5)
        #expect(CoinageSplitBar.share(clearing: 5, total: 0) == 0)
    }

    @Test
    func theClearingStripesScrollOncePerPeriod() {
        let start = Date(timeIntervalSinceReferenceDate: 0)

        #expect(CoinageSplitBar.phase(at: start) == 0)
        #expect(abs(CoinageSplitBar.phase(at: start + 0.35) - 0.5) < 0.000_001)
        #expect(abs(CoinageSplitBar.phase(at: start + 0.7)) < 0.000_001)
    }
}
