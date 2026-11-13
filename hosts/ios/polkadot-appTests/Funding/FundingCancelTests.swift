import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// What the app shows for each answer the core gives to a cancel, and for
/// each status a cancel puts a session in.
struct FundingCancelTests {
    @Test
    func aSessionTheCoreEndedClosesTheOverlay() {
        #expect(CancelOutcome.ended.followUp == .close)
    }

    @Test
    func aCancelWithTheProviderSaysItIsWaiting() {
        let waiting = CancelOutcome.FollowUp.toast(String(localized: .Funding.activityCancelRequested))

        #expect(CancelOutcome.requested.followUp == waiting)
        #expect(CancelOutcome.pending.followUp == waiting)
    }

    @Test
    func aCreditedTopUpIsTooLateToCancel() {
        #expect(CancelOutcome.tooLate.followUp == .toast(String(localized: .Funding.cancelTooLate)))
    }

    @Test
    func aRefusedCancelSaysSo() {
        #expect(
            CancelOutcome.refused.followUp
                == .toast(String(localized: .Funding.errorCancelRefused))
        )
    }

    @Test
    func eachAnswerToAnOfferHasItsMessage() {
        #expect(CancelConfirmation.accepted.toast == String(localized: .Funding.offerAccepted))
        #expect(CancelConfirmation.declined.toast == nil)
        #expect(CancelConfirmation.lapsed.toast == String(localized: .Funding.offerLapsed))
        #expect(CancelConfirmation.noOffer.toast == String(localized: .Funding.offerNone))
    }

    @Test
    func anOfferThatCameTooLateSaysSo() {
        #expect(CancelConfirmation.tooLate.toast == String(localized: .Funding.offerTooLate))
    }

    @Test
    func aPayoutAfterAnAcceptedCancelSaysTheCancelCameTooLate() {
        #expect(FundingStatus.paidOutAfterCancel.cancelCameTooLate)
        #expect(!FundingStatus.sent.cancelCameTooLate)
        #expect(!FundingStatus.cancelling.cancelCameTooLate)
    }

    @Test
    func aCancelWithTheProviderShowsWhereItStands() throws {
        let requested = try #require(FundingStatus.cancelRequested.cancelLine())
        let offer = FundingStatus.cancelOffered(fee: "1000000", refund: "49000000")
        let offered = try #require(offer.cancelLine())
        let refused = try #require(FundingStatus.cancelRefused(reason: "card settled").cancelLine())
        let compactRefused = try #require(
            FundingStatus.cancelRefused(reason: "card settled").cancelLine(compact: true)
        )
        let cancelling = try #require(FundingStatus.cancelling.cancelLine())

        #expect(requested.text == String(localized: .Funding.activityCancelRequested) && !requested.isWarning)
        #expect(offered.isWarning)
        #expect(refused.text.contains("card settled") && refused.isWarning)
        #expect(compactRefused.text == String(localized: .Funding.activityCancelRefused))
        #expect(!cancelling.isWarning)
        #expect(FundingStatus.convertingToCash.cancelLine() == nil)
    }

    @Test
    func aSecondCancelReadsDifferentlyFromTheFirst() {
        #expect(FundingCancelAvailability.available.title == String(localized: .Funding.depositCancel))
        #expect(FundingCancelAvailability.availableAgain.title == String(localized: .Funding.cancelAnyway))
        #expect(FundingCancelAvailability.unavailable.title == nil)
    }
}
