import BigInt
import Foundation
import Testing
@testable import Coinage

/// The two spend scopes: `.spendable` draws only on freely-usable funds; `.withConfirmation` widens to
/// gaining-privacy funds, but only when the strategy allows a confirmed spend — so under `maxPrivacy` it
/// equals `.spendable`. `.mustRecycle` coins are never offered on any scope.
@Suite("Coinage asset selector — spend scopes")
struct CoinageAssetSelectorTests {
    private let selector = CoinageAssetSelector(preClassificator: CoinageAssetPreClassificator())
    private let context = DenominationBreakdownContext(
        unit: BigUInt(1_000_000),
        precision: 6,
        maxExponent: 7,
        minExponent: -6
    )

    // MARK: - Coins

    @Test("Spendable scope returns only allowUse coins")
    func spendableCoins() {
        let result = selector.selectableCoins(
            coins,
            verdicts: verdicts,
            allowsConfirmedSpend: true,
            scope: .spendable
        )
        #expect(indices(result) == [0])
    }

    @Test("With-confirmation adds toRecycle coins, never mustRecycle")
    func withConfirmationCoins() {
        let result = selector.selectableCoins(
            coins,
            verdicts: verdicts,
            allowsConfirmedSpend: true,
            scope: .withConfirmation
        )
        #expect(indices(result) == [0, 1])
    }

    @Test("With-confirmation cannot widen when the strategy forbids confirmed spend")
    func withConfirmationCoinsForbidden() {
        let result = selector.selectableCoins(
            coins,
            verdicts: verdicts,
            allowsConfirmedSpend: false,
            scope: .withConfirmation
        )
        #expect(indices(result) == [0])
    }

    // MARK: - Vouchers

    @Test(arguments: [RecyclingStrategyType.balanced, .maxPrivacy])
    func maturityMakesVouchersSpendableWithoutConfirmation(_ type: RecyclingStrategyType) {
        let minimumMembers: UInt32 = 32
        let tenMinutes: TimeInterval = 10 * 60
        let ringCapacity = 767
        let enteredAt = Date(timeIntervalSince1970: 1_000)
        let voucher = gainingPrivacyVoucher.voucher.adjusting(
            state: .inRecycler(.init(index: 1, membersCount: minimumMembers, enteredAt: enteredAt))
        )
        let tracked = TrackedVoucher(voucher: voucher, state: free)
        let policy = strategy(type)
        let before = VoucherUsabilityContext(
            ringCapacities: [voucher.exponent: ringCapacity],
            now: enteredAt.addingTimeInterval(tenMinutes - 1)
        )
        let after = VoucherUsabilityContext(
            ringCapacities: [voucher.exponent: ringCapacity],
            now: enteredAt.addingTimeInterval(tenMinutes)
        )

        #expect(selector.selectableVouchers([tracked], strategy: policy, context: before, scope: .spendable).isEmpty)
        #expect(selector.selectableVouchers(
            [tracked], strategy: policy, context: before, scope: .withConfirmation
        ) == (type == .balanced ? [tracked] : []))
        #expect(selector
            .selectableVouchers([tracked], strategy: policy, context: after, scope: .spendable) == [tracked])
    }

    @Test("Spendable scope returns only usable vouchers")
    func spendableVouchers() {
        let result = selector.selectableVouchers(
            [gainingPrivacyVoucher],
            strategy: strategy(.balanced),
            context: usability,
            scope: .spendable
        )
        #expect(result.isEmpty)
    }

    @Test("With-confirmation adds gaining-privacy vouchers when the strategy allows it")
    func withConfirmationVouchers() {
        let result = selector.selectableVouchers(
            [gainingPrivacyVoucher],
            strategy: strategy(.balanced),
            context: usability,
            scope: .withConfirmation
        )
        #expect(voucherIndices(result) == [0])
    }

    @Test("Max privacy never widens: with-confirmation equals spendable")
    func maxPrivacyNeverWidens() {
        let result = selector.selectableVouchers(
            [gainingPrivacyVoucher],
            strategy: strategy(.maxPrivacy),
            context: usability,
            scope: .withConfirmation
        )
        #expect(result.isEmpty)
    }
}

// MARK: - Fixtures

private extension CoinageAssetSelectorTests {
    var verdicts: RecyclingVerdicts { [0: .allowUse, 1: .toRecycle, 2: .mustRecycle] }

    var coins: [TrackedCoin] {
        [mintedCoin(index: 0), mintedCoin(index: 1), mintedCoin(index: 2)]
    }

    /// A ring far below `balanced`/`maxPrivacy`'s required fill, so it classifies as gaining-privacy.
    var gainingPrivacyVoucher: TrackedVoucher {
        let voucher = Voucher(
            exponent: 1,
            derivationIndex: 0,
            allocatedAt: Date(timeIntervalSince1970: 0),
            readyAt: Date(timeIntervalSinceNow: 3_600),
            remoteState: .inRecycler(.init(index: 1, membersCount: 1)),
            publicKey: Data(repeating: 0, count: 32)
        )
        return TrackedVoucher(voucher: voucher, state: free)
    }

    var usability: VoucherUsabilityContext {
        VoucherUsabilityContext(ringCapacities: [1: 767], now: Date())
    }

    var free: CoinageAssetState {
        CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
    }

    func mintedCoin(index: CoinageKeyIndex) -> TrackedCoin {
        let coin = Coin(
            exponent: 1,
            derivationIndex: index,
            age: 0,
            isOnchain: true,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
        return TrackedCoin(coin: coin, state: free)
    }

    func strategy(_ type: RecyclingStrategyType) -> ParametricRecyclingStrategy {
        ParametricRecyclingStrategy(params: type.params(forcedRecyclingAge: CoinageConstants.recycleAtAge))
    }

    func indices(_ coins: [TrackedCoin]) -> [CoinageKeyIndex] {
        coins.map(\.coin.derivationIndex).sorted()
    }

    func voucherIndices(_ vouchers: [TrackedVoucher]) -> [CoinageKeyIndex] {
        vouchers.map(\.voucher.derivationIndex).sorted()
    }
}
