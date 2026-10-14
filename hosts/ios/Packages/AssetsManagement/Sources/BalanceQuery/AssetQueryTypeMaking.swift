import SubstrateSdk

public protocol AssetQueryTypeMaking {
    func deriveQueryType(_ chainAsset: ChainAssetProtocol) -> AssetQueryType?
}
