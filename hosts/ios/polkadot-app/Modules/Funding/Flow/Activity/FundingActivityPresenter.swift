import Foundation
import TrUAPIHost

/// A list of sessions. The history acknowledges the ended sessions it shows,
/// after which the core stops handing them over on each resume.
@MainActor
final class FundingActivityPresenter {
    weak var view: FundingActivityViewProtocol?

    private let mode: FundingActivityMode
    private let interactor: FundingActivityInteractorInputProtocol
    private let wireframe: FundingActivityWireframeProtocol
    private var state = FundingActivityState()
    private var justFinished: [String: Date] = [:]
    private var acknowledged: Set<String> = []

    init(
        mode: FundingActivityMode,
        interactor: FundingActivityInteractorInputProtocol,
        wireframe: FundingActivityWireframeProtocol
    ) {
        self.mode = mode
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension FundingActivityPresenter: FundingActivityPresenterProtocol {
    func setup() {
        interactor.setup(refreshing: mode == .history)
    }

    func open(_ item: FundingActivityItem) {
        wireframe.open(entryId: item.id)
    }

    func showHistory() {
        wireframe.showHistory()
    }

    func startNew() {
        guard case let .pending(direction) = mode else { return }
        wireframe.startNew(direction: direction)
    }

    func back() {
        wireframe.back()
    }
}

extension FundingActivityPresenter: FundingActivityInteractorOutputProtocol {
    func didReceive(entries: [FundingEntry]) {
        let lists = FundingActivityItem.lists(from: entries)
        let now = Date()
        justFinished = FundingActivityCenter.finished(
            before: state.inFlight.map(\.id),
            after: lists.inFlight.map(\.id),
            known: justFinished,
            now: now
        )
        switch mode {
        case .history:
            state.inFlight = lists.inFlight
            state.history = lists.history
            acknowledgeShown(entries, history: lists.history)
        case let .pending(direction):
            state.inFlight = lists.inFlight.filter { $0.direction == direction }
            state.history = []
        }
        state.arriving = Set(justFinished.keys)
        view?.didReceive(state: state)
    }
}

private extension FundingActivityPresenter {
    func acknowledgeShown(_ entries: [FundingEntry], history: [FundingActivityItem]) {
        let owed = Set(entries.filter { $0.session?.acknowledged == false }.map(\.id))
        for item in history where owed.contains(item.id) && !acknowledged.contains(item.id) {
            acknowledged.insert(item.id)
            interactor.acknowledge(item.id)
        }
    }
}
