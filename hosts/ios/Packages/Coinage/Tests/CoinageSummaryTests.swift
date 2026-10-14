import Testing
import Foundation
import BigInt
@testable import Coinage

/// ``CoinageHoldings/make(coinBuckets:voucherBuckets:verdicts:)`` and
/// ``CoinageBalanceService/calculateBalance(coinBuckets:voucherBuckets:verdicts:canSpendWithConfirmation:context:)``
/// classify the same inputs independently — one per holding, the other straight into totals. The two
/// mirror each other by hand, so these pin them together: whatever the balance counts in a bucket,
/// the holdings must attribute to the same bucket.
struct CoinageSummaryTests {
    private let context = DenominationBreakdownContext(
        unit: BigUInt(10).power(16),
        precision: 18,
        maxExponent: 10,
        minExponent: 0
    )

    // MARK: - Consistency

    @Test(
        "holdings attribute value to the same buckets the balance totals",
        arguments: [true, false]
    )
    func holdingsMatchBalance(canSpendWithConfirmation: Bool) {
        let coinBuckets = CoinBuckets(
            minted: [
                coin(1, exponent: 3), // allowUse    -> available now
                coin(2, exponent: 4), // toRecycle   -> gaining privacy
                coin(3, exponent: 5), // mustRecycle -> pending
                coin(4, exponent: 6) // no verdict  -> pending
            ],
            minting: [coin(5, exponent: 7)] // still arriving -> pending
        )
        let voucherBuckets = VoucherBuckets(
            usable: [voucher(10, exponent: 2, isPlaced: true)],
            gainingPrivacy: [voucher(11, exponent: 8, isPlaced: true)],
            minting: [voucher(12, exponent: 1, isPlaced: false)]
        )
        let verdicts: RecyclingVerdicts = [
            1: .allowUse,
            2: .toRecycle,
            3: .mustRecycle
        ]

        let balance = CoinageBalanceService.calculateBalance(
            coinBuckets: coinBuckets,
            voucherBuckets: voucherBuckets,
            verdicts: verdicts,
            canSpendWithConfirmation: canSpendWithConfirmation,
            context: context
        )
        let holdings = CoinageHoldings.make(
            coinBuckets: coinBuckets,
            voucherBuckets: voucherBuckets,
            verdicts: verdicts
        )

        let totals = planks(of: holdings)

        #expect(totals.availableNow == balance.availablePrivate)
        #expect(totals.gainingPrivacy == balance.gainingPrivacy.amount)
        #expect(totals.pending == balance.pending)
        #expect(totals.availableNow + totals.gainingPrivacy + totals.pending == balance.total)
    }

    @Test("every counted asset appears exactly once")
    func noAssetIsDroppedOrDuplicated() {
        let coinBuckets = CoinBuckets(
            minted: [coin(1, exponent: 0), coin(2, exponent: 1)],
            minting: [coin(3, exponent: 2)]
        )
        let voucherBuckets = VoucherBuckets(
            usable: [voucher(10, exponent: 3, isPlaced: true)],
            gainingPrivacy: [voucher(11, exponent: 4, isPlaced: true)],
            minting: [voucher(12, exponent: 5, isPlaced: false)]
        )

        let holdings = CoinageHoldings.make(
            coinBuckets: coinBuckets,
            voucherBuckets: voucherBuckets,
            verdicts: [1: .allowUse, 2: .toRecycle]
        )

        #expect(holdings.coins.count == 3)
        #expect(holdings.vouchers.count == 3)
        #expect(Set(holdings.coins.map(\.coin.derivationIndex)).count == 3)
        #expect(Set(holdings.vouchers.map(\.voucher.derivationIndex)).count == 3)
    }

    /// An empty snapshot has to total zero rather than being withheld, or the display would keep
    /// showing the last non-empty figures after everything is spent.
    @Test("an empty snapshot classifies to nothing")
    func emptySnapshot() {
        let holdings = CoinageHoldings.make(
            coinBuckets: CoinBuckets(minted: [], minting: []),
            voucherBuckets: VoucherBuckets(usable: [], gainingPrivacy: [], minting: []),
            verdicts: [:]
        )

        #expect(holdings == .empty)
    }
}

// MARK: - Fixtures

private extension CoinageSummaryTests {
    var freeState: CoinageAssetState {
        CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
    }

    func coin(_ index: CoinageKeyIndex, exponent: Int16) -> TrackedCoin {
        TrackedCoin(
            coin: Coin(
                exponent: exponent,
                derivationIndex: index,
                age: 1,
                isOnchain: true,
                publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
            ),
            state: freeState
        )
    }

    func voucher(_ index: CoinageKeyIndex, exponent: Int16, isPlaced: Bool) -> TrackedVoucher {
        TrackedVoucher(
            voucher: Voucher(
                exponent: exponent,
                derivationIndex: index,
                allocatedAt: Date(timeIntervalSinceReferenceDate: 0),
                readyAt: Date(timeIntervalSinceReferenceDate: 60),
                remoteState: isPlaced
                    ? .inRecycler(.init(index: 1, membersCount: 16))
                    : .onboarding,
                publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
            ),
            state: freeState
        )
    }

    func planks(
        of holdings: CoinageHoldings
    ) -> (availableNow: BigUInt, gainingPrivacy: BigUInt, pending: BigUInt) {
        var availableNow = BigUInt(0)
        var gainingPrivacy = BigUInt(0)
        var pending = BigUInt(0)

        func add(_ availability: CoinageAvailability, _ value: BigUInt) {
            switch availability {
            case .availableNow: availableNow += value
            case .gainingPrivacy: gainingPrivacy += value
            case .pending: pending += value
            }
        }

        for holding in holdings.coins {
            add(holding.availability, context.valueInPlanks(for: holding.coin.exponent))
        }

        for holding in holdings.vouchers {
            add(holding.availability, context.valueInPlanks(for: holding.voucher.exponent))
        }

        return (availableNow, gainingPrivacy, pending)
    }
}
