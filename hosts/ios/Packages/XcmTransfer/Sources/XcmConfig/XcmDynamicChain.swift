import Foundation
import SubstrateSdk

struct XcmDynamicChain: Decodable {
    let chainId: ChainId
    let assets: [XcmDynamicAsset]
}
