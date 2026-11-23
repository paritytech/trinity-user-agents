import Foundation
import TrUAPIHost

@MainActor
final class FundingActivityInteractor: FundingActivityInteractorInputProtocol {
    /// How often the history reads the sessions again.
    static let refreshInterval: Duration = .seconds(10)

    weak var presenter: FundingActivityInteractorOutputProtocol?

    private let service: FundingServiceProtocol
    private let clock: any Clock<Duration>
    private let logger: LoggerProtocol
    private var followingTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?

    init(
        service: FundingServiceProtocol,
        clock: any Clock<Duration> = ContinuousClock(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.service = service
        self.clock = clock
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
        refreshTask = Task { [weak self, clock] in
            while await (try? clock.sleep(for: Self.refreshInterval)) != nil {
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
