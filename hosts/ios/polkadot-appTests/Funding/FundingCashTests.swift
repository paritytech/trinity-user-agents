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

    @Test
    func showsCentsOnlyWhenThereAreAnyAndGroupsThousands() {
        #expect(cash.figure(50) == "$50")
        #expect(cash.figure(Decimal(string: "50.2")!) == "$50.20")
        #expect(cash.figure(Decimal(string: "1000.25")!) == "$1,000.25")
        #expect(cash.label(Decimal(string: "1000.25")!) == "$1,000.25 CASH")
    }

    @Test
    func leadsWithThePeggedCurrencysSign() throws {
        let euro = try #require(Currency.supported.first { $0.code == "EUR" })
        let cash = FundingCash(symbol: "EURC", precision: 6, fiat: euro)

        #expect(cash.label(50) == "\u{20AC}50 EURC")
    }

    @Test
    func signsByDirection() {
        #expect(cash.signed(50, direction: .in) == "+$50 CASH")
        #expect(cash.signed(50, direction: .out) == "-$50 CASH")
    }
}
