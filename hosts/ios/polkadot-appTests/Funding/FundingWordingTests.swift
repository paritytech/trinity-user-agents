import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How a session's band reads, and which ended sessions the history drops.
struct FundingWordingTests {
    /// Each band reads as the line the core docs give it for its direction
    /// and rail, whatever language the catalog is in.
    @Test
    func bandLinesFollowTheCoreDocs() {
        let adding = FundingWording.band(.adding, direction: .in, rail: .card, asset: nil, amount: "$50 CASH")
        let retrying = FundingWording.band(.retrying, direction: .in, rail: .bank, asset: nil, amount: nil)
        let sent = FundingWording.band(.sent, direction: .out, rail: .card, asset: nil, amount: nil)
        let delayed = FundingWording.band(.delayed, direction: .out, rail: .bank, asset: nil, amount: nil)

        #expect(adding?.text == String(localized: .Funding.bandAdding(amount: "$50 CASH")))
        #expect(adding?.isWarning == false)
        #expect(retrying?.text == String(localized: .Funding.bandRetryingBank))
        #expect(retrying?.isWarning == true)
        #expect(sent?.text == String(localized: .Funding.bandSentRail(rail: String(localized: .Funding.bandRailCard))))
        #expect(delayed?.text == String(localized: .Funding.bandDelayedOut))
        #expect(delayed?.isWarning == true)
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
