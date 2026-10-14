import Foundation
import Operation_iOS
import SubstrateSdk

public extension ChainRegistryProtocol {
    func fetchChainAsset(assetId: ChainAssetId) -> CompoundOperationWrapper<ChainAsset?> {
        let chainId = assetId.chainId
        let wrapper = asyncChains(for: [chainId])
        let mappingOperation = ClosureOperation<ChainAsset?> {
            let dictionary = try wrapper.targetOperation.extractNoCancellableResultData()
            guard let chainModel = dictionary[chainId] else {
                return nil
            }

            return chainModel.chainAsset(for: assetId.assetId)
        }
        mappingOperation.addDependency(wrapper.targetOperation)

        return wrapper.insertingTail(operation: mappingOperation)
    }

    func fetchChainAssets(assetIDs: Set<ChainAssetId>) -> CompoundOperationWrapper<[ChainAssetId: ChainAsset]> {
        let chainIDs = Set(assetIDs.map(\.chainId))
        let wrapper = asyncChains(for: chainIDs)

        let mappingOperation = ClosureOperation<[ChainAssetId: ChainAsset]> {
            let chainMapping = try wrapper.targetOperation.extractNoCancellableResultData()
            return assetIDs.reduce(into: [ChainAssetId: ChainAsset]()) {
                guard let chainModel = chainMapping[$1.chainId] else { return }
                $0[$1] = chainModel.chainAsset(for: $1.assetId)
            }
        }
        mappingOperation.addDependency(wrapper.targetOperation)

        return wrapper.insertingTail(operation: mappingOperation)
    }
}
