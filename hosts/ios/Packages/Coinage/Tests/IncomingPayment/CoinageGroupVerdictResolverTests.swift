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

    @Test func emptyGroupIsNotClaimed() async throws {
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(),
            coinService: InMemoryCoinService(),
            voucherService: InMemoryVoucherService()
        )

        let verdict = try await resolver.settledVerdict(groupId: "ghost", amount: 8, context: Self.denomination)

        #expect(verdict == .notClaimed)
    }
}
