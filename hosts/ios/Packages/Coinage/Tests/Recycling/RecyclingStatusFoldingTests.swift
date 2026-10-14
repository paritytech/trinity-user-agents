import Foundation
import Testing
@testable import Coinage
import DurableTransactions

struct RecyclingStatusFoldingTests {
    private typealias Factory = ExternalPaymentTestFactory

    private func entry(status: CoinageTxStatus, mintedIndex: CoinageKeyIndex) -> CoinageTxEntry {
        CoinageTxEntry(
            inputs: [],
            outputs: [.recyclerVoucher(
                mintedIndex,
                Data(repeating: UInt8(truncatingIfNeeded: mintedIndex.item), count: 32)
            )],
            groupId: "g",
            txHash: Data(repeating: 0xAB, count: 32),
            checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
            mortality: 300,
            status: status
        )
    }

    private let vouchers = StubVoucherService(vouchers: [Factory.voucher(index: 1), Factory.voucher(index: 2)])

    @Test func emptyOrUnincludedIsPending() async throws {
        #expect(try await RecyclingStatusFolder.fold(entries: [], voucherService: vouchers) == .pending)
        let mixed = [entry(status: .finalizedSuccess, mintedIndex: 1), entry(status: .pending, mintedIndex: 2)]
        #expect(try await RecyclingStatusFolder.fold(entries: mixed, voucherService: vouchers) == .pending)
    }

    @Test func bestBlockInclusionIsAllRecycledWithTheMintedVouchers() async throws {
        let entries = [entry(status: .pendingSuccess, mintedIndex: 1), entry(status: .finalizedSuccess, mintedIndex: 2)]

        let status = try await RecyclingStatusFolder.fold(entries: entries, voucherService: vouchers)

        guard case let .allRecycled(minted, finalized) = status else {
            Issue.record("expected allRecycled: \(status)")
            return
        }
        #expect(Set(minted.map(\.voucher.derivationIndex)) == [1, 2])
        #expect(!finalized)
    }

    @Test func allFinalizedIsAllRecycledFinalized() async throws {
        let entries = [entry(status: .finalizedSuccess, mintedIndex: 1)]

        let status = try await RecyclingStatusFolder.fold(entries: entries, voucherService: vouchers)

        #expect(status == .allRecycled(vouchers: [Factory.tracked(Factory.voucher(index: 1))], finalized: true))
    }

    @Test func aMintedVoucherNotYetSeenInItsRecyclerKeepsPending() async throws {
        let notLocated = StubVoucherService(vouchers: [Factory.voucher(index: 1, inRecycler: false)])
        let entries = [entry(status: .finalizedSuccess, mintedIndex: 1)]

        #expect(try await RecyclingStatusFolder.fold(entries: entries, voucherService: notLocated) == .pending)
        #expect(try await RecyclingStatusFolder.fold(
            entries: [entry(status: .finalizedSuccess, mintedIndex: 9)],
            voucherService: vouchers
        ) == .pending)
    }

    @Test func aFailureIsIncomplete() async throws {
        let entries = [entry(status: .failure, mintedIndex: 1), entry(status: .pending, mintedIndex: 2)]

        #expect(try await RecyclingStatusFolder.fold(entries: entries, voucherService: vouchers) == .incomplete)
    }
}
