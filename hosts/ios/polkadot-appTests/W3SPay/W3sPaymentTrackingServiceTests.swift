import AsyncExtensions
import Coinage
import Foundation
import SubstrateOperation
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("W3sPaymentTrackingService: follows the payment's export to the merchant's claim")
struct W3sPaymentTrackingServiceTests {
    @Test("coins on chain mark .sent, the merchant's claim then marks .claimed")
    func onChainThenClaimed() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let wallet = MockCoinageService(paymentSnapshots: [
            [],
            [Self.coin(.awaitingClaim)],
            [Self.coin(.claimed(finalized: false))],
            [Self.coin(.claimed(finalized: true))]
        ])
        let service = W3sPaymentTrackingService(historyStore: store, coinageService: wallet)

        await service.setup()
        let updates = await Self.poll(store) { $0.contains(.claimed) }
        await service.throttle()

        #expect(updates == [.sent, .claimed])
        #expect(try wallet.paymentSubscriptions == [W3sPaymentRecord.exportId(paymentId: "PAY-1")])
    }

    @Test("coins the merchant claimed before they were seen waiting go straight to .claimed")
    func alreadyClaimedSkipsSent() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let wallet = MockCoinageService(paymentSnapshots: [[Self.coin(.claimed(finalized: true))]])
        let service = W3sPaymentTrackingService(historyStore: store, coinageService: wallet)

        await service.setup()
        let updates = await Self.poll(store) { $0.contains(.claimed) }
        await service.throttle()

        #expect(updates == [.claimed])
    }

    @Test("coins that were never minted mark .failed, and never .sent or .claimed")
    func neverMintedMarksFailed() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .submitted)])
        let wallet = MockCoinageService(paymentSnapshots: [[Self.coin(.detecting)], [Self.coin(.failed)]])
        let service = W3sPaymentTrackingService(historyStore: store, coinageService: wallet)

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

    @Test("a settled payment is not followed again")
    func settledPaymentIsNotFollowed() async throws {
        let store = TrackingHistoryStoreSpy(records: [Self.makeRecord(status: .claimed)])
        let wallet = MockCoinageService(paymentSnapshots: [[Self.coin(.awaitingClaim)]])
        let service = W3sPaymentTrackingService(historyStore: store, coinageService: wallet)

        await service.setup()
        _ = await Self.poll(store) { !$0.isEmpty }
        await service.throttle()

        #expect(wallet.paymentSubscriptions.isEmpty)
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
            memo: TransferMemo(entries: [Data(repeating: 0x01, count: 64)], totalValue: 1),
            submittedAtBlock: 0,
            createdAt: now,
            updatedAt: now,
            status: status
        )
    }

    static func coin(_ status: CoinageTransferStatus) -> CoinagePaymentState {
        CoinagePaymentState(account: Data(repeating: 0x02, count: 32), value: 1, status: status)
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
