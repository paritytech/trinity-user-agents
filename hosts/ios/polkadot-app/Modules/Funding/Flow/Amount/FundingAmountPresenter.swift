import Foundation
import TrUAPIHost

/// The flow's first screen: which rail, and how much. A keypad change asks
/// for quotes once the user pauses, so the screen can show a provider's
/// limit before Continue.
final class FundingAmountPresenter: FundingScreenPresenter {
    weak var view: FundingAmountViewProtocol?
    private let interactor: FundingAmountInteractorInputProtocol
    private let wireframe: FundingAmountWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol
    /// Holds the quote ask until the keypad pauses.
    private let quoteDebouncer = Debouncer(delay: 0.6)

    init(
        context: FundingFlowContext,
        interactor: FundingAmountInteractor,
        wireframe: FundingAmountWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeAmountViewModel(for: state))
    }

    override func setup() {
        super.setup()
        reportSelection()
        refreshAmountIssue()
        interactor.followEntries { [weak self] entries in
            self?.activity.historyClock = FundingHistoryClock(entries: entries)
        }
        guard context.direction == .out else { return }

        interactor.followSpendable { [weak self] spendable in
            self?.context.update { $0.spendable = spendable }
            self?.refreshAmountIssue()
        }
    }

    override func entryDidChange(from _: FundingEntry) {
        refreshAmountIssue()
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

        quoteDebouncer.cancel()
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
        quoteDebouncer.cancel()
        wireframe.showHistory()
    }

    func close() {
        quoteDebouncer.cancel()
        wireframe.close(then: nil)
    }
}

private extension FundingAmountPresenter {
    /// Asks for quotes on the typed amount once the keypad has paused,
    /// replacing the ask waiting on the pause before it.
    func askAfterPause() {
        quoteDebouncer.cancel()
        guard context.draft.rail != .crypto, (state.amount ?? 0) > 0 else { return }

        quoteDebouncer.debounce { [weak self] in
            self?.requestQuote()
        }
    }

    /// Reads the core's issue with the typed amount again.
    func refreshAmountIssue() {
        let spendable = context.draft.spendable
        activity.amountIssue = state.amount.flatMap { interactor.amountIssue($0, spendable: spendable) }
    }
}
