import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A started session: where to pay opens once, the session is read again
/// when it turns to taking longer, an ended session is acknowledged once,
/// and the answer to a cancel offer says how it went.
@MainActor
@Suite(.timeLimit(.minutes(1)))
struct FundingProgressScreenTests {
    @Test
    func theDepositAddressOpensOnceWhenTheProviderReportsIt() {
        let entry = cryptoTopUp(reached: [.started])
        let harness = FundingScreenHarness(entry: entry)
        let presenter = harness.progress()
        presenter.setup()

        presenter.didReceive(entry: entry)

        #expect(harness.wireframe.calls == [.show(.deposit)])
    }

    @Test
    func theDepositAddressStaysClosedOnceTheFundsArrived() {
        let harness = FundingScreenHarness(entry: cryptoTopUp(reached: [.started, .received]))
        let presenter = harness.progress()
        presenter.setup()

        #expect(harness.wireframe.calls.isEmpty)
    }

    /// The core turns a session to taking longer without announcing it, so
    /// the screen reads it again at that time: after the wait on the clock,
    /// or at once when the time has passed.
    @Test(arguments: [(30_000, Duration?.some(.seconds(30))), (-1_000, nil)] as [(Int64, Duration?)])
    func theSessionIsReadAgainWhenItTurnsToTakingLonger(offsetMs: Int64, wait: Duration?) async {
        let longerAtMs = UInt64(Int64(FundingFixtures.nowMs) + offsetMs)
        let entry = FundingFixtures.started(progress: FundingFixtures.progressState(longerAtMs: longerAtMs))
        let harness = FundingScreenHarness(entry: entry)
        let presenter = harness.progress()
        presenter.setup()

        if let wait {
            await harness.service.changes.wait { harness.service.entryStateSubscriptions == 1 }
            await harness.clock.resumeSleep(for: wait)
        }
        await harness.service.changes.wait { harness.service.entryStateSubscriptions == 2 }

        #expect(harness.service.entryStateSubscriptions == 2)
    }

    @Test
    func anEndedSessionIsAcknowledgedOnce() async {
        let harness = FundingScreenHarness(entry: FundingFixtures.started())
        let presenter = harness.progress()
        presenter.setup()
        let ended = FundingFixtures.started(stage: .failed(reason: .declined, settledAtMs: FundingFixtures.nowMs))

        presenter.didReceive(entry: ended)
        presenter.didReceive(entry: ended)
        await harness.service.changes.wait { !harness.service.calls.isEmpty }

        #expect(harness.service.calls == [.acknowledge(FundingFixtures.entryId)])
    }

    @Test(arguments: [CancelConfirmation.accepted, .declined, .lapsed, .noOffer, .tooLate])
    func answeringTheCancelOfferSaysHowItWent(answer: CancelConfirmation) async {
        let harness = FundingScreenHarness(entry: offeredCancel())
        harness.service.cancelOfferAnswer = .success(answer)
        let presenter = harness.progress()
        presenter.setup()

        presenter.answerCancelOffer(accepting: terms)
        await waitUntilAnswered(harness)

        #expect(harness.service.calls == [.answerCancelOffer(terms)])
        #expect(harness.wireframe.toasts == [answer.toast].compactMap(\.self))
    }

    @Test
    func aFailedAnswerToTheCancelOfferSaysSo() async {
        let harness = FundingScreenHarness(entry: offeredCancel())
        harness.service.cancelOfferAnswer = .failure(AnswerFailed())
        let presenter = harness.progress()
        presenter.setup()

        presenter.answerCancelOffer(accepting: nil)
        await waitUntilAnswered(harness)

        #expect(harness.wireframe.toasts == [String(localized: .Funding.errorCancelFailed)])
    }

    @Test
    func aSecondAnswerWhileTheFirstRunsIsDropped() async {
        let harness = FundingScreenHarness(entry: offeredCancel())
        let presenter = harness.progress()
        presenter.setup()

        presenter.answerCancelOffer(accepting: terms)
        presenter.answerCancelOffer(accepting: nil)
        await waitUntilAnswered(harness)

        #expect(harness.service.calls == [.answerCancelOffer(terms)])
    }
}

private struct AnswerFailed: Error {}

private extension FundingProgressScreenTests {
    var terms: CancelTerms {
        CancelTerms(fee: "100000", refund: "49900000")
    }

    func cryptoTopUp(reached: [FundingStep]) -> FundingEntry {
        FundingFixtures.started(
            rail: .crypto,
            progress: FundingFixtures.progressState(reached: reached, deposit: FundingFixtures.cryptoDeposit)
        )
    }

    func offeredCancel() -> FundingEntry {
        FundingFixtures.started(
            progress: FundingFixtures.progressState(status: .cancelOffered(fee: terms.fee, refund: terms.refund))
        )
    }

    /// The screen stops showing the answer as running only once it is back.
    func waitUntilAnswered(_ harness: FundingScreenHarness) async {
        #expect(harness.view.progress?.isAnsweringOffer == true)
        await harness.view.changes.wait {
            !harness.service.calls.isEmpty && harness.view.progress?.isAnsweringOffer == false
        }
    }
}
