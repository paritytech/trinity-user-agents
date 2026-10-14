import Foundation
import Keystore_iOS
import SubstrateSdk
import AssetExchange
import KeyDerivation
import AssetHubSdk
import ChainRegistry

protocol AssetExchangeServiceFactoryProtocol {
    func createService() throws -> AssetExchangeServiceFactoryResult
}

final class AssetExchangeServiceFactory {
    let depositWallet: WalletManaging
    let accountToFund: AccountId
    let fundedAssetId: ChainAssetId
    let hydrationChainId: ChainId
    let ahChainId: ChainId
    let usdtChainId: ChainId
    let feePercentageBuffer: BigRational
    let chainRegistry: ChainRegistryProtocol
    let storageFacade: StorageFacadeProtocol
    let exchangesStateMediator: AssetsExchangeStateMediating
    let pathCostEstimator: AssetsExchangePathCostEstimating
    let priceStore: AssetExchangePriceStoring
    let feeSupportProvider: AssetsExchangeFeeSupportProviding
    let configManager: RemoteConfigManaging
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    private let delayedExecProvider = WalletNoDelayExecutionProvider()
    private let sufficiencyProvider = AssetExchangeSufficiencyProvider()

    init(
        depositWallet: WalletManaging,
        accountToFund: AccountId,
        fundedAssetId: ChainAssetId,
        hydrationChainId: ChainId,
        ahChainId: ChainId,
        usdtChainId: ChainId,
        feePercentageBuffer: BigRational,
        chainRegistry: ChainRegistryProtocol,
        storageFacade: StorageFacadeProtocol,
        exchangesStateMediator: AssetsExchangeStateMediating,
        priceStore: AssetExchangePriceStoring,
        configManager: RemoteConfigManaging,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.depositWallet = depositWallet
        self.accountToFund = accountToFund
        self.fundedAssetId = fundedAssetId
        self.feePercentageBuffer = feePercentageBuffer
        self.chainRegistry = chainRegistry
        self.storageFacade = storageFacade
        self.exchangesStateMediator = exchangesStateMediator
        self.priceStore = priceStore
        self.configManager = configManager
        self.operationQueue = operationQueue
        self.hydrationChainId = hydrationChainId
        self.ahChainId = ahChainId
        self.usdtChainId = usdtChainId
        self.logger = logger

        feeSupportProvider = AssetsExchangeFeeSupportProvider(
            ahChainId: ahChainId,
            hydrationChainId: hydrationChainId,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )

        pathCostEstimator = AssetsExchangePathCostEstimator(
            priceStore: priceStore,
            chainRegistry: chainRegistry,
            usdtLocationChainId: usdtChainId
        )
    }
}

private extension AssetExchangeServiceFactory {
    func createGraphProvider(
        for depositWallet: WalletManaging,
    ) -> AssetsExchangeGraphProviding {
        AssetsExchangeGraphProvider(
            selectedWallet: depositWallet,
            chainRegistry: chainRegistry,
            supportedExchangeProviders: [
                CrosschainAssetsExchangeProvider(
                    selectedWallet: depositWallet,
                    feeBufferInPercentage: feePercentageBuffer,
                    hydrationChainId: hydrationChainId,
                    syncService: XcmTransfersSyncService(
                        remoteConfigManager: configManager,
                        operationQueue: operationQueue,
                        logger: logger
                    ),
                    chainRegistry: chainRegistry,
                    pathCostEstimator: pathCostEstimator,
                    fungibilityPreservationProvider: AssetFungibilityPreservationProvider(),
                    substrateStorageFacade: storageFacade,
                    chainsWithExpensiveCrosschain: [
                        ahChainId
                    ],
                    operationQueue: operationQueue,
                    logger: logger
                ),
                // TODO: enable if we have proper hydration chain
                /// AssetsHydraExchangeProvider(
                //    selectedWallet: depositWallet,
                //    supportedChainIds: [hydrationChainId],
                //    chainRegistry: chainRegistry,
                //    pathCostEstimator: pathCostEstimator,
                //    substrateStorageFacade: storageFacade,
                //    exchangeStateRegistrar: exchangesStateMediator,
                //    feeBufferInPercentage: feePercentageBuffer,
                //    operationQueue: operationQueue,
                //    logger: logger
                // ),
                AssetsHubExchangeProvider(
                    selectedWallet: depositWallet,
                    supportedChainIds: [ahChainId],
                    chainRegistry: chainRegistry,
                    pathCostEstimator: pathCostEstimator,
                    substrateStorageFacade: storageFacade,
                    exchangeStateRegistrar: exchangesStateMediator,
                    feeBufferInPercentage: feePercentageBuffer,
                    operationQueue: operationQueue,
                    logger: logger
                )
            ],
            feeSupportProvider: feeSupportProvider,
            suffiencyProvider: sufficiencyProvider,
            delayedCallExecProvider: delayedExecProvider,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

extension AssetExchangeServiceFactory: AssetExchangeServiceFactoryProtocol {
    func createService() throws -> AssetExchangeServiceFactoryResult {
        let graphProvider = createGraphProvider(for: depositWallet)

        let service = AssetsExchangeService(
            graphProvider: graphProvider,
            feeSupportProvider: feeSupportProvider,
            exchangesStateMediator: exchangesStateMediator,
            pathCostEstimator: pathCostEstimator,
            operationQueue: operationQueue,
            logger: logger
        )

        return AssetExchangeServiceFactoryResult(
            service: service,
            fundedAssetId: fundedAssetId,
            walletToDeposit: depositWallet,
            accountToFund: accountToFund
        )
    }
}
