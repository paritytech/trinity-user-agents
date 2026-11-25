import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How a cancel that lost the race to a payout is told apart.
struct FundingCancelTests {
    @Test
    func aPayoutAfterAnAcceptedCancelSaysTheCancelCameTooLate() {
        #expect(FundingStatus.paidOutAfterCancel.cancelCameTooLate)
        #expect(!FundingStatus.sent.cancelCameTooLate)
        #expect(!FundingStatus.cancelling.cancelCameTooLate)
    }
}
