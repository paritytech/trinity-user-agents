import Foundation
import TrUAPIHost

/// What every funding screen's presenter does: follows the session through
/// its interactor, keeps the flow's draft and the core's selection in step,
/// and draws both with the screen's text. Each screen's presenter adds its
/// own actions.
@MainActor
class FundingScreenPresenter<ViewModel>: FundingEntryInteractorOutputProtocol {
    weak var view: (any FundingScreenViewProtocol<ViewModel>)?
    let context: FundingFlowContext
    let entryInteractor: FundingEntryInteractor
    private(set) var entry: FundingEntry
    /// What this screen is doing, drawn over the shared state.
    var activity = FundingScreenActivity() {
        didSet { render() }
    }

    private let makeViewModel: (FundingFlowState) -> ViewModel

    /// `viewModel` makes the screen's text for the state it draws.
    init(
        context: FundingFlowContext,
        interactor: FundingEntryInteractor,
        viewModel: @escaping (FundingFlowState) -> ViewModel
    ) {
        self.context = context
        entryInteractor = interactor
        makeViewModel = viewModel
        entry = context.latestEntry
    }

    func setup() {
        context.observe(self) { [weak self] in self?.render() }
        render()
        entryInteractor.setup()
    }

    var state: FundingFlowState {
        var state = context.state(for: entry)
        state.amountIssue = activity.amountIssue
        state.isStarting = activity.isStarting
        state.startError = activity.startError
        state.isCancelling = activity.isCancelling
        state.isAnsweringOffer = activity.isAnsweringOffer
        state.historyClock = activity.historyClock
        return state
    }

    var viewModel: ViewModel {
        makeViewModel(state)
    }

    func render() {
        let state = state
        view?.didReceive(state: state, viewModel: makeViewModel(state))
    }

    /// Tells the core of the draft's selection when it changed since the
    /// core was last told, and the session has not started.
    func reportSelection() {
        guard let selection = context.selectionToReport(hasStarted: entry.hasStarted) else { return }
        entryInteractor.select(selection)
    }

    /// Asks the core to price the typed amount on the draft's selection,
    /// which it is told of first.
    func requestQuote() {
        reportSelection()
        entryInteractor.quote(amount: state.amount)
    }

    /// A hook for a screen that acts on the session's new state.
    func entryDidChange(from _: FundingEntry) {}

    // MARK: FundingEntryInteractorOutputProtocol

    func didReceive(entry newEntry: FundingEntry) {
        let previous = entry
        entry = newEntry
        context.absorb(newEntry)
        reportSelection()
        render()
        entryDidChange(from: previous)
    }

    func didStopFollowing() {}
}

/// What one screen is doing, drawn over the flow's shared state.
struct FundingScreenActivity: Equatable {
    var amountIssue: FundingAmountIssue?
    var isStarting = false
    var startError: String?
    var isCancelling = false
    var isAnsweringOffer = false
    var historyClock: FundingHistoryClock = .hidden
}
