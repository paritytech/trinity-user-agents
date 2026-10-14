import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing

/// Pass-level behaviour the ladder does not cover: which transactions a pass touches, how it pins, the two
/// rounds per domain, and the compare-and-set write guard. Runs the real `DurableRecoveryPass` over the
/// in-memory ledger and a stub view.
@Suite("Recovery Pass")
struct RecoveryPassTests {
    private let store = InMemoryDurableTxRepository()
    private let view = StubPinnedChainView(finalized: .fixture(150), best: .fixture(200))
    private let owned = DurableTxOwnershipSet()
    private let oracles = TxCompletionOracleRegistry()

    @Test("A settled ledger does not pin a chain view")
    func settledLedgerDoesNotPin() async throws {
        store.insert(.fixture(status: .finalizedSuccess))
        oracles.register(StubCompletionOracle(), for: .test)

        await pass().run()

        #expect(view.pinCount == 0)
    }

    @Test("A domain with no registered oracle is left alone")
    func unregisteredDomainSkipped() async throws {
        let tx = DurableTxEntry.fixture(domainId: TxDomainId("orphan"))
        store.insert(tx)
        try view.setBodySearchResponse(#require(tx.txHash), to: .foundSucceeded(.fixture(120)))

        await pass().run()

        #expect(view.pinCount == 0)
        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .pending)
    }

    @Test("A submission-owned transaction gets no verdict")
    func submissionOwnedSkipped() async throws {
        let tx = DurableTxEntry.fixture()
        store.insert(tx)
        try owned.take(tx.id, txHash: #require(tx.txHash))
        oracles.register(
            StubCompletionOracle { txs, _ in StubPassScope(completedAtFinalized: Set(txs.map(\.id))) },
            for: .test
        )

        await pass().run()

        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .pending)
    }

    @Test("A decided transaction is written through the oracle's answer")
    func oracleAnswerIsWritten() async throws {
        let tx = DurableTxEntry.fixture()
        store.insert(tx)
        oracles.register(
            StubCompletionOracle { txs, _ in StubPassScope(completedAtFinalized: Set(txs.map(\.id))) },
            for: .test
        )

        await pass().run()

        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .finalizedSuccess)
    }

    @Test("A second round lets a domain see what the first round wrote")
    func secondRoundSeesFirstRoundWrites() async throws {
        // A predecessor whose completion the domain can only infer from its successor's status — the
        // coinage "a finalized consumer proves the minter ran" case, modelled generically.
        let predecessor = DurableTxEntry.fixture()
        let successor = DurableTxEntry.fixture()
        store.insert(predecessor)
        store.insert(successor)

        oracles.register(StubCompletionOracle { _, ledger in
            var completed: Set<DurableTxId> = [successor.id]
            if ledger.status(of: successor.id) == .finalizedSuccess {
                completed.insert(predecessor.id)
            }
            return StubPassScope(completedAtFinalized: completed)
        }, for: .test)

        await pass().run()

        let promoted = try await store.getEntry(id: predecessor.id)
        #expect(promoted?.status == .finalizedSuccess)
    }

    @Test("Two domains on one chain share a single pinned view")
    func onePinPerChain() async throws {
        store.insert(.fixture(domainId: TxDomainId("a")))
        store.insert(.fixture(domainId: TxDomainId("b")))
        oracles.register(StubCompletionOracle(chainId: "shared"), for: TxDomainId("a"))
        oracles.register(StubCompletionOracle(chainId: "shared"), for: TxDomainId("b"))

        await pass().run()

        #expect(view.pinCount == 1)
    }

    @Test("A pass that cannot pin writes nothing")
    func pinFailureWritesNothing() async throws {
        let tx = DurableTxEntry.fixture()
        store.insert(tx)
        view.setPinFails(true)
        oracles.register(
            StubCompletionOracle { txs, _ in StubPassScope(completedAtFinalized: Set(txs.map(\.id))) },
            for: .test
        )

        await pass().run()

        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .pending)
    }

    @Test("An oracle that fails to open leaves its domain untouched this pass")
    func oracleFailureWritesNothing() async throws {
        let tx = DurableTxEntry.fixture()
        store.insert(tx)
        try view.setBodySearchResponse(#require(tx.txHash), to: .foundSucceeded(.fixture(120)))
        oracles.register(StubCompletionOracle { _, _ in throw ChainReadFailure(message: "down") }, for: .test)

        await pass().run()

        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .pending)
    }

    @Test("Recorded canonicality is read from the view, once per recorded height")
    func recordedCanonicalityReadFromView() async throws {
        let recorded = BlockRef.fixture(120)
        let tx = DurableTxEntry.fixture(successDetectedAt: recorded, status: .pendingSuccess)
        store.insert(tx)
        view.setBlockHash(Data([9, 9]), forNumber: 120)
        oracles.register(StubCompletionOracle(), for: .test)

        await pass().run()

        // The recorded block was reorged out and nothing is visible: demoted and cleared.
        let fetched = try await store.getEntry(id: tx.id)
        #expect(fetched?.status == .pending)
        #expect(fetched?.successDetectedAt == nil)
    }

    // MARK: - Compare-and-set write guard

    @Test("updateTxStatus writes only while the observed status still holds")
    func compareAndSetRejectsMovedStatus() async throws {
        let entry = DurableTxEntry.fixture()
        store.insert(entry)
        try store.forceStatus(entry.id, to: .pendingSuccess)

        let wrote = try await store.updateTxStatus(
            for: entry.id,
            expectedCurrentStatus: .pending,
            expectedTxHash: #require(entry.txHash),
            verdict: Verdict(status: .finalizedSuccess, successDetectedAt: nil)
        )

        #expect(wrote == false)
        let fetched = try await store.getEntry(id: entry.id)
        #expect(fetched?.status == .pendingSuccess)
    }

    @Test("updateTxStatus does not overwrite a terminal entry")
    func compareAndSetLeavesTerminalUntouched() async throws {
        let entry = DurableTxEntry.fixture(status: .finalizedSuccess)
        store.insert(entry)

        let wrote = try await store.updateTxStatus(
            for: entry.id,
            expectedCurrentStatus: .finalizedSuccess,
            expectedTxHash: #require(entry.txHash),
            verdict: Verdict(status: .failure, successDetectedAt: nil)
        )

        #expect(wrote == false)
        let fetched = try await store.getEntry(id: entry.id)
        #expect(fetched?.status == .finalizedSuccess)
    }
}

private extension RecoveryPassTests {
    func pass() -> DurableRecoveryPass {
        DurableRecoveryPass(
            store: store,
            chainFactory: view,
            owned: owned,
            oracles: oracles,
            verdictWriter: DurableVerdictWriter(
                store: store,
                policies: DurableSubmissionPolicyRegistry(),
                logger: nil
            ),
            logger: nil
        )
    }
}
