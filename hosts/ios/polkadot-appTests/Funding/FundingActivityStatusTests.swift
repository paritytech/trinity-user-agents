import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How the CASH card lists a session: in flight with the status the core
/// gives it, then from the host's record once it has ended.
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
    func aFailedPayoutShowsWhatItsRefundReturned() throws {
        let released = session(.out, stage: .released(debited: "50000000", settledAtMs: Self.openedAtMs))
        let pending = try #require(FundingRecord(
            session: released,
            progress: progress(.payoutFailed(refunded: nil), payout: .failed(reason: "rejected"))
        ))
        let refunded = try #require(FundingRecord(
            session: released,
            progress: progress(
                .payoutFailed(refunded: "49000000"),
                payout: .failed(reason: "rejected"),
                refunded: "49000000"
            )
        ))

        #expect(FundingActivityItem(record: pending).status == .payoutFailed(refunded: nil))
        #expect(FundingActivityItem(record: refunded).status == .payoutFailed(refunded: "49000000"))
    }

    @Test
    func aCancelInProgressStaysInFlight() {
        #expect(FundingStatus.cancelRequested.isInFlight)
        #expect(FundingStatus.cancelOffered(fee: "1", refund: "1").isInFlight)
        #expect(FundingStatus.cancelRefused(reason: "no").isInFlight)
        #expect(FundingStatus.cancelling.isInFlight)
        #expect(!FundingStatus.cancelled(refunded: nil).isInFlight)
    }

    @Test
    func aCancelledWithdrawalShowsWhatCameBack() throws {
        let released = session(.out, stage: .released(debited: "50000000", settledAtMs: Self.openedAtMs))
        let record = try #require(FundingRecord(
            session: released,
            progress: progress(.cancelled(refunded: "49000000"), refunded: "49000000")
        ))

        #expect(record.outcome == .cancelled)
        #expect(FundingActivityItem(record: record).status == .cancelled(refunded: "49000000"))
    }

    @Test
    func aTopUpCancelledBeforeMoneyMovedIsCancelled() throws {
        let failed = session(.in, stage: .failed(reason: .cancelled, settledAtMs: Self.openedAtMs))
        let record = try #require(FundingRecord(session: failed, progress: progress(.failed(reason: .cancelled))))

        #expect(FundingActivityItem(record: record).status == .cancelled(refunded: nil))
    }

    @Test
    func aWithdrawalPaidOutIsSent() throws {
        let released = session(.out, stage: .released(debited: "50000000", settledAtMs: Self.openedAtMs))
        let record = try #require(FundingRecord(session: released, progress: progress(.sent, payout: .paidOut)))

        #expect(FundingActivityItem(record: record).status == .sent)
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
            providerId: "provider",
            assignedAtMs: Self.openedAtMs,
            cancel: nil,
            updates: [],
            started: [],
            choice: FundingChoice(quote: quote, rail: .bank, asset: "EUR", network: nil),
            saved: nil,
            refund: nil
        )
    }

    func progress(
        _ status: FundingStatus,
        payout: FundingPayout? = nil,
        refunded: U128? = nil
    ) -> FundingProgress {
        FundingProgress(
            steps: [FundingProgressStep(step: .started, reachedAtMs: Self.openedAtMs)],
            failedAtMs: nil,
            transactionId: nil,
            reference: nil,
            deposit: nil,
            mismatch: nil,
            retrying: status == .retrying,
            payout: payout,
            refunded: refunded,
            status: status,
            longerAtMs: nil
        )
    }
}
