import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import os
import Testing

/// The reference oracle for "an effect that appears and stays": one batched read per head, and a
/// transaction missing from the answer decides nothing.
@Suite("Monotone Effect Oracle")
struct MonotoneEffectOracleTests {
    private let view = StubPinnedChainView(finalized: .fixture(150), best: .fixture(200))

    @Test("An effect observed at the finalized head proves completion there and not-completion nowhere")
    func effectAtFinalizedProvesCompletion() async throws {
        let tx = DurableTxEntry.fixture()
        let oracle = MonotoneEffectOracle(chainId: "chain") { transactions, _ in
            Dictionary(uniqueKeysWithValues: transactions.map { ($0.id, true) })
        }

        let scope = try await oracle.openPass(
            transactions: [tx],
            ledger: SnapshotLedgerView(transactions: [tx]),
            view: view
        )

        #expect(scope.provenCompleted(tx, at: .finalized))
        #expect(scope.provenCompleted(tx, at: .best))
        #expect(!scope.provenNotCompleted(tx, at: .finalized))
        #expect(!scope.provenNotCompleted(tx, at: .best))
    }

    @Test("A false reading is a positive claim of non-completion at that head")
    func falseReadingProvesNotCompleted() async throws {
        let tx = DurableTxEntry.fixture()
        let oracle = MonotoneEffectOracle(chainId: "chain") { transactions, at in
            // Not yet at the finalized head, already at the best head.
            Dictionary(uniqueKeysWithValues: transactions.map { ($0.id, at.number >= 200) })
        }

        let scope = try await oracle.openPass(
            transactions: [tx],
            ledger: SnapshotLedgerView(transactions: [tx]),
            view: view
        )

        #expect(scope.provenNotCompleted(tx, at: .finalized))
        #expect(!scope.provenCompleted(tx, at: .finalized))
        #expect(scope.provenCompleted(tx, at: .best))
        #expect(!scope.provenNotCompleted(tx, at: .best))
    }

    @Test("A transaction missing from the answer decides nothing")
    func missingTransactionDecidesNothing() async throws {
        let tx = DurableTxEntry.fixture()
        let oracle = MonotoneEffectOracle(chainId: "chain") { _, _ in [:] }

        let scope = try await oracle.openPass(
            transactions: [tx],
            ledger: SnapshotLedgerView(transactions: [tx]),
            view: view
        )

        #expect(!scope.provenCompleted(tx, at: .finalized))
        #expect(!scope.provenCompleted(tx, at: .best))
        #expect(!scope.provenNotCompleted(tx, at: .finalized))
        #expect(!scope.provenNotCompleted(tx, at: .best))
    }

    @Test("The domain is read once per head for the whole pass, at the pinned heads")
    func readsOncePerHead() async throws {
        let transactions = [DurableTxEntry.fixture(), DurableTxEntry.fixture(), DurableTxEntry.fixture()]
        let reads = OSAllocatedUnfairLock(initialState: [(count: Int, at: BlockRef)]())
        let oracle = MonotoneEffectOracle(chainId: "chain") { batch, at in
            reads.withLock { $0.append((batch.count, at)) }
            return [:]
        }

        _ = try await oracle.openPass(
            transactions: transactions,
            ledger: SnapshotLedgerView(transactions: transactions),
            view: view
        )

        let recorded = reads.withLock { $0 }
        #expect(recorded.count == 2)
        #expect(recorded.allSatisfy { $0.count == 3 })
        #expect(Set(recorded.map(\.at)) == [view.finalizedHead, view.bestHead])
    }

    @Test("A failed read aborts the pass for the domain rather than answering")
    func failedReadThrows() async throws {
        let tx = DurableTxEntry.fixture()
        let oracle = MonotoneEffectOracle(chainId: "chain") { _, _ in throw ChainReadFailure(message: "down") }

        await #expect(throws: ChainReadFailure.self) {
            try await oracle.openPass(transactions: [tx], ledger: SnapshotLedgerView(transactions: [tx]), view: view)
        }
    }
}
