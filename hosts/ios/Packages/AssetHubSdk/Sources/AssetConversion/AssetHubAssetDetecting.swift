import Foundation
import SubstrateSdk

public protocol AssetHubFeeReporting {
    func canPayFee(using asset: ChainAssetProtocol) -> Bool
}

public class AssetHubWhitelistFeeReporter {
    public enum Mode {
        case all
        case concrete(Set<ChainAssetId>)
    }

    let mode: Mode

    public init(mode: Mode) {
        self.mode = mode
    }
}

extension AssetHubWhitelistFeeReporter: AssetHubFeeReporting {
    public func canPayFee(using asset: ChainAssetProtocol) -> Bool {
        switch mode {
        case .all:
            true
        case let .concrete(assetIds):
            assetIds.contains(asset.chainAssetId)
        }
    }
}
