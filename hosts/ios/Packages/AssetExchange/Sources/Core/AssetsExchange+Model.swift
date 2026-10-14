import Foundation
import SubstrateSdk

extension AssetsExchange {
    struct ChainAssetId: Equatable, Hashable {
        let chainId: ChainId
        let assetId: AssetId
    }
}
