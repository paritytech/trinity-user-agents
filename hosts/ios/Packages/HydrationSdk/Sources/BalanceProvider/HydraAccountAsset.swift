import Foundation
import SubstrateSdk

struct HydraAccountAsset: Equatable, Hashable {
    let accountId: AccountId
    let assetId: HydraDx.AssetId
}
