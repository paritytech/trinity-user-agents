import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing

/// The five-rule ladder over a stub scope: what a domain established is stated directly, so every test
/// is about ordering, mortality gating and the body search — the parts that are true of any transaction.
@Suite("Completion Ladder")
struct CompletionLadderTests {
    private let view = StubPinnedChainView(finalized: .fixture(150), best: .fixture(200))
    private let ladder = CompletionLadder()

    // MARK: Rule 0 — recorded inclusion

    @Test("Rule 0 finalizes when the recorded block is canonical and at or below the finalized head")
    func rule0FinalizesCanonicalAtOrBelowFinalized() async throws {
        let tx = entry(successDetectedAt: .fixture(120))

        let outcome = await evaluate(tx, scope: .unknown, recordedStillCanonical: true)

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .finalizedSuccess)
        #expect(verdict.successDetectedAt == .fixture(120))
    }

    @Test("Rule 0 holds at pendingSuccess while the recorded block is above the finalized head")
    func rule0HoldsPendingSuccessAboveFinalized() async throws {
        let tx = entry(successDetectedAt: .fixture(160))

        let outcome = await evaluate(tx, scope: .unknown, recordedStillCanonical: true)

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .pendingSuccess)
        #expect(verdict.successDetectedAt == .fixture(160))
    }

    @Test("Rule 0 re-records the finalized head when the record is gone but completion is visible at F")
    func rule0RecordGoneCompletedAtFinalized() async throws {
        let tx = entry(successDetectedAt: .fixture(120))

        let outcome = await evaluate(
            tx,
            scope: StubPassScope(completedAtFinalized: [tx.id], completedAtBest: [tx.id]),
            recordedStillCanonical: false
        )

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .finalizedSuccess)
        #expect(verdict.successDetectedAt == view.finalizedHead)
    }

    @Test("Rule 0 re-records the best head when the record is gone but completion is still visible at B")
    func rule0RecordGoneCompletedAtBest() async throws {
        let tx = entry(successDetectedAt: .fixture(120))

        let outcome = await evaluate(
            tx,
            scope: StubPassScope(completedAtBest: [tx.id]),
            recordedStillCanonical: false
        )

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .pendingSuccess)
        #expect(verdict.successDetectedAt == view.bestHead)
    }

    @Test("Rule 0 demotes to pending and clears the record when nothing is visible any more")
    func rule0RecordGoneDemotes() async throws {
        let tx = entry(successDetectedAt: .fixture(120))

        let outcome = await evaluate(tx, scope: .unknown, recordedStillCanonical: false)

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .pending)
        #expect(verdict.successDetectedAt == nil)
    }

    @Test("Rule 0 leaves the transaction undecided when the canonicality read failed")
    func rule0UndecidedWhenCanonicalityUnread() async {
        let tx = entry(successDetectedAt: .fixture(120))

        let outcome = await evaluate(tx, scope: .unknown, recordedStillCanonical: nil)

        #expect(outcome == .undecided)
        #expect(view.searchedHashes.isEmpty)
    }

    // MARK: Rules 1 and 2 — completion visible

    @Test("Rule 1 wins over Rule 2 on the same evidence")
    func rule1WinsOverRule2() async throws {
        let tx = entry()

        let outcome = await evaluate(tx, scope: StubPassScope(completedAtFinalized: [tx.id], completedAtBest: [tx.id]))

        #expect(outcome.verdict?.status == .finalizedSuccess)
    }

    @Test("Rule 1 writes no record of its own: the search is the only source of a finalized block")
    func rule1WritesNoRecord() async throws {
        let tx = entry()

        let outcome = await evaluate(tx, scope: StubPassScope(completedAtFinalized: [tx.id]))

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .finalizedSuccess)
        #expect(verdict.successDetectedAt == nil)
        #expect(view.searchedHashes.isEmpty)
    }

    @Test("A record aborts before Rule 1 while its canonicality is unknown, even with completion visible")
    func recordUnreadAbortsBeforeRule1() async {
        let tx = entry(successDetectedAt: .fixture(140))

        let outcome = await evaluate(
            tx,
            scope: StubPassScope(completedAtFinalized: [tx.id]),
            recordedStillCanonical: nil
        )

        #expect(outcome == .undecided)
    }

    @Test("Rule 2 records the best head so the domain keeps optimistic selectability")
    func rule2RecordsBestHead() async throws {
        let tx = entry()

        let outcome = await evaluate(tx, scope: StubPassScope(completedAtBest: [tx.id]))

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .pendingSuccess)
        #expect(verdict.successDetectedAt == view.bestHead)
    }

    // MARK: Rules 3 and 4 — proven not completed

    @Test("Rule 3 fails the transaction when proven not completed at F after mortality")
    func rule3FailsAfterMortality() async throws {
        let tx = entry(checkpointNumber: 50, mortality: 60)
        // mortalityEnd = 110 < finalized 150

        let outcome = await evaluate(tx, scope: StubPassScope(notCompletedAtFinalized: [tx.id]))

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .failure)
        #expect(verdict.successDetectedAt == nil)
    }

    @Test("Rule 3 does not fire before mortality has expired")
    func rule3DoesNotFireBeforeMortality() async throws {
        let tx = entry(checkpointNumber: 100, mortality: 60)
        // mortalityEnd = 160 >= finalized 150

        let outcome = await evaluate(
            tx,
            scope: StubPassScope(notCompletedAtFinalized: [tx.id], notCompletedAtBest: [tx.id])
        )

        #expect(outcome.verdict?.status == .pending)
        #expect(view.searchedHashes.isEmpty)
    }

    @Test("Rule 4 holds a transaction pending inside the window without searching block bodies")
    func rule4ShortCircuitsSearchInsideWindow() async throws {
        let tx = entry(checkpointNumber: 100, mortality: 60)

        let outcome = await evaluate(tx, scope: StubPassScope(notCompletedAtBest: [tx.id]))

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .pending)
        #expect(verdict.successDetectedAt == nil)
        #expect(view.searchedHashes.isEmpty)
    }

    @Test("Rule 4 holds only inside the window: past mortality the search decides")
    func rule4HoldsOnlyInsideWindow() async throws {
        let tx = entry(checkpointNumber: 50, mortality: 60)
        try view.setBodySearchResponse(#require(tx.txHash), to: .notFoundWindowComplete)

        let outcome = await evaluate(tx, scope: StubPassScope(notCompletedAtBest: [tx.id]))

        #expect(outcome.verdict?.status == .failure)
        #expect(try view.searchedHashes == [#require(tx.txHash)])
    }

    // MARK: Rule 5 — body search

    @Test("Rule 5 finalizes on a successful dispatch in the searched block")
    func rule5FinalizesOnSuccess() async throws {
        let tx = entry()
        try view.setBodySearchResponse(#require(tx.txHash), to: .foundSucceeded(.fixture(120)))

        let outcome = await evaluate(tx, scope: .unknown)

        let verdict = try #require(outcome.verdict)
        #expect(verdict.status == .finalizedSuccess)
        #expect(verdict.successDetectedAt == .fixture(120))
    }

    @Test("Rule 5 fails on a failed dispatch — inclusion is not success")
    func rule5FailsOnFailedDispatch() async throws {
        let tx = entry()
        try view.setBodySearchResponse(#require(tx.txHash), to: .foundFailed(.fixture(120), reason: "Test.Failed"))

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .failure)
    }

    @Test("Rule 5 leaves the transaction pending when the outcome could not be read")
    func rule5PendingWhenOutcomeUnreadable() async throws {
        let tx = entry()
        try view.setBodySearchResponse(#require(tx.txHash), to: .foundOutcomeUnreadable(.fixture(120)))

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .pending)
    }

    @Test("Rule 5 fails only once the whole window was read and mortality has expired")
    func rule5FailsWhenWholeWindowReadAndMortalityExpired() async throws {
        let tx = entry(checkpointNumber: 50, mortality: 60)
        try view.setBodySearchResponse(#require(tx.txHash), to: .notFoundWindowComplete)

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .failure)
    }

    @Test("Rule 5 keeps an absent transaction pending while its window is still open")
    func rule5PendingWhileWindowOpen() async throws {
        let tx = entry(checkpointNumber: 100, mortality: 60)
        try view.setBodySearchResponse(#require(tx.txHash), to: .notFoundWindowComplete)

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .pending)
    }

    @Test("A partially read window leaves the transaction pending")
    func rule5PartialWindowStaysPending() async throws {
        let tx = entry(checkpointNumber: 50, mortality: 60)
        try view.setBodySearchResponse(#require(tx.txHash), to: .incomplete)

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .pending)
    }

    @Test("The search window runs from the checkpoint to the finalized head, never past mortality")
    func searchWindowBoundedAtFinalizedAndMortality() async throws {
        let inWindow = entry(checkpointNumber: 100, mortality: 60)
        let expired = entry(checkpointNumber: 50, mortality: 60)

        _ = await evaluate(inWindow, scope: .unknown)
        _ = await evaluate(expired, scope: .unknown)

        #expect(try view.searchedWindow(for: #require(inWindow.txHash)) == 100 ... 150)
        #expect(try view.searchedWindow(for: #require(expired.txHash)) == 50 ... 110)
    }

    @Test("A checkpoint above the finalized head has nothing to search yet and stays pending")
    func emptyWindowStaysPending() async throws {
        let tx = entry(checkpointNumber: 180, mortality: 60)

        let outcome = await evaluate(tx, scope: .unknown)

        #expect(outcome.verdict?.status == .pending)
        #expect(view.searchedHashes.isEmpty)
    }
}

// MARK: - Harness

private extension CompletionLadderTests {
    func entry(
        checkpointNumber: UInt32 = 100,
        mortality: UInt32 = 60,
        successDetectedAt: BlockRef? = nil
    ) -> DurableTxEntry {
        .fixture(checkpoint: .fixture(checkpointNumber), mortality: mortality, successDetectedAt: successDetectedAt)
    }

    func evaluate(
        _ tx: DurableTxEntry,
        scope: StubPassScope,
        recordedStillCanonical: Bool? = nil
    ) async -> RuleOutcome {
        await ladder.evaluate(tx, scope: scope, view: view, recordedStillCanonical: recordedStillCanonical)
    }
}
