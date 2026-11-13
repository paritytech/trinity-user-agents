import Foundation
import TrUAPIHost

@MainActor
final class FundingActivityInteractor: FundingActivityInteractorInputProtocol {
    weak var presenter: FundingActivityInteractorOutputProtocol?

    private let service: FundingServiceProtocol
    private let following = FundingFollowing()
    private let refreshing = FundingFollowing()

    init(service: FundingServiceProtocol) {
        self.service = service
    }

    func setup(refreshing refreshes: Bool) {
        follow()
        guard refreshes else { return }

        refreshing.task = Task { [weak self] in
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
        following.task?.cancel()
        following.task = Task { [weak self, service] in
            do {
                for try await entries in service.subscribeEntries() {
                    self?.presenter?.didReceive(entries: entries)
                }
            } catch {}
        }
    }
}
