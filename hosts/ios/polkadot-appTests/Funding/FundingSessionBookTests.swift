import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

private let provider = try! ProductId("provider.dot")

/// An ended session is acknowledged to the core once the user has been shown
/// it, and again later when the core refused the acknowledgement.
struct FundingSessionBookTests {
    @Test
    func anEndedSessionIsAcknowledgedOnce() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent"])
    }

    // The core hands over an ending it has compacted as its summary only;
    // the user is told of it in history, and it is acknowledged once.
    @Test
    func anEndingHandedOverAsItsSummaryIsAcknowledgedOnce() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime()
        book.record(intent: "intent", session: nil, progress: nil, ended: true)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent"])
    }

    @Test
    func aSessionInFlightIsNotAcknowledged() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: .open), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged.isEmpty)
    }

    @Test
    func aFailedAcknowledgementIsTriedAgain() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime(failsFirstAcknowledgement: true)
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent", "intent"])
    }
}

private let openedAtMs: UInt64 = 1_700_000_000_000

private func delivered(_ settledAtMs: UInt64) -> FundingStage {
    .delivered(credited: "50000000", settledAtMs: openedAtMs + settledAtMs)
}

private func session(_ intent: String, stage: FundingStage) -> FundingSession {
    FundingSession(
        intent: intent,
        ownerProductId: nil,
        direction: .in,
        amount: "50000000",
        stage: stage,
        openedAtMs: openedAtMs,
        deadlineMs: openedAtMs + 86_400_000,
        acknowledged: false,
        providerId: provider,
        assignedAtMs: openedAtMs,
        cancel: nil,
        updates: [],
        started: [],
        choice: FundingChoice(
            quote: FundingQuote(
                quoteId: "quote",
                sendAmount: "50000000",
                receiveAmount: "50000000",
                providerFee: "0",
                networkFee: "0",
                etaSecs: nil,
                expiresAt: nil,
                assetDecimals: 2
            ),
            rail: .card,
            asset: "EUR",
            network: nil
        ),
        refund: nil,
        detail: nil
    )
}

private final class StubFundingRuntime: FundingRuntime, @unchecked Sendable {
    private struct Refused: Error {}

    private let state: OSAllocatedUnfairLock<(failNext: Bool, acknowledged: [String])>
    var acknowledged: [String] { state.withLock { $0.acknowledged } }

    init(failsFirstAcknowledgement: Bool = false) {
        state = OSAllocatedUnfairLock(initialState: (failsFirstAcknowledgement, []))
    }

    func selectFundingOptions(intent _: String, selection _: FundingSelection) {}
    func getFundingQuote(intent _: String, ask _: FundingQuoteAsk) -> UInt64 { 0 }
    func stopFundingQuotes(intent _: String) {}
    func selectFundingProvider(
        intent _: String,
        providerId _: ProductId,
        quoteId _: String
    ) async throws -> Bool { true }
    func cancelFunding(intent _: String) async throws -> CancelOutcome { .ended }
    func confirmFundingCancel(intent _: String, terms _: CancelTerms?) async throws -> CancelConfirmation { .declined }
    func fundingHistory() -> [FundingSessionSummary] { [] }
    func openFunding(direction _: FundingDirection, amount _: U128?) async throws -> String? { nil }

    func acknowledgeFundingSession(intent: String) async throws -> Bool {
        let failed = state.withLock { state in
            state.acknowledged.append(intent)
            defer { state.failNext = false }
            return state.failNext
        }
        if failed { throw Refused() }
        return true
    }
}
