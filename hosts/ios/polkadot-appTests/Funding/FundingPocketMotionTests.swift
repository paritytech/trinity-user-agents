import CoreGraphics
import Foundation
@testable import polkadot_app
import Testing
import TrUAPIHost

struct FundingPocketMotionTests {
    @Test
    func theCardCollapsesOverOneHundredFiftyPoints() {
        let rest = PurseMotion.Collapse.restTop

        #expect(PurseMotion.Collapse.progress(cardTop: rest + 40) == 0)
        #expect(PurseMotion.Collapse.progress(cardTop: rest) == 0)
        #expect(PurseMotion.Collapse.progress(cardTop: rest - 75) == 0.5)
        #expect(PurseMotion.Collapse.progress(cardTop: rest - 150) == 1)
        #expect(PurseMotion.Collapse.progress(cardTop: rest - 400) == 1)
    }

    @Test
    func theCardShrinksToItsDockedWidthAlongASmoothstep() {
        #expect(PurseMotion.smoothstep(0) == 0)
        #expect(PurseMotion.smoothstep(0.5) == 0.5)
        #expect(PurseMotion.smoothstep(1) == 1)
        #expect(PurseMotion.smoothstep(0.25) == 0.156_25)
        #expect(PurseMotion.Collapse.cardScale(0) == 1)
        #expect(abs(PurseMotion.Collapse.cardScale(1) - 60.0 / 366.0) < 0.000_001)
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
    func theCountHintsEveryTenSecondsForAboutTwo() {
        #expect(!PurseMotion.showsHint(elapsed: 0))
        #expect(!PurseMotion.showsHint(elapsed: 7.1))
        #expect(PurseMotion.showsHint(elapsed: 7.2))
        #expect(PurseMotion.showsHint(elapsed: 9.3))
        #expect(!PurseMotion.showsHint(elapsed: 9.4))
        #expect(PurseMotion.showsHint(elapsed: 18))
    }

    @Test
    func aSessionLeavingInProgressArrivesForAWhile() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        let finished = FundingActivityCenter.finished(
            before: ["a", "b", "c"],
            after: ["c"],
            known: ["old": now - 10, "recent": now - 1],
            now: now
        )

        #expect(finished == ["a": now, "b": now, "recent": now - 1])
    }

    @Test
    func theArrowPointsIntoTheCardForValueComingIn() {
        let box = CGRect(x: 0, y: 0, width: 22, height: 22)
        let incoming = FundingArrowGeometry.ends(in: box, direction: .in)
        let outgoing = FundingArrowGeometry.ends(in: box, direction: .out)

        #expect(incoming.tip.x < incoming.from.x)
        #expect(incoming.tip.y > incoming.from.y)
        #expect(outgoing.tip == incoming.from)
        #expect(outgoing.from == incoming.tip)
    }

    @Test
    func theClearingStripesScrollOncePerPeriod() {
        let start = Date(timeIntervalSinceReferenceDate: 0)

        #expect(CoinageSplitBar.phase(at: start) == 0)
        #expect(abs(CoinageSplitBar.phase(at: start + 0.35) - 0.5) < 0.000_001)
        #expect(abs(CoinageSplitBar.phase(at: start + 0.7)) < 0.000_001)
    }
}
