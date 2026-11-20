import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// When a session can still be cancelled, and how a cancel that lost the race
/// is told apart.
struct FundingCancelTests {
    @Test
    func aPayoutAfterAnAcceptedCancelSaysTheCancelCameTooLate() {
        #expect(FundingStatus.paidOutAfterCancel.cancelCameTooLate)
        #expect(!FundingStatus.sent.cancelCameTooLate)
        #expect(!FundingStatus.cancelling.cancelCameTooLate)
    }

    @Test
    func aSecondCancelStaysReachableUntilMoneyMoves() {
        #expect(FundingStatus.cancelRequested.canCancel(moneyMoved: false))
        #expect(FundingStatus.waitingForTransfer.canCancel(moneyMoved: false))
    }

    @Test
    func onceMoneyMovedACancelWithTheProviderWaitsForItsAnswer() {
        #expect(!FundingStatus.cancelRequested.canCancel(moneyMoved: true))
        #expect(!FundingStatus.cancelOffered(fee: "1", refund: "1").canCancel(moneyMoved: true))
        #expect(!FundingStatus.cancelling.canCancel(moneyMoved: true))
        #expect(FundingStatus.cancelRefused(reason: "no").canCancel(moneyMoved: true))
    }
}
