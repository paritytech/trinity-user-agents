import Foundation
import SubstrateSdk
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
    let logger: LoggerProtocol
    weak var entryOutput: FundingEntryInteractorOutputProtocol?

    private var followingTask: Task<Void, Never>?

    init(service: FundingServiceProtocol, entryId: String, logger: LoggerProtocol = Logger.shared) {
        self.service = service
        self.entryId = entryId
        self.logger = logger
    }

    deinit {
        followingTask?.cancel()
    }

    func setup() {
        followingTask?.cancel()
        followingTask = Task { [weak self, service, entryId, logger] in
            do {
                for try await entry in service.subscribeEntryState(entryId) {
                    self?.entryOutput?.didReceive(entry: entry)
                }
            } catch {
                if !Task.isCancelled {
                    logger.error("[funding] \(entryId): following the session failed: \(error)")
                }
            }
            guard !Task.isCancelled else { return }
            self?.entryOutput?.didStopFollowing()
        }
    }

    func select(_ selection: FundingSelection) {
        service.select(selection, for: entryId)
    }

    func quote(amount: Balance?) {
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
