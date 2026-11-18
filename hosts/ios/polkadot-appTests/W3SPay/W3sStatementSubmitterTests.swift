import AsyncExtensions
import BackgroundExecution
import Coinage
import Foundation
import MessageExchangeKit
import StatementStore
import SubstrateOperation
import SubstrateSdk
import Testing

@testable import polkadot_app

/// The order the submitter did things in. A reference box because the commit closure and the
/// statement stub are called on the submitter's own task.
private final class EventLog: @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [String] = []

    var events: [String] {
        lock.lock()
        defer { lock.unlock() }
        return recorded
    }

    func record(_ event: String) {
        lock.lock()
        recorded.append(event)
        lock.unlock()
    }
}

private struct CommitRefused: Error {}

@Suite("W3sStatementSubmitter: commits before the keys leave, never marks failed when the submit throws")
struct W3sStatementSubmitterTests {
    private func makeSubmitter(
        merchantKey: Data,
        historyStore: W3sPaymentHistoryStoreSpy,
        statements: StatementStoreSubmittingStub = StatementStoreSubmittingStub()
    ) throws -> W3sStatementSubmitter {
        try W3sStatementSubmitter(
            details: W3sPaymentDetails(
                paymentId: "TEST-HOOK",
                topic: Data(repeating: 0xAB, count: 32),
                merchantKey: merchantKey,
                merchantName: "TestMerchant",
                amountString: "1.00",
                chainAssetId: "CASH"
            ),
            wallet: MockWalletManager.mockedWallet(),
            statementStoreSubmitter: statements,
            historyStore: historyStore,
            blockInfoProvider: BlockInfoProviderStub(),
            priorityFactory: StatementPriorityFactoryStub(),
            backgroundExecutor: InlineBackgroundExecutor()
        )
    }

    @Test("the export is committed before the statement carries the keys away")
    func commitPrecedesTheStatement() async throws {
        // A statement out with its export uncommitted would leave the wallet counting coins the
        // merchant can claim.
        let log = EventLog()
        let submitter = try makeSubmitter(
            merchantKey: Data(repeating: 0x02, count: 32),
            historyStore: W3sPaymentHistoryStoreSpy(),
            statements: StatementStoreSubmittingStub(accepts: true, log: log)
        )

        try await submitter.sendTransfer(
            TransferMemo(entries: [], totalValue: 0),
            to: Data(),
            messageId: UUID().uuidString
        ) { log.record("commit") }

        #expect(log.events == ["commit", "statement"])
    }

    @Test("a refused commit keeps the statement from leaving")
    func refusedCommitSendsNothing() async throws {
        let log = EventLog()
        let submitter = try makeSubmitter(
            merchantKey: Data(repeating: 0x02, count: 32),
            historyStore: W3sPaymentHistoryStoreSpy(),
            statements: StatementStoreSubmittingStub(accepts: true, log: log)
        )

        await #expect(throws: CommitRefused.self) {
            try await submitter.sendTransfer(
                TransferMemo(entries: [], totalValue: 0),
                to: Data(),
                messageId: UUID().uuidString
            ) { throw CommitRefused() }
        }

