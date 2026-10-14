import Foundation
import Operation_iOS
import Keystore_iOS
import SubstrateSdk
import AssetExchange
import AssetHubSdk
import ExtrinsicService
import ChainStore
import KeyDerivation
import ChainRegistry

final class AssetsHubExchangeProvider: AssetsExchangeBaseProvider {
    private var supportedChains: [ChainModel.Id: ChainModel]?
    private let selectedWallet: WalletManaging
    private let substrateStorageFacade: StorageFacadeProtocol
    private let exchangeStateRegistrar: AssetsExchangeStateRegistring
    private let extrinsicServiceFactory: ExtrinsicServiceCreating
    private let extrinsicSubmissionFacade: ExtrinsicSubmissionMonitorFacadeProtocol
    private let extrinsicOriginDefiningFactory: ExtrinsicOriginDefiningFactoryProtocol
    private let timeEstimator: AssetExchangeTimeEstimating
    private let monitoringChainRegistry: ChainRegistryProtocol

    private var hosts: [ChainModel.Id: AssetHubExchangeHostProtocol] = [:]

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
            syncQueue: DispatchQueue(label: "io.assethubexchangeprovider.\(UUID().uuidString)"),
            logger: logger
        )
    }

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

extension AssetsHubExchangeProvider {
    private func setupHost(for chain: ChainProtocol) throws -> AssetHubExchangeHostProtocol {
        if let host = hosts[chain.chainId] {
            return host
        }

        let extrinsicOperationFactory = try extrinsicServiceFactory.createOperationFactory(chain: chain)
        let submissionMonitorFactory = try extrinsicSubmissionFacade.createMonitorFactory(chain: chain)
        let originDefiner = try extrinsicOriginDefiningFactory.extrinsicOriginDefiner(
            from: selectedWallet,
            chain: chain
        )

        let account = try selectedWallet.fetchAccount(for: chain)
        let connection = try monitoringChainRegistry.getRpcConnectionOrError(for: chain.chainId)
        let runtimeService = try monitoringChainRegistry.getRuntimeCodingServiceOrError(for: chain.chainId)

        let tokenConverter = AssetHubTokenConverter()

        let flowState = AssetHubFlowState(
            connection: connection,
            runtimeProvider: runtimeService,
            notificationsRegistrar: exchangeStateRegistrar,
            operationQueue: operationQueue,
            logger: logger
        )

        let host = AssetHubExchangeHost(
            chain: chain,
            selectedAccount: account,
            flowState: flowState,
            submissionMonitorFactory: submissionMonitorFactory,
            extrinsicOperationFactory: extrinsicOperationFactory,
            originDefiner: originDefiner,
            runtimeService: runtimeService,
            connection: connection,
            executionTimeEstimator: timeEstimator,
            extrinsicConverting: AssetHubExtrinsicConverter(tokenConverter: tokenConverter),
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
                let host = try setupHost(for: chain)

                let tokenConverter = AssetHubTokenConverter()
                let connection = try monitoringChainRegistry.getRpcConnectionOrError(for: chain.chainId)
                let runtimeService = try monitoringChainRegistry.getRuntimeCodingServiceOrError(for: chain.chainId)

                let swapFactory = AssetHubSwapOperationFactory(
                    chain: chain,
                    runtimeService: runtimeService,
                    connection: connection,
                    tokenConverter: tokenConverter,
                    bulkMapperFactory: AssetHubBulkTokensMapperFactory(),
                    operationQueue: operationQueue
                )

                let exchange: AssetsExchangeProtocol = AssetsHubExchange(
                    host: host,
                    swapFactory: swapFactory
                )

                return [exchange]
            } catch {
                logger.warning("AssetsHubExchange unavailable for \(chain.chainId)")
                return []
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
}
