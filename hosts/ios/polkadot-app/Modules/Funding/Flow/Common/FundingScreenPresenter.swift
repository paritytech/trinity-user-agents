import Foundation
import TrUAPIHost

/// What every funding screen's presenter does: follows the session through
/// its interactor and keeps the flow's draft and the core's selection in
/// step. Each screen's presenter overrides `render()` to push its own view
/// model, and adds its own actions.
@MainActor
class FundingScreenPresenter: FundingEntryInteractorOutputProtocol {
    let flowState: FundingFlowState
    let entryInteractor: FundingEntryInteractor
    private(set) var entry: FundingEntry
    /// What this screen is doing, drawn over the shared state.
    var activity = FundingScreenActivity() {
        didSet { render() }
    }

    init(flowState: FundingFlowState, interactor: FundingEntryInteractor) {
        self.flowState = flowState
        entryInteractor = interactor
        entry = flowState.latestEntry
    }

    func setup() {
        flowState.observe(self) { [weak self] in self?.render() }
        render()
        entryInteractor.setup()
    }

    var model: FundingEntryModel {
        var model = flowState.model(for: entry)
        model.amountIssue = activity.amountIssue
        model.isStarting = activity.isStarting
        model.startError = activity.startError
        model.isCancelling = activity.isCancelling
        model.isAnsweringOffer = activity.isAnsweringOffer
        model.historyClock = activity.historyClock
        return model
    }

    /// Builds the screen's view model from `model` and pushes it to the view.
    func render() {}

    /// Tells the core of the draft's selection when it changed since the
    /// core was last told, and the session has not started.
    func reportSelection() {
        guard let selection = flowState.selectionToReport(hasStarted: entry.hasStarted) else { return }
        entryInteractor.select(selection)
    }

    /// Asks the core to price the typed amount on the draft's selection,
    /// which it is told of first.
    func requestQuote() {
        reportSelection()
        entryInteractor.quote(amount: model.amount)
    }

    /// A hook for a screen that acts on the session's new state.
    func entryDidChange(from _: FundingEntry) {}

    // MARK: FundingEntryInteractorOutputProtocol

    func didReceive(entry newEntry: FundingEntry) {
        let previous = entry
        entry = newEntry
        flowState.absorb(newEntry)
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
