import BigInt
import Foundation
import Testing
@testable import Coinage

/// Pure three-bucket balance calculation.
///
/// Coins bucket by the recycling verdict passed in (as the evaluator supplies it); vouchers bucket by
/// the strategy's own usability rule. Asserts `availablePrivate` / `gainingPrivacy` / `pending`
/// directly against `CoinageBalanceService.calculateBalance`, with no actor, streams, or chain reads.
@Suite("Coinage balance calculation")
struct CoinageBalanceTests {
    private let context = DenominationBreakdownContext(
        unit: BigUInt(1_000_000),
        precision: 6,
        maxExponent: 7,
        minExponent: -6
    )

    // MARK: - Coins (verdict-driven)

    @Test("Empty data returns zero balance")
    func emptyIsZero() {
        expect(coins: [], vouchers: [], verdicts: [:], availablePrivate: 0, gaining: 0, pending: 0)
    }

    @Test("A coin the strategy allows is available")
    func allowedCoinAvailable() {
        expect(
            coins: [minted(exponent: 1, index: 0)],
            verdicts: [0: .allowUse],
            availablePrivate: planks(1), gaining: 0, pending: 0
        )
    }

    @Test("A coin the strategy gated is gaining privacy, not spendable")
    func gatedCoinGainsPrivacy() {
        expect(
            coins: [minted(exponent: 1, index: 0)],
            verdicts: [0: .toRecycle],
            availablePrivate: 0, gaining: planks(1), pending: 0
        )
    }

    @Test("A coin the chain will no longer accept is pending")
    func mustRecycleCoinPending() {
        expect(
            coins: [minted(exponent: 1, index: 0)],
            verdicts: [0: .mustRecycle],
            availablePrivate: 0, gaining: 0, pending: planks(1)
        )
    }

    @Test("A settled coin the evaluator has not judged yet counts as pending")
    func unjudgedCoinPending() {
        expect(
            coins: [minted(exponent: 1, index: 0)],
            verdicts: [:],
            availablePrivate: 0, gaining: 0, pending: planks(1)
        )
    }

    @Test("A coin still arriving is pending")
    func arrivingCoinPending() {
        expect(
            coins: [tracked(coin(exponent: 1, index: 0, age: nil, onChain: false), state: minter(.pending))],
            verdicts: [:],
            availablePrivate: 0, gaining: 0, pending: planks(1)
        )
    }

    @Test("A coin whose mint provably failed counts nowhere")
    func failedMintNowhere() {
        expect(
            coins: [tracked(coin(exponent: 1, index: 0, age: nil, onChain: false), state: minter(.failure))],
            verdicts: [:],
            availablePrivate: 0, gaining: 0, pending: 0
        )
    }

    @Test("A coin held by a live transaction of ours counts nowhere")
    func heldCoinNowhere() {
        expect(
            coins: [tracked(coin(exponent: 1, index: 0, age: 0, onChain: true), state: consumer(.pending))],
            verdicts: [0: .allowUse],
            availablePrivate: 0, gaining: 0, pending: 0
        )
    }

    @Test("A coin handed off counts nowhere")
    func handedOffCoinNowhere() {
        let state = CoinageAssetState(handedOff: true, consumerStatus: nil, minterStatus: nil)
        expect(
            coins: [tracked(coin(exponent: 1, index: 0, age: 0, onChain: true), state: state)],
            verdicts: [0: .allowUse],
            availablePrivate: 0, gaining: 0, pending: 0
        )
    }

    // MARK: - Vouchers (strategy-driven usability)

    @Test("An in-recycler voucher the strategy releases is available")
    func usableVoucherAvailable() {
        expect(
            vouchers: [tracked(inRecycler(exponent: 1, members: 1))],
            strategy: .minPrivacy,
            availablePrivate: planks(1), gaining: 0, pending: 0
        )
    }

    @Test("An in-recycler voucher held back for privacy is gaining privacy")
    func heldVoucherGainsPrivacy() {
        expect(
            vouchers: [tracked(inRecycler(exponent: 1, members: 1))],
            strategy: .maxPrivacy,
            capacities: [1: 767],
            availablePrivate: 0, gaining: planks(1), pending: 0
        )
    }

    @Test("An onboarding voucher is pending")
    func onboardingVoucherPending() {
        expect(
            vouchers: [tracked(voucher(exponent: 1, state: .onboarding))],
            availablePrivate: 0, gaining: 0, pending: planks(1)
        )
    }

    @Test("An unlocated voucher still arriving is pending")
    func arrivingVoucherPending() {
        expect(
            vouchers: [tracked(voucher(exponent: 1, state: .unlocated), state: minter(.pending))],
            availablePrivate: 0, gaining: 0, pending: planks(1)
        )
    }

    // MARK: - Combined

