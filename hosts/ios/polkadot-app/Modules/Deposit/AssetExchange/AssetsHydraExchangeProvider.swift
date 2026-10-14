import Foundation
import Operation_iOS
import Keystore_iOS
import SubstrateSdk
import AssetExchange
import HydrationSdk
import ExtrinsicService
import ChainStore
import KeyDerivation
import ChainRegistry

final class AssetsHydraExchangeProvider: AssetsExchangeBaseProvider {
    private var supportedChains: [ChainModel.Id: ChainModel]?
    let selectedWallet: WalletManaging
    let substrateStorageFacade: StorageFacadeProtocol
    let exchangeStateRegistrar: AssetsExchangeStateRegistring
    let extrinsicServiceFactory: ExtrinsicServiceCreating
    let extrinsicSubmissionFacade: ExtrinsicSubmissionMonitorFacadeProtocol
    let extrinsicOriginDefiningFactory: ExtrinsicOriginDefiningFactoryProtocol
    let timeEstimator: AssetExchangeTimeEstimating
    let monitoringChainRegistry: ChainRegistryProtocol

    private var hosts: [ChainModel.Id: HydraExchangeHostProtocol] = [:]

    let supportedChainIds: Set<ChainId>

    init(
        selectedWallet: WalletManaging,
        supportedChainIds: Set<ChainId>,
        chainRegistry: ChainRegistryProtocol,
        pathCostEstimator: AssetsExchangePathCostEstimating,
        substrateStorageFacade: StorageFacadeProtocol,
        exchangeStateRegistrar: AssetsExchangeStateRegistring,
        feeBufferInPercentage: BigRational,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.selectedWallet = selectedWallet
        self.substrateStorageFacade = substrateStorageFacade
        self.exchangeStateRegistrar = exchangeStateRegistrar
        self.supportedChainIds = supportedChainIds
        monitoringChainRegistry = chainRegistry

        let graphProxy = AssetExchangeGraphProxy(
            pathCostEstimator: pathCostEstimator,
            operationQueue: operationQueue,
            logger: logger
        )

        let customFeeEstimatingFactory = AssetExchangeFeeEstimatingFactory(
            graphProxy: graphProxy,
            operationQueue: operationQueue,
            feeBufferInPercentage: feeBufferInPercentage
        )

        extrinsicServiceFactory = ExtrinsicServiceFactory(
            chainRegistry: chainRegistry,
            substrateStorageFacade: substrateStorageFacade,
            customFeeEstimator: customFeeEstimatingFactory,
            transactionExtensionFactory: CompoundTxExtensionFactory(),
            extrinsicVersion: .V4,
            operationQueue: operationQueue
        )

        extrinsicSubmissionFacade = ExtrinsicSubmissionMonitorFacade(
            extrinsicServiceFactory: extrinsicServiceFactory,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )

        extrinsicOriginDefiningFactory = SignedExtrinsicOriginFactory(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )

        timeEstimator = AssetExchangeTimeEstimator(chainRegistry: chainRegistry)

        super.init(
            chainRegistry: chainRegistry,
            graphProxy: graphProxy,
            operationQueue: operationQueue,
            syncQueue: DispatchQueue(label: "io.hydraexchangeprovider.\(UUID().uuidString)"),
            logger: logger
        )
    }

    private func setupHost(
        for chain: ChainProtocol
    ) throws -> HydraExchangeHostProtocol {
        if let host = hosts[chain.chainId] {
            return host
        }

        let extrinsicOperationFactory = try extrinsicServiceFactory.createOperationFactory(
            chain: chain
        )

        let submissionMonitorFactory = try extrinsicSubmissionFacade.createMonitorFactory(
            chain: chain
        )

        let originDefiner = try extrinsicOriginDefiningFactory.extrinsicOriginDefiner(
            from: selectedWallet,
            chain: chain
        )

        let account = try selectedWallet.fetchAccount(for: chain)
        let connection = try chainRegistry.getRpcConnectionOrError(for: chain.chainId)
        let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: chain.chainId)

        let host = HydraExchangeHost(
            chain: chain,
            selectedAccount: account,
            submissionMonitorFactory: submissionMonitorFactory,
            extrinsicOperationFactory: extrinsicOperationFactory,
            referralCode: nil,
            runtimeService: runtimeService,
            connection: connection,
            originDefiner: originDefiner,
            executionTimeEstimator: timeEstimator,
            tokenConverting: HydrationTokenConverter(),
            operationQueue: operationQueue,
            logger: logger
        )

        hosts[chain.chainId] = host

        return host
    }

    private func updateStateIfNeeded() {
        guard let supportedChains else {
            return
        }

        let exchanges: [AssetsExchangeProtocol] = supportedChains.values.flatMap { chain in
            do {
                let swapHost = try setupHost(for: chain)

                let omnipoolExchange: AssetsExchangeProtocol = AssetsHydraOmnipoolExchange(
                    host: swapHost,
                    exchangeStateRegistrar: exchangeStateRegistrar
                )

                let stableswapExchange: AssetsExchangeProtocol = AssetsHydraStableswapExchange(
                    host: swapHost,
                    exchangeStateRegistrar: exchangeStateRegistrar
                )

                let xykExchange: AssetsExchangeProtocol = AssetsHydraXYKExchange(
                    host: swapHost,
                    exchangeStateRegistrar: exchangeStateRegistrar
                )

                let aaveExchange: AssetsExchangeProtocol = AssetsHydraAaveExchange(
                    host: swapHost,
                    exchangeStateRegistrar: exchangeStateRegistrar
                )

                return [omnipoolExchange, stableswapExchange, xykExchange, aaveExchange]
            } catch {
                logger.warning("Account or connection/runtime unavailable for \(chain.name)")
                return [AssetsExchangeProtocol]()
            }
        }

        updateState(with: exchanges)
    }

    private func handleChains(
        changes: [DataProviderChange<ChainModel>],
        supportedChainIds: Set<ChainId>
    ) -> Bool {
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

    // MARK: Subsclass

    override func performSetup() {
        monitoringChainRegistry.chainsSubscribe(
            self,
            runningInQueue: syncQueue
        ) { [weak self] changes in
            guard
                let self,
                handleChains(
                    changes: changes,
                    supportedChainIds: supportedChainIds
                ) else {
                return
            }

            updateStateIfNeeded()
        }
    }

    override func performThrottle() {
        monitoringChainRegistry.chainsUnsubscribe(self)
    }
}
