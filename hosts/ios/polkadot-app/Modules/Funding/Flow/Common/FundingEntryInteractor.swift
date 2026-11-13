import Foundation
import TrUAPIHost

/// What every funding screen's presenter hears from its interactor.
@MainActor
protocol FundingEntryInteractorOutputProtocol: AnyObject {
    func didReceive(entry: FundingEntry)
    /// The session ended, or the core dropped it before it started.
    func didStopFollowing()
}

/// The interactor every funding screen builds on: follows its session
/// through the shared service, and reports what the user picked.
@MainActor
class FundingEntryInteractor {
    let service: FundingServiceProtocol
    let entryId: String
    weak var entryOutput: FundingEntryInteractorOutputProtocol?

    private let following = FundingFollowing()

    init(service: FundingServiceProtocol, entryId: String) {
        self.service = service
        self.entryId = entryId
    }

    func setup() {
        following.task?.cancel()
        following.task = Task { [weak self, service, entryId] in
            do {
                for try await entry in service.subscribeEntryState(entryId) {
                    self?.entryOutput?.didReceive(entry: entry)
                }
            } catch {}
            guard !Task.isCancelled else { return }
            self?.entryOutput?.didStopFollowing()
        }
    }

    func select(_ selection: FundingSelection) {
        service.select(selection, for: entryId)
    }

    func quote(amount: U128?) {
        service.quote(amount: amount, for: entryId)
    }

    /// Reads the session again, as when it turns to taking longer, which the
    /// core announces with no change.
    func refresh() {
        setup()
    }

    /// The user left the flow: a session that never started is dropped.
    func leave() {
        service.leave(entryId)
    }
}

/// Holds a follow task, cancelled when its owner goes.
final class FundingFollowing {
    var task: Task<Void, Never>?

    deinit {
        task?.cancel()
    }
}
