import Foundation
import SubstrateSdk
import Operation_iOS
import HydrationSdk
import CommonService
import ChainStore
import ChainRegistry

final class OrmlHydrationEvmBalanceSyncService {
    let chainId: ChainModel.Id
    let accountId: AccountId
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessor: BalanceUpdateProcessing
    let syncQueue: DispatchQueue
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    let mutex = NSLock()

    private var trigger: ChainPollingStateStore?
    private var subscriptionServices: [ApplicationServiceProtocol]?

    init(
        chainId: ChainModel.Id,
        accountId: AccountId,
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessor: BalanceUpdateProcessing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainId = chainId
        self.accountId = accountId
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessor = balanceUpdateProcessor
        self.operationQueue = operationQueue
        syncQueue = DispatchQueue(label: "io.ormlhydraevm.sync.\(UUID().uuidString)")
        self.logger = logger
    }
}

private extension OrmlHydrationEvmBalanceSyncService {
    func setupSubscriptions(for chainAssetIds: [ChainAssetId], trigger: ChainPollingStateStore) {
        subscriptionServices = chainAssetIds.map { chainAssetId in
            // With sync queue as parameter we are make sure that events of balance change will be passed
            // and handled in the right order
            OrmlHydrationEvmSubscriptionService(
                chainAssetId: chainAssetId,
                accountId: accountId,
                trigger: trigger,
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                workingQueue: syncQueue,
                logger: logger,
                callbackQueue: syncQueue
            ) { [weak self] newBalance, blockHash in
                self?.balanceUpdateProcessor.process(
                    balance: newBalance,
                    blockHash: blockHash
                )
            }
        }

        subscriptionServices?.forEach { $0.setup() }
    }
}

extension OrmlHydrationEvmBalanceSyncService: ApplicationServiceProtocol {
    func setup() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard trigger == nil else {
            return
        }

        do {
            let chain = try chainRegistry.getChainOrError(for: chainId)

            let chainAssetIds: [ChainAssetId] = chain.assets.compactMap { asset in
                guard AssetType(rawType: asset.type) == .ormlHydrationEvm else {
                    return nil
                }

                return ChainAssetId(chainId: chainId, assetId: asset.assetId)
            }

            guard !chainAssetIds.isEmpty else {
                return
            }

            let trigger = ChainPollingStateStore(
                runtimeConnectionStore: ChainRegistryRuntimeConnectionStore(
                    chainId: chainId,
                    chainRegistry: chainRegistry
                ),
                operationQueue: operationQueue,
                workQueue: syncQueue,
                logger: logger
            )

            trigger.setup()
            self.trigger = trigger

            setupSubscriptions(for: chainAssetIds, trigger: trigger)
        } catch {
            logger.error("Uexpected error: \(error)")
        }
    }

    func throttle() {
        trigger?.throttle()

        subscriptionServices?.forEach { $0.throttle() }

        trigger = nil
        subscriptionServices = nil
    }
}
