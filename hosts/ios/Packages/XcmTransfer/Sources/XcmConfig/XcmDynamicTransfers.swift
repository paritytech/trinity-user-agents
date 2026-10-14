import Foundation
import SubstrateSdk

public struct XcmDynamicTransfers: Decodable {
    let customTeleports: Set<XcmCustomTeleport>?
    let chains: [XcmDynamicChain]

    public func transfer(
        from chainAssetId: ChainAssetId,
        destinationChainId: ChainId
    ) -> XcmDynamicAssetTransfer? {
        guard
            let chain = chains.first(where: { $0.chainId == chainAssetId.chainId }),
            let xcmTransfers = chain.assets.first(where: { $0.assetId == chainAssetId.assetId })?.xcmTransfers else {
            return nil
        }

        return xcmTransfers.first { $0.chainId == destinationChainId }
    }

    public func getUsesCustomTeleport(from origin: ChainAssetId, destination: ChainId) -> Bool {
        guard let customTeleports else {
            return false
        }

        let model = XcmCustomTeleport(
            originChain: origin.chainId,
            destChain: destination,
            originAsset: origin.assetId
        )

        return customTeleports.contains(model)
    }
}
