import Testing
import Foundation
@testable import Coinage

/// `CoinageBalance.available` is the only place the three buckets collapse into a spendable figure, and it
/// branches on `gainingPrivacy.canSpendWithConfirmation` — the exact bit that separates a strategy that will
/// release held-back privacy money on confirmation (balanced) from one that never will (maxPrivacy). These
/// pin both branches and confirm `total` ignores the flag entirely.
@Suite("CoinageBalance Tests")
struct CoinageBalanceTests {
    private func makeBalance(canSpendWithConfirmation: Bool) -> CoinageBalance {
        CoinageBalance(
            availablePrivate: 100,
            gainingPrivacy: .init(amount: 30, canSpendWithConfirmation: canSpendWithConfirmation),
            pending: 5
        )
    }

    @Test("available includes gaining-privacy money when it can be spent with confirmation (balanced)")
    func availableIncludesGainingPrivacyWhenConfirmable() {
        #expect(makeBalance(canSpendWithConfirmation: true).available == 130)
    }

    @Test("available excludes gaining-privacy money when it cannot be spent (maxPrivacy)")
    func availableExcludesGainingPrivacyWhenNotConfirmable() {
        #expect(makeBalance(canSpendWithConfirmation: false).available == 100)
    }

    @Test("total sums every bucket regardless of the confirmation flag")
    func totalIsFlagIndependent() {
        #expect(makeBalance(canSpendWithConfirmation: true).total == 135)
        #expect(makeBalance(canSpendWithConfirmation: false).total == 135)
    }

    @Test("empty collapses available and total to zero")
    func emptyIsZero() {
        #expect(CoinageBalance.empty.available == 0)
        #expect(CoinageBalance.empty.total == 0)
    }
}
