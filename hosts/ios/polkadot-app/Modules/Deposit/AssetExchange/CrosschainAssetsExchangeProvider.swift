import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange
import ExtrinsicService
import XcmTransfer
import KeyDerivation
import ChainRegistry

final class CrosschainAssetsExchangeProvider: AssetsExchangeBaseProvider {
    private var xcmTransfers: XcmTransfers?
    private var allChains: [ChainModel.Id: ChainModel]?

    let selectedWallet: WalletManaging
    let syncService: XcmTransfersSyncServiceProtocol
    let pathCostEstimator: AssetsExchangePathCostEstimating
    let substrateStorageFacade: StorageFacadeProtocol
    let xcmTransferService: XcmTransferServiceProtocol
    let fungibilityPreservationProvider: AssetFungibilityPreservationProviding
    let transferResolutionFactory: XcmTransferResolutionFactoryProtocol
    let monitoringChainRegistry: ChainRegistryProtocol
    let timeEstimator: AssetExchangeTimeEstimating
    let tokenDepositMatcherFactory: TokenDepositEventMatcherFactoryProtocol
    let balanceChangeDetector: BalanceChangeDetectorFactoryProtocol
    let tokenBalanceMintFactory: TokenBalanceMintingFactoryProtocol
    let chainsWithExpensiveCrosschain: Set<ChainModel.Id>

    init(
        selectedWallet: WalletManaging,
        feeBufferInPercentage: BigRational,
        hydrationChainId: ChainId,
        syncService: XcmTransfersSyncServiceProtocol,
        chainRegistry: ChainRegistryProtocol,
        pathCostEstimator: AssetsExchangePathCostEstimating,
        fungibilityPreservationProvider: AssetFungibilityPreservationProviding,
        substrateStorageFacade: StorageFacadeProtocol,
        chainsWithExpensiveCrosschain: Set<ChainModel.Id>,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.selectedWallet = selectedWallet
        self.syncService = syncService
        self.substrateStorageFacade = substrateStorageFacade
        self.pathCostEstimator = pathCostEstimator
        self.chainsWithExpensiveCrosschain = chainsWithExpensiveCrosschain
        monitoringChainRegistry = chainRegistry

        tokenDepositMatcherFactory = TokenDepositEventMatcherFactory(logger: logger)
        balanceChangeDetector = BalanceChangeDetectorFactory(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )

        tokenBalanceMintFactory = TokenBalanceMintingFactory(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )

        let graphProxy = AssetExchangeGraphProxy(
            pathCostEstimator: pathCostEstimator,
            operationQueue: operationQueue,
            logger: logger
        )

        let customFeeEstimatingFactory = AssetExchangeFeeEstimatingRouter(
            graphProxy: graphProxy,
            hydrationChainId: hydrationChainId,
            dependencies: .init(
                wallet: selectedWallet,
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                logger: logger
            ),
            feeBufferInPercentage: feeBufferInPercentage
        )

        let extrinsicServiceFactory = ExtrinsicServiceFactory(
            chainRegistry: chainRegistry,
            substrateStorageFacade: substrateStorageFacade,
            customFeeEstimator: customFeeEstimatingFactory,
            transactionExtensionFactory: CompoundTxExtensionFactory(),
            extrinsicVersion: .V4,
            operationQueue: operationQueue
        )

        xcmTransferService = XcmTransferService(
            wallet: selectedWallet,
            chainRegistry: chainRegistry,
            extrinsicServiceFactory: extrinsicServiceFactory,
            originDefiningFactory: SignedExtrinsicOriginFactory(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                logger: logger
            ),
            tokenMintingFactory: tokenBalanceMintFactory,
            depositEventMatchingFactory: tokenDepositMatcherFactory,
            operationQueue: operationQueue,
            logger: logger
        )

        self.fungibilityPreservationProvider = fungibilityPreservationProvider

        timeEstimator = AssetExchangeTimeEstimator(chainRegistry: chainRegistry)

        transferResolutionFactory = XcmTransferResolutionFactory(
            chainRegistry: chainRegistry,
            paraIdOperationFactory: ParaIdOperationFactory(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue
            )
        )

        super.init(
            chainRegistry: chainRegistry,
            graphProxy: graphProxy,
            operationQueue: operationQueue,
            syncQueue: DispatchQueue(label: "io.crosschainassetsprovider.\(UUID().uuidString)"),
            logger: logger
        )
    }

    // MARK: Subsclass

    override func performSetup() {
        setupXcmSyncService()
        setupChainsSubscription()
    }

    override func performThrottle() {
        syncService.throttle()
        monitoringChainRegistry.chainsUnsubscribe(self)
    }
}

private extension CrosschainAssetsExchangeProvider {
    func setupXcmSyncService() {
        syncService.notificationCallback = { [weak self] transfersResult in
            switch transfersResult {
            case let .success(transfers):
                self?.xcmTransfers = transfers
                self?.updateStateIfNeeded()
            case let .failure(error):
                self?.logger.error("Xcm trasfers fetch failed \(error)")
            }
        }

        syncService.notificationQueue = syncQueue

        syncService.setup()
    }

    func setupChainsSubscription() {
        monitoringChainRegistry.chainsSubscribe(
            self,
            runningInQueue: syncQueue
        ) { [weak self] changes in
            guard let self, handleChains(changes: changes) else {
                return
            }

            updateStateIfNeeded()
        }
    }

    func handleChains(changes: [DataProviderChange<ChainModel>]) -> Bool {
        let updatedChains = changes.reduce(into: allChains ?? [:]) { accum, change in
            switch change {
            case let .insert(newItem),
                 let .update(newItem):
                accum[newItem.chainId] = newItem
            case let .delete(deletedIdentifier):
                accum[deletedIdentifier] = nil
            }
        }

        guard allChains != updatedChains else {
            return false
        }

        allChains = updatedChains

        return true
    }

    func updateStateIfNeeded() {
        guard let xcmTransfers, let allChains else {
            return
        }

        let host = CrosschainExchangeHost(
            wallet: selectedWallet,
            allChains: allChains,
            chainRegistry: chainRegistry,
            xcmService: xcmTransferService,
            resolutionFactory: XcmTransferResolutionFactory(
                chainRegistry: chainRegistry,
                paraIdOperationFactory: ParaIdOperationFactory(
                    chainRegistry: chainRegistry,
                    operationQueue: operationQueue
                )
            ),
            xcmTransfers: xcmTransfers,
            executionTimeEstimator: timeEstimator,
            fungibilityPreservationProvider: fungibilityPreservationProvider,
            tokensDepositMatchingFactory: tokenDepositMatcherFactory,
            balanceDetectionFactory: balanceChangeDetector,
            chainsWithExpensiveCrosschain: chainsWithExpensiveCrosschain,
            operationQueue: operationQueue,
            logger: logger
        )

        let exchange = CrosschainAssetsExchange(host: host)

        updateState(with: [exchange])
    }
}
