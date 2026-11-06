import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// An ended session is acknowledged to the core once the user has been shown
/// it, and again later when the core refused the acknowledgement.
struct FundingSessionBookTests {
    @Test
    func anEndedSessionIsAcknowledgedOnce() async {
        let book = FundingSessionBook(logger: MockLogger())
        let runtime = MockFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent"])
    }

    // The core hands over an ending it has compacted as its summary only;
    // the user is told of it in history, and it is acknowledged once.
    @Test
    func anEndingHandedOverAsItsSummaryIsAcknowledgedOnce() async {
        let book = FundingSessionBook(logger: MockLogger())
        let runtime = MockFundingRuntime()
        book.record(intent: "intent", session: nil, progress: nil, ended: true)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent"])
    }

    @Test
    func aSessionInFlightIsNotAcknowledged() async {
        let book = FundingSessionBook(logger: MockLogger())
        let runtime = MockFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: .open), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged.isEmpty)
    }

    @Test
    func aFailedAcknowledgementIsTriedAgain() async {
        let book = FundingSessionBook(logger: MockLogger())
        let runtime = MockFundingRuntime(failsFirstAcknowledgement: true)
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil, ended: false)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent", "intent"])
    }
}

private extension FundingSessionBookTests {
    static let openedAtMs: UInt64 = 1_700_000_000_000

    func delivered(_ settledAtMs: UInt64) -> FundingStage {
        .delivered(credited: "50000000", settledAtMs: Self.openedAtMs + settledAtMs)
    }

    func session(_ intent: String, stage: FundingStage) -> FundingSession {
        let openedAtMs = Self.openedAtMs
        return FundingSession(
            intent: intent,
            ownerProductId: nil,
            direction: .in,
            amount: "50000000",
            stage: stage,
            openedAtMs: openedAtMs,
            deadlineMs: openedAtMs + 86_400_000,
            acknowledged: false,
            providerId: try? ProductId("provider.dot"),
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
}
