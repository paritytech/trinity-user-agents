import Foundation
import TrUAPIHost

@MainActor
final class FundingActivityInteractor: FundingActivityInteractorInputProtocol {
    weak var presenter: FundingActivityInteractorOutputProtocol?

    private let service: FundingServiceProtocol
    private let logger: LoggerProtocol
    private var followingTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?

    init(service: FundingServiceProtocol, logger: LoggerProtocol = Logger.shared) {
        self.service = service
        self.logger = logger
    }

    deinit {
        followingTask?.cancel()
        refreshTask?.cancel()
    }

    func setup(refreshing refreshes: Bool) {
        follow()
        guard refreshes else { return }

        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(10))
                self?.follow()
            }
        }
    }

    func acknowledge(_ entryId: String) {
        Task { [service] in await service.acknowledge(entryId) }
    }
}

private extension FundingActivityInteractor {
    func follow() {
        followingTask?.cancel()
        followingTask = Task { [weak self, service, logger] in
            do {
                for try await entries in service.subscribeEntries() {
                    self?.presenter?.didReceive(entries: entries)
                }
            } catch {
                if !Task.isCancelled {
                    logger.error("[funding] following the sessions failed: \(error)")
                }
            }
        }
    }
}
