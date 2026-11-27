import Foundation
import TrUAPIHost

/// The flow's first screen: which rail, and how much. A keypad change asks
/// for quotes once the user pauses, so the screen can show a provider's
/// limit before Continue.
final class FundingAmountPresenter: FundingScreenPresenter {
    /// How long the keypad pauses before the typed amount is quoted.
    static let quotePause: Duration = .milliseconds(600)

    weak var view: FundingAmountViewProtocol?
    private let interactor: FundingAmountInteractorInputProtocol
    private let wireframe: FundingAmountWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol
    private let clock: any Clock<Duration>
    /// The quote ask waiting on the keypad's pause.
    private var quoteAsk: Task<Void, Never>?

    init(
        flowState: FundingFlowState,
        interactor: FundingAmountInteractor,
        wireframe: FundingAmountWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.clock = clock
        super.init(flowState: flowState, interactor: interactor)
    }

    deinit {
        quoteAsk?.cancel()
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeAmountViewModel(for: model))
    }

    override func setup() {
        super.setup()
        reportSelection()
        refreshAmountIssue()
        interactor.followEntries { [weak self] entries in
            self?.activity.historyClock = FundingHistoryClock(entries: entries)
        }
        guard flowState.direction == .out else { return }

        interactor.followSpendable { [weak self] spendable in
            self?.flowState.update { $0.spendable = spendable }
            self?.refreshAmountIssue()
        }
    }

    override func entryDidChange(from _: FundingEntry) {
        refreshAmountIssue()
    }
}

extension FundingAmountPresenter: FundingAmountPresenterProtocol {
    func selectRail(_ rail: FundingRail) {
        guard rail != flowState.draft.rail else { return }

        flowState.update { draft in
            draft.rail = rail
            draft.network = nil
            draft.asset = nil
        }
        interactor.quote(amount: nil)
        reportSelection()
        askAfterPause()
    }

    func amountTextChanged(_ text: String) {
        flowState.update { $0.amountText = text }
        refreshAmountIssue()
        askAfterPause()
    }

    func continueFromAmount() {
        guard model.canContinue else { return }

        quoteAsk?.cancel()
        switch flowState.draft.rail {
        case .crypto:
            wireframe.show(.network)
        case .card,
             .bank:
            requestQuote()
            wireframe.show(.summary)
        }
    }

    func showHistory() {
        quoteAsk?.cancel()
        wireframe.showHistory()
    }

    func close() {
        quoteAsk?.cancel()
        wireframe.close(then: nil)
    }
}

private extension FundingAmountPresenter {
    /// Asks for quotes on the typed amount once the keypad has paused,
    /// replacing the ask waiting on the pause before it.
    func askAfterPause() {
        quoteAsk?.cancel()
        guard flowState.draft.rail != .crypto, (model.amount ?? 0) > 0 else { return }

        quoteAsk = Task { [weak self, clock] in
            guard await (try? clock.sleep(for: Self.quotePause)) != nil, !Task.isCancelled else { return }
            self?.requestQuote()
        }
    }

    /// Reads the core's issue with the typed amount again.
    func refreshAmountIssue() {
        let spendable = flowState.draft.spendable
        activity.amountIssue = model.amount.flatMap { interactor.amountIssue($0, spendable: spendable) }
    }
}
