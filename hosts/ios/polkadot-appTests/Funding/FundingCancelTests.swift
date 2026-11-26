import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// What follows a cancel, and how one that lost the race to a payout is told
/// apart.
struct FundingCancelTests {
    @Test
    func aCancelledSessionClosesTheFlow() {
        #expect(CancelOutcome.cancelled.followUp == .close)
        #expect(CancelOutcome.requested.followUp != .close)
        #expect(CancelOutcome.refused.followUp != .close)
    }

    @Test
    func aPayoutAfterAnAcceptedCancelSaysTheCancelCameTooLate() {
        #expect(FundingStatus.paidOutAfterCancel.cancelCameTooLate)
        #expect(!FundingStatus.sent.cancelCameTooLate)
        #expect(!FundingStatus.cancelling.cancelCameTooLate)
    }
}
