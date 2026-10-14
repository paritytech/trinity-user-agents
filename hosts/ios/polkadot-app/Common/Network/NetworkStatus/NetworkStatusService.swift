import Foundation
import SubstrateSdk
import AsyncExtensions
import AsyncAlgorithms
import ChainRegistry

protocol NetworkStatusProviding: AnyObject {
    func statusStream(for chainIds: Set<ChainModel.Id>) -> AnyAsyncSequence<NetworkStatus>
}

final class NetworkStatusService {
    fileprivate struct Snapshot {
        var isPathAvailable = true
        var connectedChainIds: Set<ChainModel.Id> = []

        func status(for scope: Set<ChainModel.Id>) -> NetworkStatus {
            guard isPathAvailable else {
                return .waitingForNetwork
            }

            return scope.isSubset(of: connectedChainIds) ? .connected : .connecting
        }
    }

    private let chainRegistry: ChainRegistryProtocol
    private let pathMonitor: NetworkPathMonitoring
    private let mutex = NSLock()
    private let snapshotSubject = AsyncCurrentValueSubject<Snapshot>(Snapshot())

    private var observedChainIds: Set<ChainModel.Id> = []
    private var pathTask: Task<Void, Never>?

    init(
        chainRegistry: ChainRegistryProtocol,
        pathMonitor: NetworkPathMonitoring
    ) {
        self.chainRegistry = chainRegistry
        self.pathMonitor = pathMonitor
    }

    deinit {
        pathTask?.cancel()
    }
}

extension NetworkStatusService: NetworkStatusProviding {
    func statusStream(for chainIds: Set<ChainModel.Id>) -> AnyAsyncSequence<NetworkStatus> {
        observeChainsIfNeeded(chainIds)

        return snapshotSubject
            .map { $0.status(for: chainIds) }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

extension NetworkStatusService: ConnectionStateSubscription {
    func didReceive(state: WebSocketEngine.State, for chainId: ChainModel.Id) {
        updateSnapshot { snapshot in
            if state.isConnected {
                snapshot.connectedChainIds.insert(chainId)
            } else {
                snapshot.connectedChainIds.remove(chainId)
            }
        }
    }
}

private extension NetworkStatusService {
    func observeChainsIfNeeded(_ chainIds: Set<ChainModel.Id>) {
        mutex.lock()

        startPathMonitoringIfNeeded()

        let newChainIds = chainIds.subtracting(observedChainIds)
        observedChainIds.formUnion(newChainIds)

        mutex.unlock()

        newChainIds.forEach { chainRegistry.subscribeChainState(self, chainId: $0) }
    }

    func startPathMonitoringIfNeeded() {
        guard pathTask == nil else {
            return
        }

        let stream = pathMonitor.pathStream()

        pathTask = Task { [weak self] in
            do {
                for try await isAvailable in stream {
                    self?.updateSnapshot { $0.isPathAvailable = isAvailable }
                }
            } catch {}
        }
    }

    func updateSnapshot(_ mutate: (inout Snapshot) -> Void) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        var snapshot = snapshotSubject.value
        mutate(&snapshot)
        snapshotSubject.send(snapshot)
    }
}
