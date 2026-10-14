import Foundation
import ChainRegistry
import StructuredConcurrency

protocol NetworkStatusObserving: AnyObject {
    func start(onStatus: @escaping @MainActor (NetworkStatus) -> Void)
}

final class NetworkStatusObserver {
    private static let debounceDuration: Duration = .milliseconds(300)

    private let networkStatusService: NetworkStatusProviding
    private let chainIds: Set<ChainModel.Id>
    private let logger: LoggerProtocol
    private var task: Task<Void, Never>?

    init(
        networkStatusService: NetworkStatusProviding,
        chainIds: Set<ChainModel.Id>,
        logger: LoggerProtocol
    ) {
        self.networkStatusService = networkStatusService
        self.chainIds = chainIds
        self.logger = logger
    }

    deinit {
        task?.cancel()
    }
}

extension NetworkStatusObserver: NetworkStatusObserving {
    func start(onStatus: @escaping @MainActor (NetworkStatus) -> Void) {
        guard task == nil else {
            return
        }

        task = Task { [networkStatusService, chainIds, logger] in
            let statusStream = networkStatusService
                .statusStream(for: chainIds)
                .withDebounce(for: Self.debounceDuration) { $0 == .connected }

            do {
                for try await status in statusStream {
                    logger.debug("Network status: \(status)")
                    await onStatus(status)
                }
            } catch {
                logger.error("Network status stream failed: \(error)")
            }
        }
    }
}
