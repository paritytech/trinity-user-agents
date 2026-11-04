import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How a session's band reads, and which ended sessions the history drops.
struct FundingWordingTests {
    @Test
    func bandLinesFollowTheCoreDocs() {
        let adding = FundingWording.band(.adding, direction: .in, rail: .card, asset: nil, amount: "$50 CASH")
        let retrying = FundingWording.band(.retrying, direction: .in, rail: .bank, asset: nil, amount: nil)
        let sent = FundingWording.band(.sent, direction: .out, rail: .card, asset: nil, amount: nil)
        let delayed = FundingWording.band(.delayed, direction: .out, rail: .bank, asset: nil, amount: nil)

        #expect(adding?.text == "Adding $50 CASH to your balance")
        #expect(adding?.isWarning == false)
        #expect(retrying?.text == "The provider is retrying. Don't send the money again.")
        #expect(retrying?.isWarning == true)
        #expect(sent?.text == "Sent to your card")
        #expect(delayed?.text == "Taking longer than usual. It will continue automatically.")
        #expect(FundingWording.band(.cancel, direction: .in, rail: .card, asset: nil, amount: nil) == nil)
    }

    @Test
    func aTopUpCancelledBeforePayingLeavesTheHistory() {
        #expect(record(.failed(.cancelled), settled: nil).isCancelledUnpaid)
        #expect(!record(.failed(.cancelled), settled: "5000").isCancelledUnpaid)
        #expect(!record(.failed(.declined), settled: nil).isCancelledUnpaid)
    }

    private func record(_ outcome: FundingRecord.Outcome, settled: String?) -> FundingRecord {
        let now = Date(timeIntervalSince1970: 1_792_324_800)
        return FundingRecord(
            intent: "fs_1",
            direction: .in,
            rail: .bank,
            asset: nil,
            providerId: nil,
            requestedAmount: "5000",
            settledAmount: settled,
            paidAmount: nil,
            paidAsset: nil,
            paidDecimals: nil,
            paidNetwork: nil,
            outcome: outcome,
            payout: nil,
            refundedAmount: nil,
            transactionId: nil,
            reference: nil,
            openedAt: now,
            settledAt: now
        )
    }
}
