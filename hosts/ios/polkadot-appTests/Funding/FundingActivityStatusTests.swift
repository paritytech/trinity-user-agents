import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How the CASH card lists a session in flight: with the status the core
/// gives it.
struct FundingActivityStatusTests {
    @Test(arguments: [
        FundingStatus.waitingForPayment,
        .waitingForTransfer,
        .waitingForDeposit,
        .convertingToCash,
        .retrying,
        .takingLonger
    ])
    func aSessionInFlightShowsTheCoresStatus(status: FundingStatus) {
        let item = FundingActivityItem(session: session(.in, stage: .open), progress: progress(status))

        #expect(item.status == .inProgress(status))
    }

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

private extension FundingActivityStatusTests {
    static let openedAtMs: UInt64 = 1_700_000_000_000

    func session(_ direction: FundingDirection, stage: FundingStage) -> FundingSession {
        let quote = FundingQuote(
            quoteId: "quote",
            sendAmount: "50000000",
            receiveAmount: "50000000",
            providerFee: "0",
            networkFee: "0",
            etaSecs: nil,
            expiresAt: nil,
            assetDecimals: 2
        )
        return FundingSession(
            intent: "intent",
            ownerProductId: nil,
            direction: direction,
            amount: "50000000",
            stage: stage,
            openedAtMs: Self.openedAtMs,
            deadlineMs: Self.openedAtMs + 86_400_000,
            acknowledged: false,
            providerId: try! ProductId("provider.dot"),
            assignedAtMs: Self.openedAtMs,
            cancel: nil,
            updates: [],
            started: [],
            choice: FundingChoice(quote: quote, rail: .bank, asset: "EUR", network: nil),
            refund: nil,
            detail: nil
        )
    }

    func progress(_ status: FundingStatus) -> FundingProgress {
        FundingProgress(
            steps: [FundingProgressStep(step: .started, reachedAtMs: Self.openedAtMs)],
            band: .awaitingPayment,
            lastHeardAtMs: nil,
            detail: nil,
            failedAtMs: nil,
            transactionId: nil,
            reference: nil,
            deposit: nil,
            mismatch: nil,
            retrying: status == .retrying,
            payout: nil,
            refunded: nil,
            status: status,
            longerAtMs: nil,
            moneyMoved: false,
            cancel: .unavailable
        )
    }
}
