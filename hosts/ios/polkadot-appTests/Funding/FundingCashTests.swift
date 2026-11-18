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
    func signsByDirection() {
        #expect(cash.signed(50, direction: .in) == "+$50 CASH")
        #expect(cash.signed(50, direction: .out) == "-$50 CASH")
    }

    @Test
    func setsACurrencyAsAPriceAndAnyOtherAssetWithItsCode() {
        #expect(AssetBalanceDisplayInfo.from(assetCode: "eur", decimals: 2).amount(units: "2510") == "\u{20AC}25.10")
        #expect(AssetBalanceDisplayInfo.from(assetCode: "EUR", decimals: 2).amount(units: "2500") == "\u{20AC}25")
        #expect(AssetBalanceDisplayInfo.from(assetCode: "USDT", decimals: 6).amount(units: "51800000") == "51.8 USDT")
        #expect(AssetBalanceDisplayInfo.from(assetCode: "USDT", decimals: 6).amount(units: "x") == nil)
    }
}
