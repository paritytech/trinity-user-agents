import Coinage
import DurableTransactions
import Foundation
import Testing

/// Ownership and status-write invariants the tracker relies on: one-shot ownership via
/// ``DurableTxOwnershipSet``, and the repository's compare-and-set / field-write guards.
@Suite("Submission Watcher")
struct SubmissionWatcherTests {
    @Test("Ownership taken once and released exactly once, per attempt")
    func ownershipOneShotRelease() async throws {
        let watched = DurableTxOwnershipSet()
        let id = UUID()
        let attempt = Data(repeating: 0xAB, count: 32)

        #expect(watched.take(id, txHash: attempt))
        #expect(watched.isOwned(id))

        #expect(watched.release(id, txHash: attempt))
        #expect(!watched.isOwned(id))

        // Release is one-shot, so a caller can keep its release side effects one-shot too.
        #expect(!watched.release(id, txHash: attempt))

        // And those bytes are never watched again — a verdict about them was already formed.
        #expect(!watched.take(id, txHash: attempt))
        #expect(!watched.isOwned(id))
    }

    @Test("A rebuilt transaction is owned afresh")
    func rebuiltAttemptOwnedAgain() async throws {
        let watched = DurableTxOwnershipSet()
        let id = UUID()
        let first = Data(repeating: 0xAB, count: 32)
        let second = Data(repeating: 0xCD, count: 32)

        #expect(watched.take(id, txHash: first))
        #expect(watched.release(id, txHash: first))

        // Same row, different bytes: the rebuild has its own watch.
        #expect(watched.take(id, txHash: second))
        #expect(watched.isOwned(id))
    }

    @Test("Releasing ownership does not itself change the entry")
    func releaseLeavesEntryUnchanged() async throws {
        let store = MockCoinageTxRepository()
        let watched = DurableTxOwnershipSet()
        let id = UUID()

        try await store.register(.fixture(id: id, outputs: [.coin(1, testKey(1))]))

        let attempt = Data(repeating: 0xAB, count: 32)
        watched.take(id, txHash: attempt)
        _ = watched.release(id, txHash: attempt)

        let fetched = try await store.getEntry(id: id)
        #expect(fetched?.status == .pending)
        // txHash is fixed at registration and never rewritten by release.
        #expect(fetched?.txHash == Data(repeating: 0xAB, count: 32))
    }

    @Test("A finalized verdict on an already-failed entry is rejected")
    func finalizedOnFailedRejected() async throws {
        let store = MockCoinageTxRepository()
        let id = UUID()

        try await store.register(.fixture(id: id, outputs: [.coin(1, testKey(1))]))
        try await store.updateStatus(id, to: .failure)

        let wrote = try await store.updateTxStatus(
            for: id,
            expectedCurrentStatus: .failure,
            expectedTxHash: Data(repeating: 0xAB, count: 32),
            verdict: Verdict(status: .finalizedSuccess, successDetectedAt: nil)
        )

        #expect(wrote == false)
        let fetched = try await store.getEntry(id: id)
        #expect(fetched?.status == .failure)
    }
}
