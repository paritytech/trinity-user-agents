import AsyncExtensions
import Coinage
import Foundation
import SubstrateOperation
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("W3sPaymentTrackingService: tolerates already-claimed coins, keeps lost-send failure")
struct W3sPaymentTrackingServiceTests {
    @Test(".onChain confirmation marks .sent then waits for claim -> .claimed")
    func onChainConfirmationMarksSentThenClaimed() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let verifier = SendVerifierStub(sendResult: .success(.onChain), claimError: nil)
        let service = W3sPaymentTrackingService(
            historyStore: store,
            sendVerifier: verifier,
            blockInfoProvider: BlockInfoProviderStub()
        )

        await service.setup()
        let updates = await Self.poll(store) { $0.contains(.sent) && $0.contains(.claimed) }
        await service.throttle()

        #expect(updates == [.sent, .claimed])
        #expect(await verifier.claimWasAwaited)
    }

    @Test(".alreadyClaimed confirmation jumps straight to .claimed, skips the claim wait")
    func alreadyClaimedConfirmationSkipsClaimWait() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let verifier = SendVerifierStub(sendResult: .success(.alreadyClaimed), claimError: nil)
        let service = W3sPaymentTrackingService(
            historyStore: store,
            sendVerifier: verifier,
            blockInfoProvider: BlockInfoProviderStub()
        )

        await service.setup()
        let updates = await Self.poll(store) { $0.contains(.claimed) }
        await service.throttle()

        #expect(updates == [.claimed])
        #expect(await verifier.claimWasAwaited == false, "claim wait must be skipped")
    }

    @Test("Genuine lost send (throws) with expired window marks .failed")
    func lostSendWithExpiredWindowMarksFailed() async throws {
        // submittedAtBlock 0, finalized far ahead -> window provably expired.
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let verifier = SendVerifierStub(sendResult: .failure(StubError.boom), claimError: nil)
        let service = W3sPaymentTrackingService(
            historyStore: store,
            sendVerifier: verifier,
            blockInfoProvider: BlockInfoProviderStub(finalized: 1_000)
        )

        await service.setup()
        let updates = await Self.poll(store) { calls in
            calls.contains { if case .failed = $0 { return true }; return false }
        }
        await service.throttle()

        let hasFailed = updates.contains { if case .failed = $0 { return true }; return false }
        #expect(hasFailed)
        #expect(updates.contains(.sent) == false)
        #expect(updates.contains(.claimed) == false)
    }
}

// MARK: - Helpers

private extension W3sPaymentTrackingServiceTests {
    static func makeRecord(status: W3sPaymentRecord.Status) -> W3sPaymentRecord {
        let now = Date()
        return W3sPaymentRecord(
            paymentId: "PAY-1",
            recipientTopic: Data(repeating: 0xAB, count: 32),
            merchantName: "Merchant",
            merchantPublicKey: Data(repeating: 0xCD, count: 33),
            amountString: "1.00",
            chainAssetId: "CASH",
            memo: TransferMemo(entries: [Data(repeating: 0x01, count: 32)], totalValue: 1),
            submittedAtBlock: 0,
            createdAt: now,
            updatedAt: now,
            status: status
        )
    }

    /// Polls the spy until `predicate` holds or a short deadline elapses, then returns the updates.
    static func poll(
        _ store: TrackingHistoryStoreSpy,
        until predicate: @Sendable ([W3sPaymentRecord.Status]) -> Bool
    ) async -> [W3sPaymentRecord.Status] {
        for _ in 0 ..< 100 {
            let updates = await store.recordedUpdates()
            if predicate(updates) { return updates }
            try? await Task.sleep(nanoseconds: 30_000_000) // 30ms
        }
        return await store.recordedUpdates()
    }
}

private enum StubError: Error { case boom }

// MARK: - Test doubles

private actor TrackingHistoryStoreSpy: W3sPaymentHistoryStoring {
    private nonisolated let records: [W3sPaymentRecord]
    private var updates: [W3sPaymentRecord.Status] = []

    init(records: [W3sPaymentRecord]) {
        self.records = records
    }

    func save(_: W3sPaymentRecord) async throws {}

    func updateStatus(paymentId _: String, status: W3sPaymentRecord.Status) async throws {
        updates.append(status)
    }

    func fetch(byId _: String) async throws -> W3sPaymentRecord? { nil }

    nonisolated func observeAll() -> AnyAsyncSequence<[W3sPaymentRecord]> {
        let batch = records
        return AsyncStream<[W3sPaymentRecord]> { continuation in
            continuation.yield(batch)
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    nonisolated func observeRecord(paymentId _: String) -> AnyAsyncSequence<W3sPaymentRecord?> {
        fatalError("Not needed for this test")
    }

    func recordedUpdates() -> [W3sPaymentRecord.Status] { updates }
}

private actor SendVerifierStub: TransferSendVerifying {
    private let sendResult: Result<SendConfirmation, Error>
    private let claimError: Error?
    private(set) var claimWasAwaited = false

    init(sendResult: Result<SendConfirmation, Error>, claimError: Error?) {
        self.sendResult = sendResult
        self.claimError = claimError
    }

    func awaitSendOnChain(memo _: TransferMemo, blockTimeout _: UInt32) async throws {}

    func awaitClaimOnChain(memo _: TransferMemo, blockTimeout _: UInt32) async throws {
        claimWasAwaited = true
        if let claimError { throw claimError }
    }

    func awaitSendOrClaimed(
        memo _: TransferMemo,
        anchorBlock _: BlockNumber?,
        blockTimeout _: UInt32
    ) async throws -> SendConfirmation {
        try sendResult.get()
    }
}

private struct BlockInfoProviderStub: BlockInfoProviding {
    var finalized: BlockNumber = 0

    func fetchCurrent() async throws -> BlockNumber { finalized }
    func fetchCurrentHash() async throws -> BlockHashData { Data() }
    func fetchFinalized() async throws -> BlockNumber { finalized }
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