    @Test(arguments: [RecyclingStrategyType.balanced, .maxPrivacy])
    func maturityMovesVoucherBalanceToAvailable(_ type: RecyclingStrategyType) {
        let minimumMembers: UInt32 = 32
        let tenMinutes: TimeInterval = 10 * 60
        let ringCapacity = 767
        let exponent: Int16 = 1
        let enteredAt = Date(timeIntervalSince1970: 1_000)
        let trackedVoucher = tracked(voucher(
            exponent: exponent,
            state: .inRecycler(.init(index: 1, membersCount: minimumMembers, enteredAt: enteredAt))
        ))
        let strategy = ParametricRecyclingStrategy(
            params: type.params(forcedRecyclingAge: CoinageConstants.recycleAtAge)
        )
        let preClassificator = CoinageAssetPreClassificator()
        let balances = [tenMinutes - 1, tenMinutes].map { elapsed in
            let usability = VoucherUsabilityContext(
                ringCapacities: [exponent: ringCapacity],
                now: enteredAt.addingTimeInterval(elapsed)
            )
            return CoinageBalanceService.calculateBalance(
                coinBuckets: preClassificator.preClassifyCoins([]),
                voucherBuckets: preClassificator.preClassifyVouchers(
                    [trackedVoucher],
                    strategy: strategy,
                    context: usability
                ),
                verdicts: [:],
                canSpendWithConfirmation: strategy.allowsConfirmedSpend(),
                context: context
            )
        }

        #expect(balances == [
            CoinageBalance(
                availablePrivate: 0,
                gainingPrivacy: .init(amount: planks(exponent), canSpendWithConfirmation: type == .balanced),
                pending: 0
            ),
            CoinageBalance(
                availablePrivate: planks(exponent),
                gainingPrivacy: .init(amount: 0, canSpendWithConfirmation: type == .balanced),
                pending: 0
            )
        ])
    }

    @Test("Buckets a mix of coins and vouchers correctly")
    func combined() {
        expect(
            coins: [
                minted(exponent: 1, index: 0),
                minted(exponent: 2, index: 1),
                minted(exponent: 3, index: 2)
            ],
            vouchers: [tracked(inRecycler(exponent: 4, members: 1))],
            verdicts: [0: .allowUse, 1: .toRecycle, 2: .mustRecycle],
            strategy: .minPrivacy,
            availablePrivate: planks(1) + planks(4), gaining: planks(2), pending: planks(3)
        )
    }
}

// MARK: - Helpers

private extension CoinageBalanceTests {
    func planks(_ exponent: Int16) -> BigUInt { context.valueInPlanks(for: exponent) }

    var free: CoinageAssetState { CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil) }
    func minter(_ status: CoinageTxStatus) -> CoinageAssetState {
        CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: status)
    }

    func consumer(_ status: CoinageTxStatus) -> CoinageAssetState {
        CoinageAssetState(handedOff: false, consumerStatus: status, minterStatus: nil)
    }

    func key(_ index: CoinageKeyIndex) -> Data {
        Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
    }

    func coin(exponent: Int16, index: CoinageKeyIndex, age: Int16?, onChain: Bool) -> Coin {
        Coin(exponent: exponent, derivationIndex: index, age: age, isOnchain: onChain, publicKey: key(index))
    }

    func minted(exponent: Int16, index: CoinageKeyIndex) -> TrackedCoin {
        tracked(coin(exponent: exponent, index: index, age: 0, onChain: true), state: free)
    }

    func tracked(_ coin: Coin, state: CoinageAssetState) -> TrackedCoin {
        TrackedCoin(coin: coin, state: state)
    }

    func voucher(exponent: Int16, state: Voucher.OnChainState, index: CoinageKeyIndex = 0) -> Voucher {
        Voucher(
            exponent: exponent,
            derivationIndex: index,
            allocatedAt: Date(timeIntervalSince1970: 0),
            readyAt: Date(timeIntervalSinceNow: -3_600),
            remoteState: state,
            publicKey: key(index)
        )
    }

    func inRecycler(exponent: Int16, members: UInt32, index: CoinageKeyIndex = 0) -> Voucher {
        voucher(exponent: exponent, state: .inRecycler(.init(index: 1, membersCount: members)), index: index)
    }

    func tracked(_ voucher: Voucher, state: CoinageAssetState? = nil) -> TrackedVoucher {
        TrackedVoucher(voucher: voucher, state: state ?? free)
    }

    // swiftlint:disable:next function_parameter_count
    func expect(
        coins: [TrackedCoin] = [],
        vouchers: [TrackedVoucher] = [],
        verdicts: RecyclingVerdicts = [:],
        strategy: RecyclingStrategyType = .minPrivacy,
        capacities: [Int16: Int] = [:],
        availablePrivate: BigUInt,
        gaining: BigUInt,
        pending: BigUInt,
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        let voucherStrategy = ParametricRecyclingStrategy(
            params: strategy.params(forcedRecyclingAge: CoinageConstants.recycleAtAge)
        )
        let usability = VoucherUsabilityContext(ringCapacities: capacities, now: Date())
        let preClassificator = CoinageAssetPreClassificator()

        let coinBuckets = preClassificator.preClassifyCoins(coins)
        let voucherBuckets = preClassificator.preClassifyVouchers(
            vouchers,
            strategy: voucherStrategy,
            context: usability
        )

        let balance = CoinageBalanceService.calculateBalance(
            coinBuckets: coinBuckets,
            voucherBuckets: voucherBuckets,
            verdicts: verdicts,
            canSpendWithConfirmation: voucherStrategy.allowsConfirmedSpend(),
            context: context
        )

        #expect(balance.availablePrivate == availablePrivate, sourceLocation: sourceLocation)
        #expect(balance.gainingPrivacy.amount == gaining, sourceLocation: sourceLocation)
        #expect(balance.pending == pending, sourceLocation: sourceLocation)
    }
}
