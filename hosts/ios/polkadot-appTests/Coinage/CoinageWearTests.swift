import CoreGraphics
import Testing

@testable import polkadot_app

@Suite("Coin wear")
struct CoinageWearTests {
    @Test("A level counts doublings of the crowd a coin hides in")
    func levelsAreDoublings() {
        #expect(CoinageWear.level(hiddenAmong: 0) == 0)
        #expect(CoinageWear.level(hiddenAmong: 1) == 1)
        #expect(CoinageWear.level(hiddenAmong: 2) == 2)
        #expect(CoinageWear.level(hiddenAmong: 3) == 2)
        #expect(CoinageWear.level(hiddenAmong: 4) == 3)
        // A ring holds 767 keys, so the deepest a coin can hide is among 766: 2^9 < 766 < 2^10.
        #expect(CoinageWear.level(hiddenAmong: CoinageWear.ringCapacity - 1) == 10)
    }

    @Test("Wear runs from untouched at a full ring to total with no crowd at all")
    func wearSpansTheLadder() {
        #expect(CoinageWear.amount(forLevel: CoinageWear.maximumLevel) == 0)
        #expect(CoinageWear.amount(forLevel: 0) == 1)
    }

    @Test("Wear never rises as a coin becomes harder to follow")
    func wearFallsMonotonically() {
        let amounts = (0 ... CoinageWear.maximumLevel).map(CoinageWear.amount(forLevel:))

        #expect(zip(amounts, amounts.dropFirst()).allSatisfy { $0 >= $1 })
    }

    @Test("A holding with no recycler record wears as though nothing hides it")
    func missingRecordWearsWorst() {
        #expect(CoinageBreakdownFactory.wear(forScore: nil, isBatchUnloaded: false) == CoinageWear.unknown)
    }

    @Test("A full ring leaves the coin unmarked, an empty one leaves it fully worn")
    func scoreMapsOntoTheLadder() {
        #expect(CoinageBreakdownFactory.wear(forScore: 100, isBatchUnloaded: false) == 0)
        #expect(CoinageBreakdownFactory.wear(forScore: 0, isBatchUnloaded: false) == 1)
    }

    @Test("A batch unload wears worse than a single one at the same score")
    func batchUnloadIsPenalised() {
        let single = CoinageBreakdownFactory.wear(forScore: 50, isBatchUnloaded: false)
        let batch = CoinageBreakdownFactory.wear(forScore: 50, isBatchUnloaded: true)

        #expect(batch > single)
    }

    @Test("The penalty cannot push a coin past the worn end of the ladder")
    func penaltyIsClamped() {
        #expect(CoinageBreakdownFactory.wear(forScore: 0, isBatchUnloaded: true) == 1)
    }
}
