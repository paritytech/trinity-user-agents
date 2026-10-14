import Foundation
import SubstrateSdk

public struct XcmAssetTransfer: Decodable {
    public let destination: XcmAssetTransfer.Destination
    public let type: XcmCallType

    public init(destination: XcmAssetTransfer.Destination, type: XcmCallType) {
        self.destination = destination
        self.type = type
    }
}

public extension XcmAssetTransfer {
    struct Destination: Decodable {
        public let chainId: ChainId
        public let assetId: AssetId
        public let fee: XcmAssetTransferFee

        public init(chainId: ChainId, assetId: AssetId, fee: XcmAssetTransferFee) {
            self.chainId = chainId
            self.assetId = assetId
            self.fee = fee
        }
    }
}
