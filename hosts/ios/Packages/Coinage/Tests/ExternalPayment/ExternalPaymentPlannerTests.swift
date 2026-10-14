import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

/// The real planner over the real pre-classification and parametric strategies: what counts as
/// private is exactly what the balance calls usable.
struct ExternalPaymentPlannerTests {
    private typealias Factory = ExternalPaymentTestFactory

    private func makePlanner(
        vouchers: [Voucher] = [],
        coins: [Coin] = [],
        strategy: RecyclingStrategyType = .balanced
    ) -> ExternalPaymentPlanner {
        ExternalPaymentPlanner(
            coinService: StubCoinService(coins: coins.map { Factory.tracked($0) }),
            voucherService: StubVoucherService(vouchers: vouchers),
            classifier: ExternalPaymentAssetClassifier(
                settings: StubRecyclingStrategySettings(strategy: strategy),
                strategyResolver: StubRecyclingStrategyProvider(),
                ringCapacityProvider: StubRingCapacityProvider(capacities: [1: 767, 2: 767, 3: 767]),
                preClassificator: CoinageAssetPreClassificator()
            )
        )
    }

    private func indices(_ vouchers: [TrackedVoucher]) -> [CoinageKeyIndex] {
        vouchers.map(\.voucher.derivationIndex)
    }

    @Test func privateVouchersCoverTheAmountLargestFirst() async throws {
        let planner = makePlanner(vouchers: [
            Factory.voucher(index: 1, exponent: 2),
            Factory.voucher(index: 2, exponent: 3)
        ])

        let preview = try await planner.plan(amount: Factory.planks(3), context: Factory.denomination)

        guard case let .unloadVouchers(offboarding) = preview else {
            Issue.record("expected unloadVouchers: \(preview)")
            return
        }
        #expect(indices(offboarding.vouchers) == [2])
        #expect(offboarding.surplus == 0)
        #expect(try await planner.canPayPrivately(amount: Factory.planks(3), context: Factory.denomination))
    }

    @Test func gainingVouchersAreSpentAfterPrivateOnesAndLargestFirst() async throws {
        // A private 4 and two gaining 8s for a payment of 12 give up the privacy of one voucher, not two.
        let planner = makePlanner(vouchers: [
            Factory.voucher(index: 1, exponent: 1),
            Factory.gainingVoucher(index: 2, exponent: 2),
            Factory.gainingVoucher(index: 3, exponent: 2)
        ])
        let amount = Factory.planks(2) + Factory.planks(1)

        let preview = try await planner.plan(amount: amount, context: Factory.denomination)

        guard case let .unloadVouchers(offboarding) = preview else {
            Issue.record("expected unloadVouchers: \(preview)")
            return
        }
        #expect(indices(offboarding.vouchers).first == 1)
        #expect(indices(offboarding.vouchers).count == 2)
        #expect(offboarding.surplus == 0)
        #expect(try await !planner.canPayPrivately(amount: amount, context: Factory.denomination))
    }

    @Test func coinsCoverTheShortfallAndEveryVoucherIsOffboardedAsIs() async throws {
        let planner = makePlanner(
            vouchers: [Factory.voucher(index: 1, exponent: 2), Factory.gainingVoucher(index: 2, exponent: 1)],
            coins: [Factory.coin(index: 9, exponent: 3), Factory.coin(index: 8, exponent: 1)]
        )
        let amount = Factory.planks(3) + Factory.planks(2) + Factory.planks(1)

        let preview = try await planner.plan(amount: amount, context: Factory.denomination)

        guard case let .loadCoins(coins, exactVouchers) = preview else {
            Issue.record("expected loadCoins: \(preview)")
            return
        }
        #expect(Set(indices(exactVouchers)) == [1, 2])
        #expect(coins.map(\.coin.derivationIndex) == [9])
    }

    @Test func privateCoinsStillCostPrivacy() async throws {
        // Coins alone never make a plan private: loading them only to unload them gains nothing.
        let planner = makePlanner(coins: [Factory.coin(index: 9, exponent: 3)], strategy: .minPrivacy)

        let preview = try await planner.plan(amount: Factory.planks(3), context: Factory.denomination)

        #expect(preview.coins.map(\.coin.derivationIndex) == [9])
        #expect(try await !planner.canPayPrivately(amount: Factory.planks(3), context: Factory.denomination))
    }

    @Test func minPrivacyTreatsEveryRecyclerVoucherAsPrivate() async throws {
        let planner = makePlanner(vouchers: [Factory.gainingVoucher(index: 2, exponent: 3)], strategy: .minPrivacy)

        #expect(try await planner.canPayPrivately(amount: Factory.planks(3), context: Factory.denomination))
        #expect(try await planner.plan(amount: Factory.planks(3), context: Factory.denomination).coins.isEmpty)
    }

    @Test func nonSpendableAssetsAreIgnored() async throws {
        let consumed = CoinageAssetState(handedOff: false, consumerStatus: .finalizedSuccess, minterStatus: nil)
        let planner = ExternalPaymentPlanner(
            coinService: StubCoinService(coins: [
                Factory.tracked(Factory.coin(index: 9, isOnchain: false)),
                Factory.tracked(Factory.coin(index: 8), state: consumed)
            ]),
            voucherService: StubVoucherService(
                vouchers: [Factory.voucher(index: 1, inRecycler: false), Factory.voucher(index: 2)],
                states: [Factory.tracked(Factory.voucher(index: 2), state: consumed)]
            ),
            classifier: ExternalPaymentAssetClassifier(
                settings: StubRecyclingStrategySettings(strategy: .balanced),
                strategyResolver: StubRecyclingStrategyProvider(),
                ringCapacityProvider: StubRingCapacityProvider(capacities: [3: 767]),
                preClassificator: CoinageAssetPreClassificator()
            )
        )

        let preview = try await planner.plan(amount: Factory.planks(1), context: Factory.denomination)

        #expect(preview == .notEnoughBalance)
    }

    @Test func pickOffboardingTakesTheLargestVouchersUntilTheTargetIsReached() throws {
        let planner = makePlanner()
        let vouchers = [
            Factory.voucher(index: 1, exponent: 1), Factory.voucher(index: 2, exponent: 3), Factory.voucher(
                index: 3,
                exponent: 2
            )
        ].map { Factory.tracked($0) }

        let picked = try planner.pickOffboarding(
            from: vouchers,
            target: Factory.planks(3) + 1,
            context: Factory.denomination
        )

        #expect(indices(picked.vouchers) == [2, 3])
        #expect(picked.surplus == Factory.planks(2) - 1)
        #expect(throws: ExternalPaymentPlannerError.self) {
            try planner.pickOffboarding(from: vouchers, target: Factory.planks(4), context: Factory.denomination)
        }
    }

    @Test func unreachableAmountIsNotEnoughBalance() async throws {
        let planner = makePlanner(
            vouchers: [Factory.voucher(index: 1, exponent: 1)],
            coins: [Factory.coin(index: 9, exponent: 1)]
        )

        let preview = try await planner.plan(amount: Factory.planks(3), context: Factory.denomination)

        #expect(preview == .notEnoughBalance)
    }
}
