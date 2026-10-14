import Foundation
import SubstrateSdk
import AsyncExtensions
import StructuredConcurrency
import AssetsManagement
import ChainRegistry
import EventCenter

struct BalanceTrackingDeps {
    let wallet: MetaAccountModelProtocol
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let eventCenter: EventCenterProtocol
    let logger: LoggerProtocol
}

struct AccountBalanceTrackingDeps {
    let accountId: AccountId
    let asset: ChainAsset
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol
}

enum BalanceTracking {
    static func track(with dependencies: BalanceTrackingDeps) -> AnyAsyncSequence<AssetBalance> {
        let syncQueue = DispatchQueue(label: "io.balance.tracking.syncQueue")

        let substrateStream = trackSubstrate(with: dependencies, syncQueue: syncQueue)
        let hydrationEvmStream = trackOrmlHydration(with: dependencies, syncQueue: syncQueue)

        return merge(substrateStream, hydrationEvmStream).eraseToAnyAsyncSequence()
    }

    static func trackAccountAsset(with dependencies: AccountBalanceTrackingDeps) -> AnyAsyncSequence<AssetBalance> {
        let syncQueue = DispatchQueue(label: "io.balance.tracking.account.syncQueue")

        // we might add evm support later
        return trackSubstrateAccountAsset(with: dependencies, syncQueue: syncQueue).eraseToAnyAsyncSequence()
    }
}

private extension BalanceTracking {
    static func trackSubstrate(
        with deps: BalanceTrackingDeps,
        syncQueue: DispatchQueue
    ) -> AsyncStream<AssetBalance> {
        AsyncStream { continuation in
            let processorFactory = CallbackBalanceProcessorFactory(
                callbackQueue: syncQueue
            ) { balance in
                continuation.yield(balance)
            }

            let handlingFactory = BalanceRemoteSubscriptionHandlingFactory(
                chainRegistry: deps.chainRegistry,
                balanceUpdateProcessorFactory: processorFactory,
                operationQueue: deps.operationQueue,
                logger: deps.logger
            )

            let balancesService = BalanceRemoteSubscriptionService(
                chainRegistry: deps.chainRegistry,
                subscriptionHandlingFactory: handlingFactory,
                operationQueue: deps.operationQueue,
                logger: deps.logger
            )

            let substrateService = SubstrateAssetsUpdatingService(
                selectedAccount: deps.wallet,
                chainRegistry: deps.chainRegistry,
                remoteSubscriptionService: balancesService,
                eventCenter: deps.eventCenter,
                logger: deps.logger
            )

            continuation.onTermination = { _ in
                substrateService.throttle()
                deps.logger.debug("Stopped substrate tracking")
            }

            substrateService.setup()
        }
    }

    static func trackOrmlHydration(
        with deps: BalanceTrackingDeps,
        syncQueue: DispatchQueue
    ) -> AsyncStream<AssetBalance> {
        AsyncStream { continuation in
            let updateProcessor = CallbackBalanceUpdateProcessor(
                transactionHandler: nil,
                callbackQueue: syncQueue
            ) { balance in
                continuation.yield(balance)
            }

            let hydraEvmSyncFactory = OrmlHydrationEvmWalletSyncFactory(
                chainRegistry: deps.chainRegistry,
                balanceUpdateProcessor: updateProcessor,
                operationQueue: deps.operationQueue,
                logger: deps.logger
            )

            let hydraEvmService = OrmlHydrationEvmWalletSyncService(
                selectedAccount: deps.wallet,
                syncServiceFactory: hydraEvmSyncFactory,
                chainRegistry: deps.chainRegistry,
                eventCenter: deps.eventCenter,
                logger: deps.logger
            )

            continuation.onTermination = { _ in
                hydraEvmService.throttle()
                deps.logger.debug("Stopped tracking orml hydration tracking")
            }

            hydraEvmService.setup()
        }
    }

    static func trackSubstrateAccountAsset(
        with deps: AccountBalanceTrackingDeps,
        syncQueue: DispatchQueue
    ) -> AsyncThrowingStream<AssetBalance, Error> {
        AsyncThrowingStream { continuation in
            let processorFactory = CallbackBalanceProcessorFactory(
                callbackQueue: syncQueue
            ) { balance in
                continuation.yield(balance)
            }

            let handlingFactory = BalanceRemoteSubscriptionHandlingFactory(
                chainRegistry: deps.chainRegistry,
                balanceUpdateProcessorFactory: processorFactory,
                operationQueue: deps.operationQueue,
                logger: deps.logger
            )

            let balancesService = BalanceRemoteSubscriptionService(
                chainRegistry: deps.chainRegistry,
                subscriptionHandlingFactory: handlingFactory,
                operationQueue: deps.operationQueue,
                logger: deps.logger
            )

            let subscriptionHolder = AnyObjectHolder<UUID>()

            continuation.onTermination = { _ in
                if let subscriptionId = subscriptionHolder.get() {
                    balancesService.detachFromAssetBalance(
                        for: subscriptionId,
                        accountId: deps.accountId,
                        chainAssetId: deps.asset.chainAssetId,
                        queue: syncQueue,
                        closure: nil
                    )
                }

                deps.logger.debug("Stopped balance tracking tracking")
            }

            let subscriptionId = balancesService.attachToAssetBalance(
                for: deps.accountId,
                chainAsset: deps.asset,
                queue: syncQueue
            ) { result in
                switch result {
                case .success:
                    deps.logger.debug("Subscription initiated")
                case let .failure(error):
                    deps.logger.error("Subscription failed: \(error)")
                    continuation.finish(throwing: error)
                }
            }

            subscriptionHolder.set(subscriptionId)
        }
    }
}
