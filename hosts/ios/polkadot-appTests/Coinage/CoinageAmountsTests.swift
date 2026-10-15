import Foundation
import Testing

@testable import polkadot_app

@Suite("Coinage amounts")
struct CoinageAmountsTests {
    @Test("Everything ready: the total already is the ready amount")
    func allReady() {
        let amounts = CoinageAmounts(total: 45, availableNow: 45, gainingPrivacy: 0)
        #expect(amounts.hasFundsNotReady == false)
    }

    /// Pending used to be a third figure of its own. It is part of Clearing now, so a top-up
    /// arrives here rather than in a bucket the user never sees.
    @Test("Clearing funds are not ready")
    func clearing() {
        let amounts = CoinageAmounts(total: 45, availableNow: 40, gainingPrivacy: 5)
        #expect(amounts.hasFundsNotReady)
    }

    @Test("Zero holdings are treated as ready")
    func zero() {
        #expect(CoinageAmounts.zero.hasFundsNotReady == false)
    }
}
