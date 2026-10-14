import Foundation
import SubstrateSdk

public struct XcmDynamicAssetTransfer: Decodable {
    public let chainId: ChainId
    public let assetId: AssetId
    public let hasDeliveryFee: Bool?
    public let supportsXcmExecute: Bool?

    public init(
        chainId: ChainId,
        assetId: AssetId,
        hasDeliveryFee: Bool?,
        supportsXcmExecute: Bool?
    ) {
        self.chainId = chainId
        self.assetId = assetId
        self.hasDeliveryFee = hasDeliveryFee
        self.supportsXcmExecute = supportsXcmExecute
    }
}
