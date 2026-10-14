import Foundation
import SubstrateSdk

public struct AssetExchangeAtomicOperationArgs {
    public let swapLimit: AssetExchangeSwapLimit
    public let feeAsset: ChainAssetId

    public init(swapLimit: AssetExchangeSwapLimit, feeAsset: ChainAssetId) {
        self.swapLimit = swapLimit
        self.feeAsset = feeAsset
    }
}
