import Foundation
import TrUAPIHost

/// The flow's first screen: which rail, and how much. A keypad change asks
/// for quotes once the user pauses, so the screen can show a provider's
/// limit before Continue.
final class FundingAmountPresenter: FundingScreenPresenter {
    private let interactor: FundingAmountInteractorInputProtocol
    private let wireframe: FundingAmountWireframeProtocol
    private let clock: any Clock<Duration>
    /// The ask waiting on the keypad pause.
    private var pause: Task<Void, Never>?

    init(
        context: FundingFlowContext,
        interactor: FundingAmountInteractor,
        wireframe: FundingAmountWireframeProtocol,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.clock = clock
        super.init(context: context, interactor: interactor)
    }

    override func setup() {
        super.setup()
        reportSelection()
        refreshAmountIssue()
        guard context.direction == .out else { return }

        interactor.followSpendable { [weak self] spendable in
            self?.context.update { $0.spendable = spendable }
            self?.refreshAmountIssue()
        }
    }

    override func entryDidChange(from _: FundingEntry) {
        refreshAmountIssue()
    }

    /// Returns once the ask waiting on the keypad pause has been made.
    func pauseSettled() async {
        await pause?.value
    }
}

extension FundingAmountPresenter: FundingAmountPresenterProtocol {
    func selectRail(_ rail: FundingRail) {
        guard rail != context.draft.rail else { return }

        context.update { draft in
            draft.rail = rail
            draft.network = nil
            draft.asset = nil
        }
        interactor.quote(amount: nil)
        reportSelection()
        askAfterPause()
    }

    func amountTextChanged(_ text: String) {
        context.update { $0.amountText = text }
        refreshAmountIssue()
        askAfterPause()
    }

    func continueFromAmount() {
        guard state.canContinue else { return }

        pause?.cancel()
        switch context.draft.rail {
        case .crypto:
            wireframe.show(.network)
        case .card,
             .bank:
            requestQuote()
            wireframe.show(.summary)
        }
    }

    func showHistory() {
        pause?.cancel()
        wireframe.showHistory()
    }

    func close() {
        pause?.cancel()
        wireframe.close(then: nil)
    }
}

private extension FundingAmountPresenter {
    /// Asks for quotes on the typed amount once the keypad has paused,
    /// replacing the ask waiting on the pause before it.
    func askAfterPause() {
        pause?.cancel()
        guard context.draft.rail != .crypto, state.amount > 0 else { return }

        pause = Task { [clock] in
            try? await clock.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }

            requestQuote()
        }
    }

    /// Reads the core's issue with the typed amount again.
    func refreshAmountIssue() {
        let spendable = context.draft.spendable.flatMap { context.cash.units($0) }
        activity.amountIssue = state.amountUnits.flatMap { interactor.amountIssue($0, spendable: spendable) }
    }
}
