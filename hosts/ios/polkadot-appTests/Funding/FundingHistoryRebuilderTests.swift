import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A lost history comes back from the sessions the core recorded, without
/// overwriting what the store still has.
@MainActor
struct FundingHistoryRebuilderTests {
    @Test
    func aSummaryReadsAsTheOutcomeItEndedWith() throws {
        let delivered = try #require(FundingRecord(summary: summary(
            "in",
            stage: .delivered(credited: "90", settledAtMs: 2_000)
        )))
        let released = try #require(
            FundingRecord(summary: summary("out", direction: .out, stage: .released(debited: "80", settledAtMs: 2_000)))
        )
        let refunded = try #require(
            FundingRecord(summary: summary("refunded", stage: .failed(reason: .refunded, settledAtMs: 2_000)))
        )
        let declined = try #require(
            FundingRecord(summary: summary("declined", stage: .failed(reason: .declined, settledAtMs: 2_000)))
        )

        #expect(delivered.outcome == .delivered && delivered.settledAmount == "90")
        #expect(released.outcome == .released && released.settledAmount == "80" && released.direction == .out)
        #expect(refunded.outcome == .refunded && refunded.settledAmount == nil)
        #expect(declined.outcome == .failed(.declined))
        #expect(delivered.requestedAmount == "100" && delivered.providerId == nil && delivered.rail == nil)
        #expect(delivered.openedAt == Date(milliseconds: 1_000) && delivered.settledAt == Date(milliseconds: 2_000))
        #expect(FundingRecord(summary: summary("open", stage: .open)) == nil)
    }

    @Test
    func onlyTheSessionsTheStoreLacksAreWrittenAndOnlyOnce() async throws {
        let store = RecordingHistoryStore()
        let kept = try #require(FundingRecord(summary: summary(
            "kept",
            stage: .delivered(credited: "1", settledAtMs: 2_000)
        )))
        try await store.save(kept)
        let rebuilder = FundingHistoryRebuilder(store: store)
        let history = [
            summary("kept", stage: .delivered(credited: "99", settledAtMs: 2_000)),
            summary("lost", stage: .delivered(credited: "50", settledAtMs: 2_000))
        ]
        var reads = 0

        await rebuilder.rebuildOnce { reads += 1; return history }
        await rebuilder.rebuildOnce { reads += 1; return history }

        let records = try await store.records().sorted { $0.intent < $1.intent }
        #expect(records.map(\.intent) == ["kept", "lost"])
        #expect(records.first?.settledAmount == "1")
        #expect(reads == 1)
    }
}

private extension FundingHistoryRebuilderTests {
    func summary(_ intent: String, direction: FundingDirection = .in, stage: FundingStage) -> FundingSessionSummary {
        FundingSessionSummary(
            intent: intent,
            ownerProductId: nil,
            direction: direction,
            amount: "100",
            stage: stage,
            openedAtMs: 1_000
        )
    }
}

private actor RecordingHistoryStore: FundingHistoryStoring {
    private var rows: [String: FundingRecord] = [:]

    func records() async throws -> [FundingRecord] { Array(rows.values) }

    func save(_ record: FundingRecord) async throws { rows[record.intent] = record }
}
