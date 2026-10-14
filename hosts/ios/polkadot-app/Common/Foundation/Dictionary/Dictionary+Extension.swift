import Foundation
import SubstrateSdk
import ChainRegistry

extension Dictionary where Key == ChainModel.Id {
    subscript<T>(chainAssetId: ChainAssetId) -> T? where Value == [AssetModel.Id: T] {
        get {
            self[chainAssetId.chainId]?[chainAssetId.assetId]
        }
        set {
            var innerDict = self[chainAssetId.chainId, default: [:]]
            innerDict[chainAssetId.assetId] = newValue
            self[chainAssetId.chainId] = innerDict
        }
    }
}
