import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// What a session in flight says under its title on the CASH card, from where
/// the provider has got to on each rail.
struct FundingActivityStepTests {
    @Test(arguments: [
        (FundingRail.card, FundingActivityStep.waitingForPayment),
        (.bank, .waitingForTransfer),
        (.crypto, .waitingForDeposit)
    ])
    func valueInWaitsForTheUsersPaymentByRail(rail: FundingRail, expected: FundingActivityStep) {
        let item = FundingActivityItem(session: session(.in, rail: rail), progress: progress(reached: [.started]))

        #expect(item.status == .inProgress(expected, isDelayed: false))
    }

    @Test
    func valueInConvertsOnceTheProviderHasThePayment() {
        let item = FundingActivityItem(
            session: session(.in, rail: .card),
            progress: progress(reached: [.started, .payment])
        )

        #expect(item.status == .inProgress(.converting, isDelayed: false))
    }

    @Test
    func valueOutIsInitiatedUntilTheProviderIsPaid() {
        let item = FundingActivityItem(
            session: session(.out, rail: .bank),
            progress: progress(reached: [.started, .payment])
        )

        #expect(item.status == .inProgress(.transactionInitiated, isDelayed: false))
    }

    @Test
    func aReleasedWithdrawalPreparesItsPayoutOrConvertsToItsAsset() {
        let reached: [FundingStep] = [.started, .payment, .sent]

        let bank = FundingActivityItem(session: session(.out, rail: .bank), progress: progress(reached: reached))
        let crypto = FundingActivityItem(session: session(.out, rail: .crypto), progress: progress(reached: reached))

        #expect(bank.status == .inProgress(.preparingPayout, isDelayed: false))
        #expect(crypto.status == .inProgress(.convertingOut(asset: "USDT"), isDelayed: false))
    }

    @Test
    func retryingWinsOverEveryOtherStep() {
        let item = FundingActivityItem(
            session: session(.in, rail: .card),
            progress: progress(reached: [.started, .payment], retrying: true)
        )

        #expect(item.status == .inProgress(.retrying, isDelayed: false))
    }
}

private extension FundingActivityStepTests {
    static let openedAtMs: UInt64 = 1_700_000_000_000

    func session(_ direction: FundingDirection, rail: FundingRail) -> FundingSession {
        let quote = FundingQuote(
            quoteId: "quote",
            sendAmount: "50000000",
            receiveAmount: "50000000",
            providerFee: "0",
            networkFee: "0",
            etaSecs: nil,
            expiresAt: nil
        )
        return FundingSession(
            intent: "intent",
            ownerProductId: nil,
            direction: direction,
            amount: "50000000",
            stage: .open,
            openedAtMs: Self.openedAtMs,
            deadlineMs: Self.openedAtMs + 86_400_000,
            acknowledged: false,
            providerId: "provider",
            cancelRequested: false,
            updates: [],
            choice: FundingChoice(quote: quote, rail: rail, asset: "USDT", network: nil, amount: "50000000"),
            saved: nil
        )
    }

    func progress(reached: [FundingStep], retrying: Bool = false) -> FundingProgress {
        FundingProgress(
            steps: reached.map { FundingProgressStep(step: $0, reachedAtMs: Self.openedAtMs) },
            failedAtMs: nil,
            transactionId: nil,
            reference: nil,
            deposit: nil,
            mismatch: nil,
            retrying: retrying,
            payout: nil
        )
    }
}
