import Foundation
import SubstrateSdk

protocol XcmTransfersProtocol {
    func getChains() -> [XcmTransferChainProtocol]
}

protocol XcmTransferChainProtocol {
    var chainId: ChainId { get }
    func getAssets() -> [XcmTransferAssetProtocol]
}

protocol XcmTransferAssetProtocol {
    var assetId: AssetId { get }
    func getDestinations() -> [XcmTransferDestinationProtocol]
}

protocol XcmTransferDestinationProtocol {
    var chainId: ChainId { get }
    var assetId: AssetId { get }
    var type: XcmCallType { get }
}
