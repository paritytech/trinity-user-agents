import Testing
import Foundation
@testable import Coinage

struct RecyclerVoucherChunkerTests {
    private func makeVouchers(
        count: Int,
        exponent: Int16 = 0,
        recyclerIndex: UInt32 = 0,
        startingAt start: UInt64 = 0
    ) -> [Voucher] {
        (0 ..< count).map { offset in
            let item = start + UInt64(offset)
            return Voucher(
                exponent: exponent,
                derivationIndex: .harness(item),
                allocatedAt: .distantPast,
                readyAt: .distantPast,
                remoteState: .inRecycler(.init(index: recyclerIndex, membersCount: 0)),
                publicKey: Data(repeating: UInt8(truncatingIfNeeded: item), count: 32)
            )
        }
    }

    @Test("A recycler group over the limit is split into chunks that keep every voucher")
    func groupOverLimitIsChunked() throws {
        let vouchers = makeVouchers(count: 5)

        let chunks = try RecyclerVoucherChunker.chunk(vouchers, maxPerChunk: 2)

        #expect(chunks.map(\.vouchers.count) == [2, 2, 1])
        #expect(chunks.allSatisfy { $0.key == RecyclerKey(exponent: 0, index: 0) })
        #expect(chunks.flatMap(\.vouchers) == vouchers)
    }

    @Test("A group exactly at the limit stays in one chunk")
    func groupAtLimitIsNotSplit() throws {
        let vouchers = makeVouchers(count: 2)

        let chunks = try RecyclerVoucherChunker.chunk(vouchers, maxPerChunk: 2)

        #expect(chunks.map(\.vouchers.count) == [2])
    }

    @Test("Vouchers of different recyclers never share a chunk")
    func differentRecyclersStaySeparate() throws {
        let vouchers = makeVouchers(count: 2, exponent: 1, recyclerIndex: 7)
            + makeVouchers(count: 1, exponent: 1, recyclerIndex: 8, startingAt: 100)
            + makeVouchers(count: 1, exponent: 2, recyclerIndex: 7, startingAt: 200)

        let chunks = try RecyclerVoucherChunker.chunk(vouchers, maxPerChunk: 64)

        #expect(chunks.count == 3)
        #expect(Set(chunks.map(\.key)) == [
            RecyclerKey(exponent: 1, index: 7),
            RecyclerKey(exponent: 1, index: 8),
            RecyclerKey(exponent: 2, index: 7)
        ])
    }

    @Test("A voucher that is not in a recycler is rejected")
    func voucherWithoutRecyclerThrows() throws {
        let pending = Voucher(
            exponent: 0,
            derivationIndex: .harness(1),
            allocatedAt: .distantPast,
            readyAt: .distantPast,
            remoteState: .onboarding,
            publicKey: Data(repeating: 1, count: 32)
        )

        #expect(throws: CoinageCommonError.self) {
            try RecyclerVoucherChunker.chunk([pending], maxPerChunk: 64)
        }
    }
}
