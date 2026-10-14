import Foundation
import BigInt
import SubstrateSdk

public struct XcmTransferRequest {
    public let unweighted: XcmUnweightedTransferRequest
    public let originFeeAsset: ChainAssetId?

    public init(
        unweighted: XcmUnweightedTransferRequest,
        originFeeAsset: ChainAssetId? = nil
    ) {
        self.unweighted = unweighted
        self.originFeeAsset = originFeeAsset
    }
}
