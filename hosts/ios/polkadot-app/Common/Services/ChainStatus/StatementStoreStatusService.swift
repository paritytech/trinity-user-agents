import AsyncExtensions
import ChainRegistry
import Foundation
import os

protocol StatementStoreStatusProviding: AnyObject {
    func statusStream() -> AnyAsyncSequence<StatementStoreStatus>
    func start()
}

final class StatementStoreStatusService: StatementStoreStatusProviding, @unchecked Sendable {
    fileprivate struct Inputs {
        var network: NetworkStatus = .connecting
        var activity: StatementStoreActivity = .idle
    }

    private let networkStatusService: NetworkStatusProviding
    private let activityMonitor: StatementStoreActivityMonitoring
    private let chainId: ChainModel.Id
    private let logger: LoggerProtocol

    private let subject = AsyncCurrentValueSubject<StatementStoreStatus>(.connecting)
    private let inputs = OSAllocatedUnfairLock<Inputs>(initialState: Inputs())
    private let tasks = OSAllocatedUnfairLock<[Task<Void, Never>]>(initialState: [])

    init(
        networkStatusService: NetworkStatusProviding,
        activityMonitor: StatementStoreActivityMonitoring,
        chainId: ChainModel.Id = AppConfig.Chains.chatChain,
        logger: LoggerProtocol
    ) {
        self.networkStatusService = networkStatusService
        self.activityMonitor = activityMonitor
        self.chainId = chainId
        self.logger = logger
    }

    deinit {
        tasks.withLock { $0.forEach { $0.cancel() } }
    }

    func statusStream() -> AnyAsyncSequence<StatementStoreStatus> {
        subject.eraseToAnyAsyncSequence()
    }

    func start() {
        tasks.withLock { tasks in
            guard tasks.isEmpty else { return }
            tasks = [observeNetwork(), observeActivity()]
        }
    }
}

private extension StatementStoreStatusService {
    func observeNetwork() -> Task<Void, Never> {
        Task { [weak self, networkStatusService, chainId, logger] in
            do {
                for try await status in networkStatusService.statusStream(for: [chainId]) {
                    guard let self, !Task.isCancelled else { return }
                    resolve { $0.network = status }
                }
            } catch {
                logger.error("Statement store status: network stream failed: \(error)")
            }
        }
    }

    func observeActivity() -> Task<Void, Never> {
        Task { [weak self, activityMonitor, logger] in
            do {
                for try await activity in activityMonitor.activityStream() {
                    guard let self, !Task.isCancelled else { return }
                    resolve { $0.activity = activity }
                }
            } catch {
                logger.error("Statement store status: activity stream failed: \(error)")
            }
        }
    }

    func resolve(_ mutate: (inout Inputs) -> Void) {
        inputs.withLock { inputs in
            mutate(&inputs)

            let status = StatementStoreStatus.resolve(network: inputs.network, activity: inputs.activity)

            if status != subject.value {
                subject.send(status)
            }
        }
    }
}
