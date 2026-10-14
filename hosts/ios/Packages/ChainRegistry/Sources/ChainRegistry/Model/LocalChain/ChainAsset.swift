import Foundation
import Operation_iOS

public struct ChainAsset: Equatable, Hashable {
    public let chain: ChainModel
    public let asset: AssetModel

    public init(chain: ChainModel, asset: AssetModel) {
        self.chain = chain
        self.asset = asset
    }
}

public extension ChainAsset {
    var isUtilityAsset: Bool {
        chain.utilityAsset()?.assetId == asset.assetId
    }
}
