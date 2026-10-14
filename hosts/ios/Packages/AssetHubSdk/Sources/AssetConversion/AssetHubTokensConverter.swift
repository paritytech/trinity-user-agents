import Foundation
import SubstrateSdk

public protocol AssetHubTokenConverting {
    func convertToMultilocation(
        chainAsset: ChainAssetProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) -> AssetConversionPallet.AssetId?

    func convertFromMultilocation(
        _ assetId: AssetConversionAssetIdProtocol,
        chain: ChainProtocol
    ) -> AssetConversionPallet.PoolAsset?
}

public extension AssetHubTokenConverting {
    func convertToMultilocation(
        chainAssetId: ChainAssetId,
        chain: ChainProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) -> AssetConversionPallet.AssetId? {
        guard
            chain.chainId == chainAssetId.chainId,
            let localAsset = chain.chainAssetInterface(for: chainAssetId.assetId) else {
            return nil
        }

        return convertToMultilocation(chainAsset: localAsset, codingFactory: codingFactory)
    }
}
