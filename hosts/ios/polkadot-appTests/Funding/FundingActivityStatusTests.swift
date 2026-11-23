import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How the CASH card lists a session in flight: with the status the core
/// gives it.
struct FundingActivityStatusTests {
    @Test
    func aReleasedWithdrawalStaysInFlightUntilItsPayoutIsReported() {
        #expect(FundingStatus.preparingPayout.isInFlight)
        #expect(FundingStatus.convertingTo(asset: "USDT").isInFlight)
        #expect(FundingStatus.retryingPayout.isInFlight)
        #expect(FundingStatus.retryingPayout.isDelayed)
        #expect(!FundingStatus.sent.isInFlight)
        #expect(!FundingStatus.payoutFailed(refunded: nil).isInFlight)
    }

    @Test
    func aCancelInProgressStaysInFlight() {
        #expect(FundingStatus.cancelRequested.isInFlight)
        #expect(FundingStatus.cancelOffered(fee: "1", refund: "1").isInFlight)
        #expect(FundingStatus.cancelRefused(reason: "no").isInFlight)
        #expect(FundingStatus.cancelling.isInFlight)
        #expect(!FundingStatus.cancelled(refunded: nil).isInFlight)
    }
}
