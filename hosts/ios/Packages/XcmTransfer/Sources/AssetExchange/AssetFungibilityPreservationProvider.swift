import Foundation
import SubstrateSdk

public protocol AssetFungibilityPreservationProviding {
    func requiresPreservationForCrosschain(
        assetIn: ChainAssetId,
        features: XcmTransferFeatures
    ) -> Bool
}
