import Foundation
import SubstrateSdk

extension XcmLegacyTransfers: XcmTransfersProtocol {
    func getChains() -> [XcmTransferChainProtocol] {
        chains
    }
}

extension XcmChain: XcmTransferChainProtocol {
    func getAssets() -> [XcmTransferAssetProtocol] {
        assets
    }
}

extension XcmAsset: XcmTransferAssetProtocol {
    func getDestinations() -> [XcmTransferDestinationProtocol] {
        xcmTransfers
    }
}

extension XcmAssetTransfer: XcmTransferDestinationProtocol {
    var chainId: ChainId {
        destination.chainId
    }

    var assetId: AssetId {
        destination.assetId
    }
}
