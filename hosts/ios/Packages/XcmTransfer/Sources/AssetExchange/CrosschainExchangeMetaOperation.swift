import Foundation
import SubstrateSdk
import AssetExchange

final class CrosschainExchangeMetaOperation: AssetExchangeBaseMetaOperation {
    let requiresOriginAccountKeepAlive: Bool

    init(
        assetIn: ChainAssetProtocol,
        assetOut: ChainAssetProtocol,
        amountIn: Balance,
        amountOut: Balance,
        requiresOriginAccountKeepAlive: Bool
    ) {
        self.requiresOriginAccountKeepAlive = requiresOriginAccountKeepAlive

        super.init(
            assetIn: assetIn,
            assetOut: assetOut,
            amountIn: amountIn,
            amountOut: amountOut
        )
    }
}

extension CrosschainExchangeMetaOperation: AssetExchangeMetaOperationProtocol {
    var label: AssetExchangeMetaOperationLabel { .transfer }
}
