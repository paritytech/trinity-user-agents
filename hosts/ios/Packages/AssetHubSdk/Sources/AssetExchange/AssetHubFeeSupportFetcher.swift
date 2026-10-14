import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

public final class AssetHubExchangeFeeSupportFetcher {
    let swapOperationFactory: AssetHubSwapOperationFactoryProtocol
    let chain: ChainProtocol

    public init(
        chain: ChainProtocol,
        swapOperationFactory: AssetHubSwapOperationFactoryProtocol
    ) {
        self.chain = chain
        self.swapOperationFactory = swapOperationFactory
    }
}

extension AssetHubExchangeFeeSupportFetcher: AssetExchangeFeeSupportFetching {
    public var identifier: String { "asset-hub-\(chain.chainId)" }

    public func createFeeSupportWrapper() -> CompoundOperationWrapper<AssetExchangeFeeSupporting> {
        guard let utilityAssetId = chain.utilityChainAssetId() else {
            return .createWithError(ChainError.noUtilityAsset)
        }

        let availableDirectionsWrapper = swapOperationFactory.availableDirections()

        let mappingOperation = ClosureOperation<AssetExchangeFeeSupporting> {
            let availableDirections = try availableDirectionsWrapper.targetOperation.extractNoCancellableResultData()

            let supportedAssetIds = availableDirections
                .filter { $0.value.contains(utilityAssetId) }
                .keys

            return AssetExchangeFeeSupport(supportedAssets: Set(supportedAssetIds))
        }

        mappingOperation.addDependency(availableDirectionsWrapper.targetOperation)

        return availableDirectionsWrapper.insertingTail(operation: mappingOperation)
    }
}
