import CoreGraphics
import Foundation
@testable import polkadot_app
import Testing
import TrUAPIHost

/// A finished session arriving in the history, and its row's arrow.
struct FundingActivityMotionTests {
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
}
