import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How a session's band reads.
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
}
