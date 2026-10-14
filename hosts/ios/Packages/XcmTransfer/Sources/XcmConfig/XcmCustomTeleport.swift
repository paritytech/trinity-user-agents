import Foundation
import SubstrateSdk

struct XcmCustomTeleport: Equatable, Hashable, Decodable {
    let originChain: ChainId
    let destChain: ChainId
    let originAsset: AssetId
}
