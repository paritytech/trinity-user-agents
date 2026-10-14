import Testing

@testable import Coinage

@Suite("Display buckets")
struct CoinageDisplayBucketTests {
    @Test("Only spendable-now holdings are Ready")
    func readyIsSpendableNow() {
        #expect(CoinageAvailability.availableNow.displayBucket == .ready)
    }

    @Test("Everything not spendable now is Clearing, whatever the reason")
    func everythingElseIsClearing() {
        #expect(CoinageAvailability.gainingPrivacy.displayBucket == .clearing)
        #expect(CoinageAvailability.pending.displayBucket == .clearing)
    }

    @Test("Ready and Clearing account for the whole balance")
    func thePairIsExhaustive() {
        let balance = CoinageBalance(
            availablePrivate: 700,
            gainingPrivacy: .init(amount: 200, canSpendWithConfirmation: true),
            pending: 100
        )

        #expect(balance.ready == 700)
        #expect(balance.clearing == 300)
        #expect(balance.ready + balance.clearing == balance.total)
    }

    @Test("Money still arriving is Clearing rather than missing")
    func pendingIsNotDropped() {
        let balance = CoinageBalance(
            availablePrivate: 0,
            gainingPrivacy: .init(amount: 0, canSpendWithConfirmation: true),
            pending: 42
        )

        #expect(balance.clearing == 42)
        #expect(balance.ready + balance.clearing == balance.total)
    }
}
