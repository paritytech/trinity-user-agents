import Foundation
import SubstrateSdk
import BigInt

public protocol HydrationTokenConverting {
    func convertToRemoteLocalMapping(
        remoteAssets: Set<HydraDx.AssetId>,
        chain: ChainProtocol,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> [HydraDx.AssetId: ChainAssetId]

    func convertToRemote(
        chainAsset _: ChainAssetProtocol,
        codingFactory _: RuntimeCoderFactoryProtocol
    ) throws -> HydraDx.LocalRemoteAssetId
}
