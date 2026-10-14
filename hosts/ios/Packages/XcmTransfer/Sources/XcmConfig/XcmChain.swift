import Foundation
import SubstrateSdk

struct XcmChain: Decodable {
    let chainId: ChainId
    let assets: [XcmAsset]
}