        #expect(log.events.isEmpty)
    }

    @Test("Invalid merchantKey fails buildEnvelope; record stays pending, never failed")
    func sendTransferWithInvalidMerchantKeyDoesNotMarkFailed() async throws {
        // Wrong-length X25519 key -> buildEnvelope throws before makeSigner /
        // submitStatement are ever reached, exercising the catch path cheaply.
        let invalidMerchantKey = Data([0x00])
        let details = W3sPaymentDetails(
            paymentId: "TEST-001",
            topic: Data(repeating: 0xAB, count: 32),
            merchantKey: invalidMerchantKey,
            merchantName: "TestMerchant",
            amountString: "1.00",
            chainAssetId: "CASH"
        )

        let historyStoreSpy = W3sPaymentHistoryStoreSpy()

        let submitter = try W3sStatementSubmitter(
            details: details,
            wallet: MockWalletManager.mockedWallet(),
            statementStoreSubmitter: StatementStoreSubmittingStub(),
            historyStore: historyStoreSpy,
            blockInfoProvider: BlockInfoProviderStub(),
            priorityFactory: StatementPriorityFactoryStub(),
            backgroundExecutor: InlineBackgroundExecutor()
        )

        let memo = TransferMemo(entries: [], totalValue: 0)

        // Submit path throws (buildEnvelope fails on the invalid key).
        await #expect(throws: Error.self) {
            try await submitter.sendTransfer(memo, to: Data(), messageId: UUID().uuidString) {}
        }

        // The pending record is saved exactly once before the failure.
        let saveCalls = await historyStoreSpy.getSaveCalls()
        #expect(saveCalls.count == 1)
        #expect(saveCalls.first?.record.status == .pending, "Record should be saved as pending")

        // submit-failure path must NOT write `.failed` — coins may already
        // be on-chain, so the tracker reconciles from chain truth instead.
        let updateStatusCalls = await historyStoreSpy.getUpdateStatusCalls()
        let failedUpdates = updateStatusCalls.filter { call in
            if case .failed = call.status { return true }
            return false
        }
        #expect(failedUpdates.isEmpty, "No .failed status updates should be recorded")
        #expect(updateStatusCalls.isEmpty, "No updateStatus calls at all on this path")
    }
}

// MARK: - Test Mocks

/// Spy that records every save / updateStatus call.
private actor W3sPaymentHistoryStoreSpy: W3sPaymentHistoryStoring {
    struct SaveCall {
        let record: W3sPaymentRecord
    }

    struct UpdateStatusCall {
        let paymentId: String
        let status: W3sPaymentRecord.Status
    }

    private var saveCalls: [SaveCall] = []
    private var updateStatusCalls: [UpdateStatusCall] = []

    func save(_ record: W3sPaymentRecord) async throws {
        saveCalls.append(.init(record: record))
    }

    func updateStatus(paymentId: String, status: W3sPaymentRecord.Status) async throws {
        updateStatusCalls.append(.init(paymentId: paymentId, status: status))
    }

    func fetch(byId _: String) async throws -> W3sPaymentRecord? { nil }

    nonisolated func observeAll() -> AnyAsyncSequence<[W3sPaymentRecord]> {
        fatalError("Not needed for this test")
    }

    nonisolated func observeRecord(paymentId _: String) -> AnyAsyncSequence<W3sPaymentRecord?> {
        fatalError("Not needed for this test")
    }

    func getSaveCalls() -> [SaveCall] { saveCalls }
    func getUpdateStatusCalls() -> [UpdateStatusCall] { updateStatusCalls }
}

/// Returns fixed values; only `fetchFinalized()` is reached on this path.
private struct BlockInfoProviderStub: BlockInfoProviding {
    func fetchCurrent() async throws -> BlockNumber { 0 }
    func fetchCurrentHash() async throws -> BlockHashData { Data() }
    func fetchFinalized() async throws -> BlockNumber { 0 }
    func fetchFinalizedHash() async throws -> BlockHashData { Data() }
    func fetchBlockHash(_: BlockNumber) async throws -> BlockHashData { Data() }
    func fetchBlockNumber(byHash _: BlockHashData) async throws -> BlockNumber { 0 }
    func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
        fatalError("Not needed for this test")
    }

    func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> {
        fatalError("Not needed for this test")
    }
}

/// Never reached where buildEnvelope fails first; accepts the statement where it does not.
private struct StatementStoreSubmittingStub: StatementStoreSubmitting {
    var accepts = false
    var log: EventLog?

    func submitStatement(with _: StatementSubmitParametersBuilding) async throws {
        guard accepts else {
            fatalError("Should not be reached when buildEnvelope fails")
        }
        log?.record("statement")
    }
}

private struct StatementPriorityFactoryStub: StatementPriorityMaking {
    func makeTimestampPriority() -> UInt64 { UInt64(Date().timeIntervalSince1970) }
}
