import Foundation
import SubstrateSdk
import ChainRegistry
import EventCenter

final class BalanceTrackingService {
    private var substrateService: SubstrateAssetsUpdatingService?
    private var hydraEvmService: OrmlHydrationEvmWalletSyncService?

    let wallet: MetaAccountModelProtocol
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let eventCenter: EventCenterProtocol
    let logger: LoggerProtocol

    init(
        wallet: MetaAccountModelProtocol,
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        eventCenter: EventCenterProtocol,
        logger: LoggerProtocol
    ) {
        self.wallet = wallet
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.eventCenter = eventCenter
        self.logger = logger
    }
}

private extension BalanceTrackingService {
    func setupSubstrateAssetService(
        callBackQueue: DispatchQueue,
        callbackClosure: @escaping BalanceProcessorCallback
    ) {
        let handlingFactory = BalanceRemoteSubscriptionHandlingFactory(
            chainRegistry: chainRegistry,
            balanceUpdateProcessorFactory: CallbackBalanceProcessorFactory(
                callbackQueue: callBackQueue,
                callbackClosure: callbackClosure
            ),
            operationQueue: operationQueue,
            logger: logger
        )

        let balancesService = BalanceRemoteSubscriptionService(
            chainRegistry: chainRegistry,
            subscriptionHandlingFactory: handlingFactory,
            operationQueue: operationQueue,
            logger: logger
        )

        substrateService = SubstrateAssetsUpdatingService(
            selectedAccount: wallet,
            chainRegistry: chainRegistry,
            remoteSubscriptionService: balancesService,
            eventCenter: eventCenter,
            logger: logger
        )

        substrateService?.setup()
    }

    func setupHydraEvmAssetService(
        callBackQueue: DispatchQueue,
        callbackClosure: @escaping BalanceProcessorCallback
    ) {
        let hydraEvmSyncFactory = OrmlHydrationEvmWalletSyncFactory(
            chainRegistry: chainRegistry,
            balanceUpdateProcessor: CallbackBalanceUpdateProcessor(
                transactionHandler: nil,
                callbackQueue: callBackQueue,
                callbackClosure: callbackClosure
            ),
            operationQueue: operationQueue,
            logger: logger
        )

        hydraEvmService = OrmlHydrationEvmWalletSyncService(
            selectedAccount: wallet,
            syncServiceFactory: hydraEvmSyncFactory,
            chainRegistry: chainRegistry,
            eventCenter: eventCenter,
            logger: logger
        )

        hydraEvmService?.setup()
    }
}

extension BalanceTrackingService {
    func setup(callBackQueue: DispatchQueue, callbackClosure: @escaping BalanceProcessorCallback) {
        guard substrateService == nil, hydraEvmService == nil else {
            logger.warning("Already running")
            return
        }

        setupSubstrateAssetService(callBackQueue: callBackQueue, callbackClosure: callbackClosure)
        setupHydraEvmAssetService(callBackQueue: callBackQueue, callbackClosure: callbackClosure)
    }

    func throttle() {
        substrateService?.throttle()
        substrateService = nil

        hydraEvmService?.throttle()
        hydraEvmService = nil
    }
}
