import Foundation
import SubstrateSdk
import AssetExchange
import XcmTransfer
import ChainRegistry

final class AssetFungibilityPreservationProvider {
    let allAssets: Set<ChainModel.Id>
    let concreteAssets: Set<ChainAssetId>

    convenience init() {
        self.init(allAssets: [], concreteAssets: [])
    }

    init(allAssets: Set<ChainModel.Id>, concreteAssets: Set<ChainAssetId>) {
        self.allAssets = allAssets
        self.concreteAssets = concreteAssets
    }
}

extension AssetFungibilityPreservationProvider: AssetFungibilityPreservationProviding {
    func requiresPreservationForCrosschain(
        assetIn: ChainAssetId,
        features: XcmTransferFeatures
    ) -> Bool {
        // xcm execute allows to bypass keep alive requirements
        guard !features.shouldUseXcmExecute else {
            return false
        }

        let requiresKeepAlive = allAssets.contains(assetIn.chainId) ||
            concreteAssets.contains(assetIn)

        return requiresKeepAlive
    }
}
