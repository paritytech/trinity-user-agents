import Foundation
import SubstrateSdk

public struct XcmTransferOrigin {
    public let chainAsset: ChainAssetProtocol
    public let parachainId: ParaId?

    public init(chainAsset: ChainAssetProtocol, parachainId: ParaId?) {
        self.chainAsset = chainAsset
        self.parachainId = parachainId
    }
}
