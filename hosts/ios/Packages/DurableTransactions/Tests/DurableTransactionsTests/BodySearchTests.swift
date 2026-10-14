import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
import Testing

@Suite("Block Body Searcher")
struct BodySearchTests {
    struct HitOutcomeCase: CustomStringConvertible {
        let lookup: BlockLookup
        let expected: BodySearchOutcome
        let name: String

        var description: String { name }
    }

    struct OutcomeCase: CustomStringConvertible {
        let lookup: BlockLookup
        let expected: DispatchOutcome?
        let name: String

        var description: String { name }
    }

    @Test("Window is scanned newest-first; hash in higher block wins")
    func windowScannedNewestFirst() async {
        let hash = Data([1])

        let stub = StubBlockDataReader(
            lookups: [
                Data([101]): .outcome(.present(.failed(reason: "Test.Failed"))),
                Data([102]): .outcome(.present(.succeeded))
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                101: Data([101]),
                102: Data([102])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: hash, in: 101 ... 102)

        #expect(result == .foundSucceeded(BlockRef(number: 102, hash: Data([102]))))

        let reads = await stub.reads
        #expect(reads.count == 1)
        #expect(reads[0] == Data([102]))
    }

    @Test("Window is re-read on each pass; nothing carried between passes")
    func windowRereadBetweenPasses() async {
        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): .notInBlock,
                Data([101]): .notInBlock,
                Data([102]): .notInBlock
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100]),
                101: Data([101]),
                102: Data([102])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result1 = await searcher.search(for: Data([99]), in: 100 ... 102)
        #expect(result1 == .notFoundWindowComplete)

        let result2 = await searcher.search(for: Data([99]), in: 100 ... 102)
        #expect(result2 == .notFoundWindowComplete)

        let reads = await stub.reads
        #expect(reads == [102, 101, 100, 102, 101, 100].map { Data([$0]) })
    }

    @Test("Hash in block with hash fetch failure leaves entry pending")
    func hashNotFoundButHashFetchFails() async {
        let targetHash = Data([99])

        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): .notInBlock
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: targetHash, in: 100 ... 101)

        #expect(result == .incomplete)

        let reads = await stub.reads
        #expect(reads == [Data([100])])
    }

    @Test("Unreadable block body leaves entry pending")
    func blockBodyUnreadable() async {
        let hash = Data([2])

        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): .notInBlock,
                Data([101]): .unreadable
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100]),
                101: Data([101])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: hash, in: 100 ... 101)

        #expect(result == .incomplete)
    }

    @Test("Absence proven only when entire window readable")
    func absenceProvenWhenWindowComplete() async {
        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): .notInBlock,
                Data([101]): .notInBlock,
                Data([102]): .notInBlock
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100]),
                101: Data([101]),
                102: Data([102])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: Data([99]), in: 100 ... 102)

        #expect(result == .notFoundWindowComplete)

        let reads = await stub.reads
        #expect(reads == [102, 101, 100].map { Data([$0]) })
    }

    @Test("Hit resolves outcome at same block; outcome unreadable maps to foundOutcomeUnreadable")
    func hitResolvesOutcomeAtSameBlock() async {
        let hash = Data([42])

        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): .notInBlock,
                Data([101]): .outcome(.present(.succeeded)),
                Data([102]): .notInBlock
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100]),
                101: Data([101]),
                102: Data([102])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: hash, in: 100 ... 102)

        #expect(result == .foundSucceeded(BlockRef(number: 101, hash: Data([101]))))

        let reads = await stub.reads
        #expect(reads.count == 2)
        #expect(reads[0] == Data([102]))
        #expect(reads[1] == Data([101]))
    }

    @Test("Hit with outcome", arguments: [
        HitOutcomeCase(
            lookup: .outcome(.present(.succeeded)),
            expected: .foundSucceeded(BlockRef(number: 100, hash: Data([100]))),
            name: "success"
        ),
        HitOutcomeCase(
            lookup: .outcome(.present(.failed(reason: "Test.Failed"))),
            expected: .foundFailed(BlockRef(number: 100, hash: Data([100])), reason: "Test.Failed"),
            name: "failure"
        ),
        HitOutcomeCase(
            lookup: .outcome(.failedRead),
            expected: .foundOutcomeUnreadable(BlockRef(number: 100, hash: Data([100]))),
            name: "unreadable (failedRead)"
        ),
        HitOutcomeCase(
            lookup: .outcome(.absent),
            expected: .foundOutcomeUnreadable(BlockRef(number: 100, hash: Data([100]))),
            name: "unreadable (absent)"
        )
    ])
    func hitWithOutcome(_ testCase: HitOutcomeCase) async {
        let hash = Data([1])

        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): testCase.lookup
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.search(for: hash, in: 100 ... 100)

        #expect(result == testCase.expected)
    }

    @Test("outcome(of:at:)", arguments: [
        OutcomeCase(
            lookup: .outcome(.present(.succeeded)),
            expected: .succeeded,
            name: "succeeded"
        ),
        OutcomeCase(
            lookup: .outcome(.present(.failed(reason: "Test.Failed"))),
            expected: .failed(reason: "Test.Failed"),
            name: "failed carries its reason"
        ),
        OutcomeCase(
            lookup: .outcome(.failedRead),
            expected: nil,
            name: "failedRead"
        ),
        OutcomeCase(
            lookup: .notInBlock,
            expected: nil,
            name: "notInBlock"
        ),
        OutcomeCase(
            lookup: .unreadable,
            expected: nil,
            name: "unreadable"
        )
    ])
    func outcomeTest(_ testCase: OutcomeCase) async {
        let hash = Data([1])

        let stub = StubBlockDataReader(
            lookups: [
                Data([100]): testCase.lookup
            ]
        )
        let blockInfo = StubBlockInfoProvider(
            hashes: [
                100: Data([100])
            ]
        )

        let searcher = BlockBodyScan(
            outcomeReader: stub,
            blockInfoProvider: blockInfo
        )

        let result = await searcher.outcome(
            of: hash,
            at: BlockRef(number: 100, hash: Data([100]))
        )

        if let expected = testCase.expected {
            #expect(result.isPresent)
            #expect(result.value == expected)
        } else {
            #expect(result.isFailedRead)
        }

        let reads = await stub.reads
        #expect(reads == [Data([100])])
    }
}
