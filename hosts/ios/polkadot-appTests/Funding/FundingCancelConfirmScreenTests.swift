import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Confirming a cancel: the overlay closes once the session ended, and
/// otherwise the screen goes back and says where the cancel stands. An
/// unpaid top-up ends on the first tap.
@MainActor
@Suite(.timeLimit(.minutes(1)))
struct FundingCancelConfirmScreenTests {
    @Test(arguments: [CancelOutcome.cancelled, .requested, .pending, .tooLate, .refused])
    func aCancelLeavesAsTheCoreAnswered(outcome: CancelOutcome) async {
        let harness = FundingScreenHarness(entry: paidCardTopUp())
        harness.service.cancelResults = [.success(outcome)]
        let presenter = harness.cancelConfirm()
        presenter.setup()

        presenter.requestCancel()
        await waitUntilCancelled(harness)

        #expect(harness.service.calls == [.cancel])
        switch outcome.followUp {
        case .close:
            #expect(harness.wireframe.calls == [.close])
        case let .toast(message):
            #expect(harness.wireframe.calls == [.back, .toast(message)])
        }
    }

    @Test
    func anUnpaidTopUpEndsOnTheFirstTap() async {
        let harness = FundingScreenHarness(entry: FundingFixtures.started(rail: .bank))
        harness.service.cancelResults = [.success(.requested), .success(.cancelled)]
        let presenter = harness.cancelConfirm()
        presenter.setup()

        presenter.requestCancel()
        await waitUntilCancelled(harness)

        #expect(harness.service.calls == [.cancel, .cancel])
        #expect(harness.wireframe.calls == [.close])
    }

    @Test
    func aFailedCancelGoesBackAndSaysSo() async {
        let harness = FundingScreenHarness(entry: paidCardTopUp())
        harness.service.cancelResults = [.failure(CancelFailed())]
        let presenter = harness.cancelConfirm()
        presenter.setup()

        presenter.requestCancel()
        await waitUntilCancelled(harness)

        #expect(harness.wireframe.calls == [.back, .toast(String(localized: .Funding.errorCancelFailed))])
    }

    @Test
    func aSecondTapWhileCancellingAsksOnce() async {
        let harness = FundingScreenHarness(entry: paidCardTopUp())
        harness.service.holdsCalls = true
        let presenter = harness.cancelConfirm()
        presenter.setup()

        presenter.requestCancel()
        await harness.service.changes.wait { harness.service.heldCallCount == 1 }
        presenter.requestCancel()
        harness.service.releaseHeldCalls()
        await waitUntilCancelled(harness)

        #expect(harness.service.calls == [.cancel])
        #expect(harness.wireframe.calls == [.close])
    }
}

private struct CancelFailed: Error {}

private extension FundingCancelConfirmScreenTests {
    func paidCardTopUp() -> FundingEntry {
        FundingFixtures.started(rail: .card, progress: FundingFixtures.progressState(reached: [.started, .received]))
    }

    /// The screen stops showing the cancel as running only once it is done.
    func waitUntilCancelled(_ harness: FundingScreenHarness) async {
        #expect(harness.view.cancelConfirm?.isCancelling == true)
        await harness.view.changes.wait {
            !harness.service.calls.isEmpty && harness.view.cancelConfirm?.isCancelling == false
        }
    }
}
