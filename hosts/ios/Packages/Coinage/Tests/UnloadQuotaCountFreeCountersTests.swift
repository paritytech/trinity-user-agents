import Testing
import Foundation
@testable import Coinage

/// Pins the paged counting `UnloadQuotaTracker` uses to estimate remaining unload quota: consumed
/// counters form a prefix (tokens are taken in index order), so the walk queries a batch at a time and
/// stops at the first batch that ends on a free counter — the tail past it need not be read.
@Suite("UnloadQuotaTracker.countFreeCounters")
struct UnloadQuotaCountFreeCountersTests {
    @Test("An all-free range stops after one batch and counts the whole range")
    func allFreeStopsEarly() async {
        let chain = Chain(consumedPrefix: 0)
        let free = await count(maxCounter: 250, batchSize: 100, chain: chain)

        #expect(free == 250)
        #expect(chain.requested == [0 ..< 100]) // stopped on the first batch, never queried the tail
    }

    @Test("A fully consumed range walks every batch and counts zero")
    func allConsumedWalksEverything() async {
        let chain = Chain(consumedPrefix: 250)
        let free = await count(maxCounter: 250, batchSize: 100, chain: chain)

        #expect(free == 0)
        #expect(chain.requested == [0 ..< 100, 100 ..< 200, 200 ..< 250])
    }

    @Test("A prefix boundary mid-batch stops there and counts the free remainder")
    func prefixBoundaryStopsAtItsBatch() async {
        let chain = Chain(consumedPrefix: 150)
        let free = await count(maxCounter: 250, batchSize: 100, chain: chain)

        #expect(free == 100) // 250 - 150 consumed
        #expect(chain.requested == [0 ..< 100, 100 ..< 200]) // third batch skipped
    }

    @Test("A prefix boundary on a batch edge stops on the next batch")
    func prefixBoundaryOnBatchEdge() async {
        let chain = Chain(consumedPrefix: 100)
        let free = await count(maxCounter: 250, batchSize: 100, chain: chain)

        #expect(free == 150)
        #expect(chain.requested == [0 ..< 100, 100 ..< 200])
    }

    @Test("A range smaller than a batch is a single query")
    func rangeSmallerThanBatch() async {
        let chain = Chain(consumedPrefix: 10)
        let free = await count(maxCounter: 50, batchSize: 100, chain: chain)

        #expect(free == 40)
        #expect(chain.requested == [0 ..< 50])
    }

    @Test("A zero range queries nothing")
    func zeroRange() async {
        let chain = Chain(consumedPrefix: 0)
        let free = await count(maxCounter: 0, batchSize: 100, chain: chain)

        #expect(free == 0)
        #expect(chain.requested.isEmpty)
    }
}

private extension UnloadQuotaCountFreeCountersTests {
    func count(maxCounter: UInt32, batchSize: UInt32, chain: Chain) async -> Int {
        await UnloadQuotaTracker.countFreeCounters(maxCounter: maxCounter, batchSize: batchSize) { range in
            chain.consumedStatus(for: range)
        }
    }

    /// A chain where counters `0 ..< consumedPrefix` are consumed and everything at or above it is free,
    /// recording each queried range so the paging can be asserted.
    final class Chain {
        let consumedPrefix: UInt32
        private(set) var requested: [Range<UInt32>] = []

        init(consumedPrefix: UInt32) {
            self.consumedPrefix = consumedPrefix
        }

        func consumedStatus(for range: Range<UInt32>) -> [Bool] {
            requested.append(range)
            return range.map { $0 < consumedPrefix }
        }
    }
}
