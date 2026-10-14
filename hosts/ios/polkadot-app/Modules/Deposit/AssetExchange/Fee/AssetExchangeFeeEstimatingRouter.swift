import Foundation
import Foundation_iOS
import SubstrateSdk
import AssetExchange
import AssetHubSdk
import ExtrinsicService
import ChainRegistry

final class AssetExchangeFeeEstimatingRouter {
    let graphBasedFactory: ExtrinsicCustomFeeEstimatingFactoryProtocol
    let dependencies: AssetExchangeFeeEstimatingRouter.Dependencies
    let feeBufferInPercentage: BigRational
    let hydrationChainId: ChainId

    // cache factories to optimize fee calc for multi attempts
    private let conversionCache: InMemoryCache<ChainModel.Id, ExtrinsicFeeEstimatorProviding> = .init()

    init(
        graphProxy: AssetQuoteFactoryProtocol,
        hydrationChainId: ChainId,
        dependencies: AssetExchangeFeeEstimatingRouter.Dependencies,
        feeBufferInPercentage: BigRational
    ) {
        graphBasedFactory = AssetExchangeFeeEstimatingFactory(
            graphProxy: graphProxy,
            operationQueue: dependencies.operationQueue,
            feeBufferInPercentage: feeBufferInPercentage
        )

        self.hydrationChainId = hydrationChainId

        self.dependencies = dependencies
        self.feeBufferInPercentage = feeBufferInPercentage
    }
}

private extension AssetExchangeFeeEstimatingRouter {
    func canSwapViaGraph(chainAsset: ChainAssetProtocol) -> Bool {
        hydrationChainId == chainAsset.chainInterface.chainId
    }

    func routeViaGraph(chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        dependencies.logger.debug("Using graph factory for chain \(chainAsset.chainInterface.name)")

        return graphBasedFactory.createCustomFeeEstimator(for: chainAsset)
    }

    func routeViaConversion(chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        do {
            let factory: ExtrinsicFeeEstimatorProviding
            let chain = chainAsset.chainInterface
            let chainId = chain.chainId

            if let estimatorFactory = conversionCache.fetchValue(for: chainId) {
                factory = estimatorFactory

                dependencies.logger.debug("Using conversion cache for chain \(chain.name)")
            } else {
                let connection = try dependencies.chainRegistry.getConnectionOrError(for: chainId)
                let runtimeProvider = try dependencies.chainRegistry.getRuntimeProviderOrError(for: chainId)

                factory = AssetHubFeeEstimatorProvider(
                    host: ExtrinsicFeeEstimatorHost(
                        chain: chain,
                        connection: connection,
                        runtimeProvider: runtimeProvider,
                        operationQueue: dependencies.operationQueue,
                        logger: dependencies.logger
                    ),
                    feeBufferInPercentage: feeBufferInPercentage,
                    tokenConverter: AssetHubTokenConverter(),
                    tokensBulkMapperFactory: AssetHubBulkTokensMapperFactory(),
                    feeReporter: AssetHubWhitelistFeeReporter(mode: .all),
                    logger: dependencies.logger
                )

                conversionCache.store(value: factory, for: chainId)

                dependencies.logger.debug("New factory for chain \(chain.name)")
            }

            return factory.provideFeeEstimator(for: chainAsset)
        } catch {
            dependencies.logger.error("Unexpected error: \(error)")

            return nil
        }
    }
}

extension AssetExchangeFeeEstimatingRouter: ExtrinsicCustomFeeEstimatingFactoryProtocol {
    func createCustomFeeEstimator(for chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        // swaps might be turned off on the chain
        if canSwapViaGraph(chainAsset: chainAsset) {
            routeViaGraph(chainAsset: chainAsset)
        } else {
            routeViaConversion(chainAsset: chainAsset)
        }
    }
}

extension AssetExchangeFeeEstimatingRouter {
    struct Dependencies {
        let wallet: MetaAccountModelProtocol
        let chainRegistry: ChainRegistryProtocol
        let operationQueue: OperationQueue
        let logger: LoggerProtocol
    }
}
