import Foundation
import SubstrateSdk

struct XcmDynamicAsset: Decodable {
    let assetId: AssetId
    let xcmTransfers: [XcmDynamicAssetTransfer]
}
