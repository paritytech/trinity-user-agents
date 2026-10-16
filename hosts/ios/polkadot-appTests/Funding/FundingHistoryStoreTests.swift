import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The funding history keeps every outcome a session can end with, and
/// writing a session again updates its row.
struct FundingHistoryStoreTests {
    @Test
    func everyOutcomeReadsBackAsItWasWritten() async throws {
        let store = CoreDataFundingHistoryStore(storageFacade: UserDataStorageTestFacade())
        let written = [
            record("delivered", outcome: .delivered),
            record("released", direction: .out, outcome: .released, payout: .paidOut),
            record(
                "payout",
                direction: .out,
                outcome: .released,
                payout: .failed(reason: "bank closed"),
                refunded: "4"
            ),
            record("cancelled", direction: .out, outcome: .cancelled, refunded: "95"),
            record("refunded", outcome: .refunded),
            record("other", outcome: .failed(.other(code: "kyc", message: "id expired"))),
            record("declined", outcome: .failed(.declined))
        ]

        for record in written {
            try await store.save(record)
        }

        let read = try await store.records().sorted { $0.intent < $1.intent }
        #expect(read == written.sorted { $0.intent < $1.intent })
    }

    @Test
    func writingASessionAgainUpdatesItsRow() async throws {
        let store = CoreDataFundingHistoryStore(storageFacade: UserDataStorageTestFacade())

        try await store.save(record("one", direction: .out, outcome: .released))
        try await store.save(record("one", direction: .out, outcome: .released, payout: .paidOut))

        #expect(try await store.records() == [record("one", direction: .out, outcome: .released, payout: .paidOut)])
    }
}

private extension FundingHistoryStoreTests {
    func record(
        _ intent: String,
        direction: FundingDirection = .in,
        outcome: FundingRecord.Outcome,
        payout: FundingRecord.Payout? = nil,
        refunded: U128? = nil
    ) -> FundingRecord {
        FundingRecord(
            intent: intent,
            direction: direction,
            rail: .card,
            asset: "EUR",
            providerId: "provider",
            requestedAmount: "100",
            settledAmount: "100",
            paidAmount: "10200",
            paidAsset: "EUR",
            paidDecimals: 2,
            paidNetwork: nil,
            outcome: outcome,
            payout: payout,
            refundedAmount: refunded,
            transactionId: "tx",
            reference: "ref",
            openedAt: Date(timeIntervalSince1970: 1_700_000_000),
            settledAt: Date(timeIntervalSince1970: 1_700_000_600)
        )
    }
}
