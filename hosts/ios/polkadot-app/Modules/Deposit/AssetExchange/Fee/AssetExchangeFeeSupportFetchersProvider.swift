import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import AssetExchange
import AssetHubSdk
import HydrationSdk
import CommonService
import ChainRegistry

class AssetExchangeFeeSupportFetchersProvider {
    private var observableState: Observable<NotEqualWrapper<[AssetExchangeFeeSupportFetching]>> = .init(
        state: .init(value: [])
    )

    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let syncQueue: DispatchQueue
    let logger: LoggerProtocol

    private var supportedChains: [ChainModel.Id: ChainModel]?

    private let ahChainId: ChainId
    private let hydrationChainId: ChainId

    private var supportedChainIds: Set<ChainId> {
        [
            ahChainId,
            hydrationChainId
        ]
    }

    init(
        ahChainId: ChainId,
        hydrationChainId: ChainId,
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.ahChainId = ahChainId
        self.hydrationChainId = hydrationChainId
        self.operationQueue = operationQueue
        syncQueue = DispatchQueue(label: "io.assetexchangefeeprovider.\(UUID().uuidString)")
        self.logger = logger
    }

    private func updateState(with newSupporters: [AssetExchangeFeeSupportFetching]) {
        observableState.state = .init(value: newSupporters)
    }

    private func updateStateIfNeeded() {
        guard let supportedChains else {
            return
        }

        let feeFetchers: [AssetExchangeFeeSupportFetching] = supportedChains.values.compactMap { chain in
            do {
                let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
                let runtimeService = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

                if chain.chainId == ahChainId {
                    return AssetHubExchangeFeeSupportFetcher(
                        chain: chain,
                        swapOperationFactory: AssetHubSwapOperationFactory(
                            chain: chain,
                            runtimeService: runtimeService,
                            connection: connection,
                            tokenConverter: AssetHubTokenConverter(),
                            bulkMapperFactory: AssetHubBulkTokensMapperFactory(),
                            operationQueue: operationQueue
                        )
                    )
                } else if chain.chainId == hydrationChainId {
                    return HydraExchangeFeeSupportFetcher(
                        chain: chain,
                        tokenConverter: HydrationTokenConverter(),
                        connection: connection,
                        runtimeProvider: runtimeService,
                        operationQueue: operationQueue,
                        logger: logger
                    )
                } else {
                    return nil
                }
            } catch {
                logger.error("Can't create fetcher \(error)")
                return nil
            }
        }

        updateState(with: feeFetchers)
    }

    private func handleChains(changes: [DataProviderChange<ChainModel>]) -> Bool {
        let updatedChains = changes.reduce(into: supportedChains ?? [:]) { accum, change in
            switch change {
            case let .insert(newItem),
                 let .update(newItem):
                accum[newItem.chainId] = supportedChainIds.contains(newItem.chainId) ? newItem : nil
            case let .delete(deletedIdentifier):
                accum[deletedIdentifier] = nil
            }
        }

        guard supportedChains != updatedChains else {
            return false
        }

        supportedChains = updatedChains

        return true
    }

    private func performSetup() {
        chainRegistry.chainsSubscribe(
            self,
            runningInQueue: syncQueue
        ) { [weak self] changes in
            guard let self, handleChains(changes: changes) else {
                return
            }

            updateStateIfNeeded()
        }
    }

    private func performThrottle() {
        chainRegistry.chainsUnsubscribe(self)
    }
}

extension AssetExchangeFeeSupportFetchersProvider: AssetExchangeFeeSupportFetchersProviding {
    func setup() {
        performSetup()
    }

    func throttle() {
        performThrottle()
    }

    func subscribeFeeFetchers(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping ([AssetExchangeFeeSupportFetching]) -> Void
    ) {
        syncQueue.async { [weak self] in
            self?.observableState.addObserver(
                with: target,
                sendStateOnSubscription: true,
                queue: queue
            ) { _, newState in
                onChange(newState.value)
            }
        }
    }

    func unsubscribeFeeFetchers(_ target: AnyObject) {
        syncQueue.async { [weak self] in
            self?.observableState.removeObserver(by: target)
        }
    }
}
