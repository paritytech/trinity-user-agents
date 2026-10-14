import Foundation
import SubstrateSdk

public protocol AssetHubBulkTokensMapperFactoryProtocol {
    func createBulkMapper(
        for chain: ChainProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> AssetHubBulkTokensMapping
}

public protocol AssetHubBulkTokensMapping {
    func convertPoolAsset(_ asset: AssetConversionPallet.PoolAsset) -> ChainAssetId?
}
