import Testing
import Foundation
import SubstrateSdk
@testable import Coinage

/// The secret-lost fallback values a group by what finalized, whichever claim service registered it.
struct CoinageGroupVerdictResolverTests {
    /// Denominations 8, 4, 2, 1 planks.
    private static let denomination = DenominationBreakdownContext(
        unit: 1,
        precision: 0,
        maxExponent: 3,
        minExponent: 0
    )

    @Test func valuesFinalizedCoinsAndVouchersOnly() async throws {
        let repository = MockCoinageTxRepository()
        let coin = Coin(exponent: 3, derivationIndex: 1, age: nil, publicKey: testKey(1))
        let voucher = Voucher(
            exponent: 1, derivationIndex: 2, allocatedAt: Date(), readyAt: Date(), publicKey: testKey(2)
        )
        let failedVoucher = Voucher(
            exponent: 2, derivationIndex: 3, allocatedAt: Date(), readyAt: Date(), publicKey: testKey(3)
        )
        try await repository.register(.fixture(
            outputs: [.coin(1, coin.publicKey)], status: .finalizedSuccess, groupId: "g"
        ))
        try await repository.register(.fixture(
            outputs: [.recyclerVoucher(2, voucher.publicKey)], status: .finalizedSuccess, groupId: "g"
        ))
        try await repository.register(.fixture(
            outputs: [.recyclerVoucher(3, failedVoucher.publicKey)], status: .failure, groupId: "g"
        ))
        let vouchers = InMemoryVoucherService()
        vouchers.save([voucher, failedVoucher])
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(store: repository),
            coinService: InMemoryCoinService(coins: [coin]),
            voucherService: vouchers
        )

        let verdict = try await resolver.settledVerdict(groupId: "g", amount: 14, context: Self.denomination)

        #expect(verdict == .claimedPartially(claimed: 10))
    }

    @Test func fullyFinalizedGroupIsClaimed() async throws {
        let repository = MockCoinageTxRepository()
        let voucher = Voucher(
            exponent: 3, derivationIndex: 4, allocatedAt: Date(), readyAt: Date(), publicKey: testKey(4)
        )
        try await repository.register(.fixture(
            outputs: [.recyclerVoucher(4, voucher.publicKey)], status: .finalizedSuccess, groupId: "g"
        ))
        let vouchers = InMemoryVoucherService()
        vouchers.save([voucher])
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(store: repository),
            coinService: InMemoryCoinService(),
            voucherService: vouchers
        )

        let verdict = try await resolver.settledVerdict(groupId: "g", amount: 8, context: Self.denomination)

        #expect(verdict == .claimed(amount: 8, finalized: true))
    }

    @Test(arguments: [Balance(0), Balance(8)])
    func emptyGroupIsNotClaimed(amount: Balance) async throws {
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(),
            coinService: InMemoryCoinService(),
            voucherService: InMemoryVoucherService()
        )

        let verdict = try await resolver.settledVerdict(groupId: "ghost", amount: amount, context: Self.denomination)

        #expect(verdict == .notClaimed)
    }

    @Test func zeroMinimumStillReportsOnlyRealFinalizedCredit() async throws {
        let repository = MockCoinageTxRepository()
        let coin = Coin(exponent: 3, derivationIndex: 1, age: nil, publicKey: testKey(1))
        try await repository.register(.fixture(
            outputs: [.coin(1, coin.publicKey)], status: .finalizedSuccess, groupId: "g"
        ))
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(store: repository),
            coinService: InMemoryCoinService(coins: [coin]),
            voucherService: InMemoryVoucherService()
        )

        let verdict = try await resolver.settledVerdict(groupId: "g", amount: 0, context: Self.denomination)

        #expect(verdict == .claimedPartially(claimed: 8))
        #expect(IncomingPaymentStatus(detection: verdict, amount: 0) == .claimed(finalized: true))
    }

    @Test(.timeLimit(.minutes(1)))
    func interruptedGroupObservationDoesNotFinalizePartialCredit() async throws {
        let repository = MockCoinageTxRepository()
        let finalCoin = Coin(exponent: 1, derivationIndex: 1, age: nil, publicKey: testKey(1))
        let pendingCoin = Coin(exponent: 3, derivationIndex: 2, age: nil, publicKey: testKey(2))
        try await repository.register(.fixture(
            outputs: [.coin(1, finalCoin.publicKey)], status: .finalizedSuccess, groupId: "g"
        ))
        try await repository.register(.fixture(
            outputs: [.coin(2, pendingCoin.publicKey)], status: .pendingSuccess, groupId: "g"
        ))
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(store: repository),
            coinService: InMemoryCoinService(coins: [finalCoin, pendingCoin]),
            voucherService: InMemoryVoucherService()
        )
        let observation = Task {
            try await resolver.settledVerdict(groupId: "g", amount: 0, context: Self.denomination)
        }
        observation.cancel()

        await #expect(throws: (any Error).self) {
            _ = try await observation.value
        }
    }
}
