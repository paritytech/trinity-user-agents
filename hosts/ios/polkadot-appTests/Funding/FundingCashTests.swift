import Foundation
import Testing
@testable import polkadot_app

/// How the funding screens turn the balance's smallest units into what they
/// show and what they ask for.
struct FundingCashTests {
    private let cash = FundingCash(symbol: "CASH", precision: 6)

    @Test
    func readsUnitsAsDollars() {
        #expect(cash.decimal("50200000") == Decimal(string: "50.2"))
        #expect(cash.decimal("not a number") == nil)
        #expect(cash.decimal(nil as String?) == nil)
    }

    @Test
    func neverAsksForMoreThanTheUserTyped() {
        #expect(cash.units(Decimal(string: "50.2")!) == "50200000")
        #expect(cash.units(Decimal(string: "0.0000019")!) == "1")
        #expect(cash.units(-5) == nil)
    }
}
