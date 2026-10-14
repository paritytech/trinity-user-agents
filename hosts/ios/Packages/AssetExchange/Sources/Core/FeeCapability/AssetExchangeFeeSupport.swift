import Foundation
import SubstrateSdk

public struct AssetExchangeFeeSupport {
    public let supportedAssets: Set<ChainAssetId>

    public init(supportedAssets: Set<ChainAssetId>) {
        self.supportedAssets = supportedAssets
    }
}

extension AssetExchangeFeeSupport: AssetExchangeFeeSupporting {
    public func canPayFee(inNonNative chainAssetId: ChainAssetId) -> Bool {
        supportedAssets.contains(chainAssetId)
    }
}

public struct CompoundAssetExchangeFeeSupport {
    let supporters: [AssetExchangeFeeSupporting]

    public init(supporters: [AssetExchangeFeeSupporting]) {
        self.supporters = supporters
    }
}

extension CompoundAssetExchangeFeeSupport: AssetExchangeFeeSupporting {
    public func canPayFee(inNonNative chainAssetId: ChainAssetId) -> Bool {
        supporters.contains { $0.canPayFee(inNonNative: chainAssetId) }
    }
}
