import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// What the app shows for each answer the core gives to a cancel, and for
/// each status a cancel puts a session in.
struct FundingCancelPresentationTests {
    @Test
    func aSessionTheCoreEndedClosesTheOverlay() {
        #expect(FundingCancelPresentation.after(CancelOutcome.ended) == .close)
    }

    @Test
    func aCancelWithTheProviderSaysItIsWaiting() {
        let waiting = FundingCancelPresentation.AfterCancel.toast(String(localized: .Funding.activityCancelRequested))

        #expect(FundingCancelPresentation.after(CancelOutcome.requested) == waiting)
        #expect(FundingCancelPresentation.after(CancelOutcome.pending) == waiting)
    }

    @Test
    func aCreditedTopUpIsTooLateToCancel() {
        #expect(FundingCancelPresentation
            .after(CancelOutcome.tooLate) == .toast(String(localized: .Funding.cancelTooLate)))
    }

    @Test
    func aRefusedCancelSaysSo() {
        #expect(
            FundingCancelPresentation.after(CancelOutcome.refused)
                == .toast(String(localized: .Funding.errorCancelRefused))
        )
    }

    @Test
    func eachAnswerToAnOfferHasItsMessage() {
        #expect(FundingCancelPresentation
            .after(CancelConfirmation.accepted) == String(localized: .Funding.offerAccepted))
        #expect(FundingCancelPresentation.after(CancelConfirmation.declined) == nil)
        #expect(FundingCancelPresentation.after(CancelConfirmation.lapsed) == String(localized: .Funding.offerLapsed))
        #expect(FundingCancelPresentation.after(CancelConfirmation.noOffer) == String(localized: .Funding.offerNone))
    }

    @Test
    func anOfferThatChangedOrCameTooLateSaysSo() {
        #expect(FundingCancelPresentation.after(CancelConfirmation.changed) == String(localized: .Funding.offerChanged))
        #expect(FundingCancelPresentation.after(CancelConfirmation.tooLate) == String(localized: .Funding.offerTooLate))
    }

    @Test
    func aPayoutAfterAnAcceptedCancelSaysTheCancelCameTooLate() {
        #expect(FundingCancelPresentation.cancelCameTooLate(.paidOutAfterCancel))
        #expect(!FundingCancelPresentation.cancelCameTooLate(.sent))
        #expect(!FundingCancelPresentation.cancelCameTooLate(.cancelling))
    }

    @Test
    func aCancelWithTheProviderShowsWhereItStands() throws {
        let requested = try #require(FundingCancelPresentation.line(for: .cancelRequested))
        let offered = try #require(FundingCancelPresentation.line(for: .cancelOffered(
            fee: "1000000",
            refund: "49000000"
        )))
        let refused = try #require(FundingCancelPresentation.line(for: .cancelRefused(reason: "card settled")))
        let compactRefused = try #require(
            FundingCancelPresentation.line(for: .cancelRefused(reason: "card settled"), compact: true)
        )
        let cancelling = try #require(FundingCancelPresentation.line(for: .cancelling))

        #expect(requested.text == String(localized: .Funding.activityCancelRequested) && !requested.isWarning)
        #expect(offered.isWarning)
        #expect(refused.text.contains("card settled") && refused.isWarning)
        #expect(compactRefused.text == String(localized: .Funding.activityCancelRefused))
        #expect(!cancelling.isWarning)
        #expect(FundingCancelPresentation.line(for: .convertingToCash) == nil)
    }

    @Test
    func aSecondCancelStaysReachableUntilMoneyMoves() {
        #expect(FundingCancelPresentation.canCancel(.cancelRequested, moneyMoved: false))
        #expect(FundingCancelPresentation.cancelTitle(.cancelRequested) == String(localized: .Funding.cancelAnyway))
    }

    @Test
    func onceMoneyMovedACancelWithTheProviderWaitsForItsAnswer() {
        #expect(!FundingCancelPresentation.canCancel(.cancelRequested, moneyMoved: true))
        #expect(!FundingCancelPresentation.canCancel(.cancelOffered(fee: "1", refund: "1"), moneyMoved: true))
        #expect(!FundingCancelPresentation.canCancel(.cancelling, moneyMoved: true))
        #expect(FundingCancelPresentation.canCancel(.cancelRefused(reason: "no"), moneyMoved: true))
        #expect(FundingCancelPresentation.canCancel(.waitingForTransfer, moneyMoved: false))
        #expect(FundingCancelPresentation.cancelTitle(.waitingForTransfer) == String(localized: .Funding.depositCancel))
    }
}
